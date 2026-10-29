//! Document, component and semantic field addresses.
use super::{AtomicU64, ComponentId, Error, IdentityError, IdentityScope, Ordering, ParameterId};

/// The process-local identity of one editor document. Identities are never
/// reused, so a callback or task that captured one cannot address another
/// document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub(super) u64);

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

impl DocumentId {
    pub(super) fn allocate() -> Result<Self, Error> {
        NEXT_DOCUMENT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(Self)
            .map_err(|_| Error::Identity(IdentityError::Exhausted(IdentityScope::Document)))
    }
}

/// Opaque process-local identity of a rendered document or component lifetime.
/// A restored component retains its authored ID but receives a different row key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowKey {
    pub(super) document: DocumentId,
    pub(super) component: Option<ComponentId>,
    pub(super) generation: u64,
}
impl RowKey {
    pub(super) fn for_document(document: DocumentId) -> Self {
        Self {
            document,
            component: None,
            generation: 0,
        }
    }
    pub(super) fn for_component(
        document: DocumentId,
        component: ComponentId,
        generation: u64,
    ) -> Self {
        Self {
            document,
            component: Some(component),
            generation,
        }
    }
    /// Same document and authored owner, ignoring interpretation generations.
    /// Native reconciliation may retain widgets by owner; action admission must still compare the
    /// full key.
    pub fn same_owner(self, other: Self) -> bool {
        self.document == other.document && self.component == other.component
    }
    /// Bind a semantic parameter to this component lifetime.
    #[must_use]
    pub fn parameter(self, parameter: ParameterId) -> Option<FieldHandle> {
        Some(FieldHandle {
            row: self,
            field: EditingField::Parameter(self.component?, parameter),
        })
    }
    /// Bind metadata to this document lifetime; component keys cannot address metadata.
    #[must_use]
    pub fn metadata(self, field: MetadataField) -> Option<FieldHandle> {
        self.component.is_none().then_some(FieldHandle {
            row: self,
            field: EditingField::Metadata(field),
        })
    }
}
impl std::fmt::Display for RowKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:x}:{:x}:{:x}",
            self.document.0,
            self.component.map_or(0, ComponentId::get),
            self.generation
        )
    }
}
impl std::str::FromStr for RowKey {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        const INVALID: &str = "invalid editor row key";
        if text.len() > 42 {
            return Err(INVALID);
        }
        let parse = |part: &str| {
            if part.is_empty()
                || part.len() > 1 && part.starts_with('0')
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(INVALID);
            }
            u64::from_str_radix(part, 16).map_err(|_| INVALID)
        };
        let mut parts = text.split(':');
        let document = parse(parts.next().ok_or(INVALID)?)?;
        let component = u32::try_from(parse(parts.next().ok_or(INVALID)?)?).map_err(|_| INVALID)?;
        let generation = parse(parts.next().ok_or(INVALID)?)?;
        if document == 0 || parts.next().is_some() || component == 0 && generation != 0 {
            return Err(INVALID);
        }
        Ok(Self {
            document: DocumentId(document),
            component: ComponentId::new(component),
            generation,
        })
    }
}

/// A recipe-wide quantity or text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MetadataField {
    /// The document title.
    Title,
    /// The document's own notes.
    Summary,
    /// The finite length of saved and exported audio.
    Duration,
    /// The edge fade of saved and exported audio.
    Fade,
}

impl MetadataField {
    /// Every metadata field in display order.
    pub const ALL: [Self; 4] = [Self::Title, Self::Summary, Self::Duration, Self::Fade];

    /// The numeric quantity behind the field, if it is one.
    #[must_use]
    pub const fn parameter(self) -> Option<ParameterId> {
        match self {
            Self::Duration => Some(ParameterId::Duration),
            Self::Fade => Some(ParameterId::Fade),
            Self::Title | Self::Summary => None,
        }
    }
}

/// A field of a document, addressed by meaning rather than position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EditingField {
    /// A component's quantity.
    Parameter(ComponentId, ParameterId),
    /// A recipe-wide field.
    Metadata(MetadataField),
}

/// A field of one rendered document/component lifetime. Commands carry the
/// captured key, so a late callback cannot edit a replacement document or a
/// component that has been removed and restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldHandle {
    /// The rendered document or component lifetime the field belongs to.
    pub row: RowKey,
    /// The field within that document.
    pub field: EditingField,
}

impl std::fmt::Display for FieldHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.field {
            EditingField::Parameter(_, parameter) => {
                write!(f, "{}/p/{}", self.row, parameter.key())
            }
            EditingField::Metadata(field) => {
                let key = match field {
                    MetadataField::Title => "title",
                    MetadataField::Summary => "summary",
                    MetadataField::Duration => "duration",
                    MetadataField::Fade => "fade",
                };
                write!(f, "{}/m/{key}", self.row)
            }
        }
    }
}
impl std::str::FromStr for FieldHandle {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        const INVALID: &str = "invalid editor field handle";
        let mut parts = text.split('/');
        let row: RowKey = parts.next().ok_or(INVALID)?.parse()?;
        let kind = parts.next().ok_or(INVALID)?;
        let key = parts.next().ok_or(INVALID)?;
        if parts.next().is_some() {
            return Err(INVALID);
        }
        match kind {
            "p" => row.parameter(key.parse().map_err(|_| INVALID)?),
            "m" => row.metadata(match key {
                "title" => MetadataField::Title,
                "summary" => MetadataField::Summary,
                "duration" => MetadataField::Duration,
                "fade" => MetadataField::Fade,
                _ => return Err(INVALID),
            }),
            _ => None,
        }
        .ok_or(INVALID)
    }
}
