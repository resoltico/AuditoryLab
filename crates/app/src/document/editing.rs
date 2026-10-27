//! Editable recipe documents, staged input and candidate evaluation.
//!
//! An [`EditorSession`] owns one document: its committed [`Draft`], the text a
//! person has typed but not committed, the saved-content baseline and the
//! document's identity. Staging never changes committed values. Committing
//! parses the whole candidate at once: if any staged field does not parse, or
//! any numeric value of the complete candidate is not finite, nothing is
//! committed and every staged field is kept. A fully parsed finite candidate
//! is committed even when it is not acoustically valid, so it can be saved and
//! repaired. Audio sessions own accepted and generated targets; the editor
//! only supplies candidates.
use crate::document::controls;
use crate::document::recipe::{Origin, RecipeSnapshot};
use crate::error::{EditorError as Error, IdentityError, IdentityScope};
use crate::presentation::ParameterText;
use auditorylab_core::{
    CarrierKind, ComponentId, Ears, ModulationKind, ParameterId, Recipe, RecipeBuilder, SampleRate,
    Signal, Sound, SoundError, Target, TargetCheck, Voice,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// The process-local identity of one editor document. Identities are never
/// reused, so a callback or task that captured one cannot address another
/// document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(u64);

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);

impl DocumentId {
    fn allocate() -> Result<Self, Error> {
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
    document: DocumentId,
    component: Option<ComponentId>,
    generation: u64,
}
impl RowKey {
    fn for_document(document: DocumentId) -> Self {
        Self {
            document,
            component: None,
            generation: 0,
        }
    }
    fn for_component(document: DocumentId, component: ComponentId, generation: u64) -> Self {
        Self {
            document,
            component: Some(component),
            generation,
        }
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

/// Why an editing command was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusal {
    /// The command addresses a document that is no longer open.
    StaleDocument,
    /// The authored component exists, but this handle belongs to an older row lifetime.
    StaleRow,
    /// The component no longer exists.
    UnknownComponent,
    /// The component's carrier or modulation does not have the quantity.
    Inapplicable,
    /// Adding one component would exceed an authored or expanded budget.
    BudgetExceeded {
        /// Current authored component count.
        components: usize,
        /// Current expanded voice count.
        voices: usize,
        /// Expanded voices required by the component being added or restored.
        additional_voices: usize,
    },
    /// No unused component or row-lifetime identity remains in this document.
    IdentitiesExhausted,
    /// No removed component is waiting to be restored.
    NothingRemoved,
}

/// Why staged text is not a usable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProblem {
    /// The text is not a number.
    Unparseable,
    /// The value is not a finite number, whether typed or inherited.
    Nonfinite,
}

/// The candidate could not be committed; every staged field was kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRefusal {
    /// Every field that prevented the commit.
    pub problems: Vec<(EditingField, InputProblem)>,
}

/// An authored document with complete portable metadata and stable owner IDs.
/// Committed settings may be finite but acoustically invalid and remain repairable.
#[derive(Clone, Debug)]
pub struct Draft {
    content: RecipeBuilder,
    next_id: Option<u32>,
    based_on: Option<RecipeSnapshot>,
}
impl Draft {
    /// Create a provisional random identity for a silent sixty-second document.
    ///
    /// # Errors
    /// Returns an actionable error if secure identity generation fails.
    pub fn new() -> Result<Self, Error> {
        Ok(Self::empty(crate::identity::fresh_document()?))
    }
    fn empty(document: auditorylab_core::DocumentIdentity) -> Self {
        let mut content = Recipe::builder(document);
        content.description = Some(auditorylab_core::Description {
            title: crate::presentation::UNTITLED_SOUND.into(),
            notes: Some(String::new()),
        });
        Self {
            content,
            next_id: Some(1),
            based_on: None,
        }
    }
    /// Open editable owned content without changing its identity or declared dates.
    #[must_use]
    pub fn from_recipe(content: Recipe) -> Self {
        let content = content.into_builder();
        let next_id = content
            .sound
            .components
            .iter()
            .map(|c| c.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1);
        Self {
            content,
            next_id,
            based_on: None,
        }
    }
    /// Explicitly fork a snapshot; preserve all bibliography and flat scoped ancestry.
    ///
    /// # Errors
    /// Refuses random-identity failure or full ancestry capacity, dropping no history.
    pub fn from_snapshot(snapshot: &RecipeSnapshot) -> Result<Self, Error> {
        let catalog = match snapshot.origin() {
            Origin::BuiltIn(id) => Some((*id).into()),
            _ => None,
        };
        let content = snapshot
            .recipe()
            .fork(
                crate::identity::fresh_document()?,
                catalog,
                crate::presentation::recipe_text::copy_note(snapshot.recipe()),
            )
            .map_err(Error::Recipe)?;
        let mut draft = Self::from_recipe(content);
        draft.set_title(crate::presentation::edited_title(&snapshot.title()));
        draft.based_on = Some(snapshot.clone());
        Ok(draft)
    }
    /// Immediate in-memory ancestor snapshot, when this session made an explicit copy.
    #[must_use]
    pub const fn based_on(&self) -> Option<&RecipeSnapshot> {
        self.based_on.as_ref()
    }
    /// Authored components in source order, including stable owner identities.
    #[must_use]
    pub fn components(&self) -> &[auditorylab_core::Component] {
        &self.content.sound.components
    }
    /// One authored form by its primary owner identity.
    #[must_use]
    pub fn component(&self, id: ComponentId) -> Option<&auditorylab_core::ComponentForm> {
        self.content
            .sound
            .components
            .iter()
            .find(|c| c.id == id)
            .map(|c| &c.form)
    }
    fn component_mut(&mut self, id: ComponentId) -> Option<&mut auditorylab_core::ComponentForm> {
        self.content
            .sound
            .components
            .iter_mut()
            .find(|c| c.id == id)
            .map(|c| &mut c.form)
    }
    /// Add one default primitive, checking authored and expanded budgets.
    ///
    /// # Errors
    /// Refuses full budgets or exhausted nonreused owner identities.
    pub fn add(&mut self, kind: CarrierKind) -> Result<ComponentId, EditRefusal> {
        self.add_node(auditorylab_core::ComponentForm::Voice(
            controls::default_component(kind),
        ))
    }
    /// Check the authored and expanded capacity for one additional component.
    ///
    /// # Errors
    /// Reports current counts and the requested voice cost if either limit is exceeded.
    pub fn check_addition(&self, form: auditorylab_core::ComponentForm) -> Result<(), EditRefusal> {
        let components = self.components().len();
        let voices = self
            .components()
            .iter()
            .map(|c| c.form.voices())
            .sum::<usize>();
        let additional_voices = form.voices();
        if components >= Sound::MAX_COMPONENTS || voices + additional_voices > Sound::MAX_VOICES {
            Err(EditRefusal::BudgetExceeded {
                components,
                voices,
                additional_voices,
            })
        } else {
            Ok(())
        }
    }
    pub(crate) fn admit_new_component(
        &self,
        form: auditorylab_core::ComponentForm,
    ) -> Result<ComponentId, EditRefusal> {
        self.check_addition(form)?;
        self.next_id
            .and_then(ComponentId::new)
            .ok_or(EditRefusal::IdentitiesExhausted)
    }
    /// Add an atomic authored form, including a two-voice pair.
    ///
    /// # Errors
    /// Refuses full budgets or exhausted owner identities.
    pub fn add_node(
        &mut self,
        form: auditorylab_core::ComponentForm,
    ) -> Result<ComponentId, EditRefusal> {
        let id = self.admit_new_component(form)?;
        let components = &mut self.content.sound.components;
        self.next_id = id.get().checked_add(1);
        components.push(auditorylab_core::Component { id, form });
        self.demote_evidence();
        Ok(id)
    }
    /// Replace one primitive while preserving its authored owner.
    pub fn replace(&mut self, id: ComponentId, component: Voice) -> bool {
        let Some(before) = self.component(id).copied() else {
            return false;
        };
        let form = auditorylab_core::ComponentForm::Voice(component);
        if !before.same_content(form) {
            self.demote_evidence();
        }
        if let Some(value) = self.component_mut(id) {
            *value = form;
            true
        } else {
            false
        }
    }
    /// Remove one authored form without reusing its owner identity.
    pub fn remove(&mut self, id: ComponentId) -> Option<(usize, auditorylab_core::Component)> {
        let components = &mut self.content.sound.components;
        let index = components.iter().position(|c| c.id == id)?;
        let removed = components.remove(index);
        self.demote_evidence();
        Some((index, removed))
    }
    fn restore(
        &mut self,
        index: usize,
        component: auditorylab_core::Component,
    ) -> Result<(), EditRefusal> {
        self.check_addition(component.form)?;
        let components = &mut self.content.sound.components;
        components.insert(index.min(components.len()), component);
        self.demote_evidence();
        Ok(())
    }
    /// Current user-visible title; missing imported descriptions remain unmodified.
    #[must_use]
    pub fn title(&self) -> &str {
        self.content
            .description
            .as_ref()
            .map_or(crate::presentation::UNTITLED_SOUND, |d| d.title.as_str())
    }
    /// Set current title; a title edit does not change research scope.
    pub fn set_title(&mut self, title: String) {
        self.content
            .description
            .get_or_insert_with(|| auditorylab_core::Description {
                title: String::new(),
                notes: None,
            })
            .title = title;
    }
    /// Own notes, independent of attributed source findings.
    #[must_use]
    pub fn summary(&self) -> &str {
        self.content
            .description
            .as_ref()
            .and_then(|d| d.notes.as_deref())
            .unwrap_or("")
    }
    /// Set current own notes.
    pub fn set_summary(&mut self, summary: String) {
        if self.summary() == summary {
            return;
        }
        let title = self.title().into();
        self.content
            .description
            .get_or_insert(auditorylab_core::Description { title, notes: None })
            .notes = Some(summary);
    }
    /// Requested finite duration.
    #[must_use]
    pub const fn duration_seconds(&self) -> f64 {
        self.content.sound.duration_seconds
    }
    /// Set finite duration; invalid finite values remain editable for repair.
    pub fn set_duration_seconds(&mut self, value: f64) {
        if self.content.sound.duration_seconds.to_bits() != value.to_bits() {
            self.demote_evidence();
        }
        self.content.sound.duration_seconds = value;
    }
    /// Requested endpoint fade.
    #[must_use]
    pub const fn fade_seconds(&self) -> f64 {
        self.content.sound.fade_seconds
    }
    /// Set endpoint fade.
    pub fn set_fade_seconds(&mut self, value: f64) {
        if self.content.sound.fade_seconds.to_bits() != value.to_bits() {
            self.demote_evidence();
        }
        self.content.sound.fade_seconds = value;
    }
    /// Applicable semantic quantity, preserving exact binary64 values.
    #[must_use]
    pub fn value(&self, field: EditingField) -> Option<f64> {
        match field {
            EditingField::Parameter(id, p) => self.component(id)?.read(p),
            EditingField::Metadata(MetadataField::Duration) => Some(self.duration_seconds()),
            EditingField::Metadata(MetadataField::Fade) => Some(self.fade_seconds()),
            _ => None,
        }
    }
    fn set_value(&mut self, field: EditingField, value: f64) -> bool {
        match field {
            EditingField::Parameter(id, p) => {
                let Some(before) = self.component(id).and_then(|c| c.read(p)) else {
                    return false;
                };
                if before.to_bits() != value.to_bits() {
                    self.demote_evidence();
                }
                self.component_mut(id).is_some_and(|c| c.write(p, value))
            }
            EditingField::Metadata(MetadataField::Duration) => {
                self.set_duration_seconds(value);
                true
            }
            EditingField::Metadata(MetadataField::Fade) => {
                self.set_fade_seconds(value);
                true
            }
            _ => false,
        }
    }
    fn has_field(&self, field: EditingField) -> Result<(), EditRefusal> {
        match field {
            EditingField::Parameter(id, p) => self
                .component(id)
                .ok_or(EditRefusal::UnknownComponent)?
                .read(p)
                .map(|_| ())
                .ok_or(EditRefusal::Inapplicable),
            _ => Ok(()),
        }
    }
    /// Complete authored sound, with pair intent and stable owners retained.
    #[must_use]
    pub fn sound(&self) -> auditorylab_core::Sound {
        self.content.sound.clone()
    }
    /// Immutable full portable candidate. Rendition edits demote current-condition evidence.
    ///
    /// # Errors
    /// Returns the portable-contract failure for an incomplete editable candidate.
    pub fn recipe(&self) -> Result<Recipe, Error> {
        self.content.clone().build().map_err(Error::Recipe)
    }
    fn demote_evidence(&mut self) {
        for evidence in &mut self.content.evidence {
            evidence.scope = auditorylab_core::EvidenceScope::Ancestor;
        }
    }
    /// Capture full content under a new process-local snapshot identity.
    ///
    /// # Errors
    /// Refuses a candidate that is not a valid portable recipe.
    pub fn snapshot(&self) -> Result<RecipeSnapshot, Error> {
        Ok(RecipeSnapshot::new(
            Origin::Draft(self.title().into()),
            self.recipe()?,
        ))
    }
    /// Exact editable content equality, excluding portable history.
    #[must_use]
    pub fn content_matches(&self, recipe: &Recipe) -> bool {
        self.content.document.id == recipe.document().id
            && self
                .content
                .same_editable_content(&recipe.clone().into_builder())
    }
    /// Compile all expanded pair lanes as one atomic target candidate.
    #[must_use]
    pub fn check(&self, rate: SampleRate) -> TargetCheck {
        self.content.sound.check(rate)
    }
    /// Serialize a complete finite candidate without I/O.
    ///
    /// # Errors
    /// Rejects portable contract violations, including nonfinite values and capacity.
    pub fn to_toml(&self) -> Result<String, Error> {
        serialize(&self.recipe()?).map_err(Error::Recipe)
    }
}
/// Serialize finite recipe content within the reader's byte limit.
///
/// # Errors
/// Returns [`Outcome::InvalidRecipe`] for non-finite numbers, a writer
/// failure or text longer than [`Recipe::MAX_BYTES`].
pub(crate) fn serialize(recipe: &Recipe) -> Result<String, crate::core::RecipeError> {
    recipe.to_toml()
}

/// A complete or best-effort candidate: committed values with every staged
/// field applied that parses.
#[derive(Debug, Clone)]
pub struct Candidate {
    draft: Draft,
    problems: Vec<(EditingField, InputProblem)>,
}

impl Candidate {
    /// The candidate values. When [`Candidate::is_complete`] is false, fields
    /// with problems keep their committed values.
    #[must_use]
    pub const fn draft(&self) -> &Draft {
        &self.draft
    }

    /// Fields that do not parse and numeric values that are not finite,
    /// including inherited values that were never staged.
    #[must_use]
    pub fn problems(&self) -> &[(EditingField, InputProblem)] {
        &self.problems
    }

    /// Whether every staged field parses and every numeric value is finite.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.problems.is_empty()
    }
}

/// A candidate's evaluation at one explicit sample rate. Invalid results are
/// kept as results, so an unchanged invalid candidate is not evaluated again.
#[derive(Debug, Clone)]
pub struct Analysis {
    rate: SampleRate,
    check: TargetCheck,
    envelope: Result<(), SoundError>,
    signal: Option<Arc<Signal>>,
}

impl Analysis {
    fn new(draft: &Draft, rate: SampleRate) -> Self {
        let check = draft.check(rate);
        let envelope = Signal::check_envelope(rate, draft.duration_seconds(), draft.fade_seconds());
        let signal = match (&envelope, check.target()) {
            (Ok(()), Some(target)) => target
                .signal(draft.duration_seconds(), draft.fade_seconds())
                .ok()
                .map(Arc::new),
            _ => None,
        };
        Self {
            rate,
            check,
            envelope,
            signal,
        }
    }

    /// The rate of this evaluation.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// The components' one-pass check.
    #[must_use]
    pub const fn check(&self) -> &TargetCheck {
        &self.check
    }

    /// The live target, when the components form one.
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        self.check.target()
    }

    /// The finite duration and fade check.
    ///
    /// # Errors
    /// Returns the first duration or fade error.
    pub const fn envelope(&self) -> Result<(), &SoundError> {
        match &self.envelope {
            Ok(()) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// The first finite-signal error in the established order: envelope
    /// before components.
    #[must_use]
    pub fn finite_error(&self) -> Option<&SoundError> {
        self.envelope
            .as_ref()
            .err()
            .or_else(|| self.check.result().err())
    }

    /// The validated finite signal, when both components and envelope are valid.
    #[must_use]
    pub fn signal(&self) -> Option<&Arc<Signal>> {
        self.signal.as_ref()
    }
}

#[derive(Debug, Clone)]
struct AnalysisKey {
    duration: u64,
    fade: u64,
    components: Vec<auditorylab_core::Component>,
    rate: SampleRate,
}

impl AnalysisKey {
    fn new(draft: &Draft, rate: SampleRate) -> Self {
        Self {
            duration: draft.duration_seconds().to_bits(),
            fade: draft.fade_seconds().to_bits(),
            components: draft.components().to_vec(),
            rate,
        }
    }
}

impl PartialEq for AnalysisKey {
    fn eq(&self, other: &Self) -> bool {
        self.duration == other.duration
            && self.fade == other.fade
            && self.rate == other.rate
            && self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|(a, b)| a.id == b.id && a.form.same_content(b.form))
    }
}
impl Eq for AnalysisKey {}

/// Bounded memo of evaluations: the selected export rate, the live device
/// rate and their candidate and committed forms.
const CACHED_ANALYSES: usize = 4;

/// The most recently removed component, with the text that was staged for it.
#[derive(Debug)]
struct Removed {
    generation: u64,
    index: usize,
    id: ComponentId,
    component: auditorylab_core::Component,
    before: RecipeBuilder,
    after: RecipeBuilder,
    saved: Arc<Recipe>,
    staged: Vec<(EditingField, String)>,
}

/// One open document and its editing state.
#[derive(Debug)]
pub struct EditorSession {
    document: DocumentId,
    row_generations: BTreeMap<ComponentId, u64>,
    draft: Draft,
    staged: BTreeMap<EditingField, String>,
    draft_revision: u64,
    input_revision: u64,
    saved: Arc<Recipe>,
    saved_nominal: Option<crate::core::rhythm::Analysis>,
    nominal: Option<(u64, Arc<crate::core::rhythm::Analysis>)>,
    analyses: Vec<(AnalysisKey, Arc<Analysis>)>,
    candidate: Option<(u64, u64, Arc<Candidate>)>,
    save_check: Option<(u64, u64, Result<Arc<str>, SaveRefusal>)>,
    removed: Option<Removed>,
    #[cfg(test)]
    evaluations: u64,
    #[cfg(test)]
    serializations: u64,
}

impl EditorSession {
    /// Open a document. A new blank draft is clean; any other content is
    /// dirty until saved.
    ///
    /// # Errors
    /// Refuses invalid component owner identities or exhausted document identities.
    pub fn new(draft: Draft) -> Result<Self, Error> {
        let saved = Draft::empty(draft.content.document.clone()).recipe()?;
        auditorylab_core::Sound::check_structure(draft.components()).map_err(Error::Structure)?;
        let row_generations = draft
            .components()
            .iter()
            .map(|component| (component.id, 0))
            .collect();
        Ok(Self {
            document: DocumentId::allocate()?,
            row_generations,
            draft,
            staged: BTreeMap::new(),
            draft_revision: 0,
            input_revision: 0,
            saved: Arc::new(saved),
            saved_nominal: None,
            nominal: None,
            analyses: Vec::new(),
            candidate: None,
            save_check: None,
            removed: None,
            #[cfg(test)]
            evaluations: 0,
            #[cfg(test)]
            serializations: 0,
        })
    }

    /// Classification of committed values, cached independently of staged text and rate.
    pub fn nominal(&mut self) -> Arc<crate::core::rhythm::Analysis> {
        if let Some((revision, analysis)) = &self.nominal
            && *revision == self.draft_revision
        {
            return analysis.clone();
        }
        let analysis = Arc::new(crate::core::rhythm::nominal(self.draft.components()));
        self.nominal = Some((self.draft_revision, analysis.clone()));
        analysis
    }

    /// Nominal classification of the exact last confirmed save candidate.
    /// Later committed edits and staged text cannot change this baseline.
    #[must_use]
    pub const fn saved_nominal(&self) -> Option<&crate::core::rhythm::Analysis> {
        self.saved_nominal.as_ref()
    }

    /// The document's identity.
    #[must_use]
    pub const fn document(&self) -> DocumentId {
        self.document
    }

    /// The committed values.
    #[must_use]
    pub const fn draft(&self) -> &Draft {
        &self.draft
    }

    /// Increases when committed values change.
    #[must_use]
    pub const fn draft_revision(&self) -> u64 {
        self.draft_revision
    }

    /// Increases when staged input changes.
    #[must_use]
    pub const fn input_revision(&self) -> u64 {
        self.input_revision
    }

    /// The metadata/add/Undo row key for this document lifetime.
    #[must_use]
    pub fn document_key(&self) -> RowKey {
        RowKey::for_document(self.document)
    }

    /// A key for an active authored component's current rendered lifetime.
    #[must_use]
    pub fn component_key(&self, id: ComponentId) -> Option<RowKey> {
        self.draft.component(id)?;
        Some(RowKey::for_component(
            self.document,
            id,
            *self.row_generations.get(&id)?,
        ))
    }

    /// Bind a semantic field to the currently rendered lifetime.
    ///
    /// # Errors
    /// Refuses absent or inapplicable fields rather than minting a future-valid key.
    pub fn handle(&self, field: EditingField) -> Result<FieldHandle, EditRefusal> {
        self.draft.has_field(field)?;
        let row = match field {
            EditingField::Metadata(_) => self.document_key(),
            EditingField::Parameter(id, _) => self
                .component_key(id)
                .ok_or(EditRefusal::UnknownComponent)?,
        };
        Ok(FieldHandle { row, field })
    }

    fn admit(&self, document: DocumentId) -> Result<(), EditRefusal> {
        if document == self.document {
            Ok(())
        } else {
            Err(EditRefusal::StaleDocument)
        }
    }
    fn admit_row(&self, row: RowKey) -> Result<Option<ComponentId>, EditRefusal> {
        self.admit(row.document)?;
        if let Some(id) = row.component {
            let generation = self
                .row_generations
                .get(&id)
                .ok_or(EditRefusal::UnknownComponent)?;
            if *generation != row.generation {
                return Err(EditRefusal::StaleRow);
            }
            self.draft
                .component(id)
                .ok_or(EditRefusal::UnknownComponent)?;
        }
        Ok(row.component)
    }
    fn admit_component(&self, row: RowKey) -> Result<ComponentId, EditRefusal> {
        self.admit_row(row)?.ok_or(EditRefusal::Inapplicable)
    }
    pub(crate) fn admit_document(&self, row: RowKey) -> Result<(), EditRefusal> {
        if self.admit_row(row)?.is_some() {
            Err(EditRefusal::Inapplicable)
        } else {
            Ok(())
        }
    }
    fn admit_field(&self, handle: FieldHandle) -> Result<(), EditRefusal> {
        let component = self.admit_row(handle.row)?;
        match (component, handle.field) {
            (None, EditingField::Metadata(_)) => self.draft.has_field(handle.field),
            (Some(owner), EditingField::Parameter(id, _)) if owner == id => {
                self.draft.has_field(handle.field)
            }
            _ => Err(EditRefusal::Inapplicable),
        }
    }

    /// Staged text for a field, if any.
    #[must_use]
    pub fn staged(&self, field: EditingField) -> Option<&str> {
        self.staged.get(&field).map(String::as_str)
    }

    /// Whether any field has staged text.
    #[must_use]
    pub fn has_staged(&self) -> bool {
        !self.staged.is_empty()
    }

    /// The committed value of a field as editable text.
    #[must_use]
    pub fn committed_text(&self, field: EditingField) -> String {
        match field {
            EditingField::Metadata(MetadataField::Title) => self.draft.title().to_owned(),
            EditingField::Metadata(MetadataField::Summary) => self.draft.summary().to_owned(),
            EditingField::Parameter(_, parameter) => self
                .draft
                .value(field)
                .map_or_else(String::new, |value| (parameter).input_text(value)),
            EditingField::Metadata(metadata) => {
                match (metadata.parameter(), self.draft.value(field)) {
                    (Some(parameter), Some(value)) => (parameter).input_text(value),
                    _ => String::new(),
                }
            }
        }
    }

    /// The text a field shows: staged text, or the committed value.
    #[must_use]
    pub fn text(&self, field: EditingField) -> String {
        self.staged(field)
            .map_or_else(|| self.committed_text(field), str::to_owned)
    }

    /// Record typed text without committing it.
    ///
    /// # Errors
    /// Returns an [`EditRefusal`] for a stale document or an absent field.
    pub fn stage(&mut self, handle: FieldHandle, text: &str) -> Result<(), EditRefusal> {
        self.admit_field(handle)?;
        if self.staged.get(&handle.field).map(String::as_str) != Some(text) {
            self.staged.insert(handle.field, text.to_owned());
            self.input_revision += 1;
        }
        Ok(())
    }

    /// Discard a field's staged text and return its committed text.
    ///
    /// # Errors
    /// Returns an [`EditRefusal`] for a stale document or an absent field.
    pub fn restore(&mut self, handle: FieldHandle) -> Result<String, EditRefusal> {
        self.admit_field(handle)?;
        if self.staged.remove(&handle.field).is_some() {
            self.input_revision += 1;
        }
        Ok(self.committed_text(handle.field))
    }

    fn parse(&self) -> Candidate {
        let mut draft = self.draft.clone();
        let mut problems = Vec::new();
        for (field, text) in &self.staged {
            match field {
                EditingField::Metadata(MetadataField::Title) => draft.set_title(text.clone()),
                EditingField::Metadata(MetadataField::Summary) => draft.set_summary(text.clone()),
                EditingField::Parameter(_, parameter) => {
                    apply_number(&mut draft, *field, *parameter, text, &mut problems);
                }
                EditingField::Metadata(metadata) => {
                    if let Some(parameter) = metadata.parameter() {
                        apply_number(&mut draft, *field, parameter, text, &mut problems);
                    }
                }
            }
        }
        for (index, parameter) in draft.sound().nonfinite() {
            let field = match index {
                None if parameter == ParameterId::Duration => {
                    EditingField::Metadata(MetadataField::Duration)
                }
                None => EditingField::Metadata(MetadataField::Fade),
                Some(index) => EditingField::Parameter(draft.components()[index].id, parameter),
            };
            if !problems.iter().any(|(other, _)| *other == field) {
                problems.push((field, InputProblem::Nonfinite));
            }
        }
        Candidate { draft, problems }
    }

    /// The candidate: committed values with staged text applied.
    pub fn candidate(&mut self) -> Arc<Candidate> {
        if let Some((draft, input, candidate)) = &self.candidate
            && *draft == self.draft_revision
            && *input == self.input_revision
        {
            return Arc::clone(candidate);
        }
        let candidate = Arc::new(self.parse());
        self.candidate = Some((
            self.draft_revision,
            self.input_revision,
            Arc::clone(&candidate),
        ));
        candidate
    }

    /// Commit the whole candidate atomically. Returns whether committed
    /// values changed.
    ///
    /// # Errors
    /// Returns every unparseable or non-finite field and keeps all staged
    /// text and the committed draft unchanged.
    pub fn commit(&mut self) -> Result<bool, CommitRefusal> {
        let candidate = self.candidate();
        if !candidate.is_complete() {
            return Err(CommitRefusal {
                problems: candidate.problems.clone(),
            });
        }
        if self.staged.is_empty() {
            return Ok(false);
        }
        self.draft = candidate.draft.clone();
        self.staged.clear();
        self.draft_revision += 1;
        self.input_revision += 1;
        Ok(true)
    }

    /// Stage a field's text and commit the whole candidate.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale or absent field, or the
    /// [`CommitRefusal`] that kept the candidate staged.
    pub fn commit_field(
        &mut self,
        handle: FieldHandle,
        text: &str,
    ) -> Result<Result<bool, CommitRefusal>, EditRefusal> {
        self.stage(handle, text)?;
        Ok(self.commit())
    }

    fn structural(
        &mut self,
        row: RowKey,
        change: impl FnOnce(&mut auditorylab_core::ComponentForm),
    ) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        let component = self
            .draft
            .component_mut(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let before = *component;
        change(component);
        if !component.same_content(before) {
            let after = *component;
            self.draft.demote_evidence();
            let staged = self.staged.len();
            self.staged.retain(|field, _| match field {
                EditingField::Parameter(owner, parameter) if *owner == id => {
                    after.read(*parameter).is_some()
                }
                _ => true,
            });
            if staged != self.staged.len() {
                self.input_revision += 1;
            }
            self.draft_revision += 1;
        }
        Ok(())
    }

    /// Route a component to other ears.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn set_ears(&mut self, row: RowKey, ears: Ears) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        if self
            .draft
            .component(id)
            .is_some_and(|c| matches!(c, auditorylab_core::ComponentForm::BinauralPair(_)))
        {
            return Err(EditRefusal::Inapplicable);
        }
        self.structural(row, |c| match c {
            auditorylab_core::ComponentForm::Voice(c) => c.ears = ears,
            auditorylab_core::ComponentForm::MonauralPair { ears: route, .. } => *route = ears,
            _ => {}
        })
    }

    /// Give a component another carrier kind with that kind's defaults.
    /// Staged text of quantities the new carrier lacks is discarded.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn set_carrier(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), EditRefusal> {
        self.structural(row, |c| match c {
            auditorylab_core::ComponentForm::Voice(c) => {
                if c.carrier.kind() != kind {
                    c.carrier = controls::default_carrier(kind);
                }
            }
            form => {
                *form = auditorylab_core::ComponentForm::Voice(controls::default_component(kind))
            }
        })
    }

    /// Give a component another modulation kind, or none, with defaults.
    /// Staged text of quantities the new modulation lacks is discarded.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn set_modulation(
        &mut self,
        row: RowKey,
        kind: Option<ModulationKind>,
        rate: SampleRate,
    ) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        if self
            .draft
            .component(id)
            .is_some_and(|c| !matches!(c, auditorylab_core::ComponentForm::Voice(_)))
        {
            return Err(EditRefusal::Inapplicable);
        }
        self.structural(row, |form| {
            if let auditorylab_core::ComponentForm::Voice(c) = form
                && c.modulation.map(|m| m.kind()) != kind
            {
                c.modulation = kind.map(|kind| controls::default_modulation(kind, rate));
            }
        })
    }

    /// Add a default component.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document, either capacity limit or
    /// exhausted identities.
    pub fn add_component(
        &mut self,
        row: RowKey,
        kind: CarrierKind,
    ) -> Result<ComponentId, EditRefusal> {
        self.admit_document(row)?;
        let id = self.draft.add(kind)?;
        self.row_generations.insert(id, 0);
        self.draft_revision += 1;
        Ok(id)
    }

    /// Add both lanes of an explicit pair in one committed editor revision.
    ///
    /// # Errors
    /// Refuses stale documents, owner exhaustion or either voice budget.
    pub fn add_pair(
        &mut self,
        row: RowKey,
        kind: auditorylab_core::PairKind,
    ) -> Result<ComponentId, EditRefusal> {
        self.admit_document(row)?;
        let id = self.draft.add_node(controls::default_pair(kind))?;
        self.row_generations.insert(id, 0);
        self.draft_revision += 1;
        Ok(id)
    }

    /// Remove a component and only its staged text.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn remove_component(&mut self, row: RowKey) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        let generation = row
            .generation
            .checked_add(1)
            .ok_or(EditRefusal::IdentitiesExhausted)?;
        let before = self.draft.content.clone();
        let (index, component) = self.draft.remove(id).ok_or(EditRefusal::UnknownComponent)?;
        let after = self.draft.content.clone();
        let owned = |field: &EditingField| matches!(field, EditingField::Parameter(owner, _) if *owner == id);
        let staged: Vec<_> = self
            .staged
            .iter()
            .filter(|(field, _)| owned(field))
            .map(|(field, text)| (*field, text.clone()))
            .collect();
        if !staged.is_empty() {
            self.staged.retain(|field, _| !owned(field));
            self.input_revision += 1;
        }
        self.row_generations.remove(&id);
        self.removed = Some(Removed {
            generation,
            index,
            id,
            component,
            before,
            after,
            saved: self.saved.clone(),
            staged,
        });
        self.draft_revision += 1;
        Ok(())
    }

    /// The most recently removed component and the position it had, while it
    /// can still be restored.
    #[must_use]
    pub fn removed(&self) -> Option<(usize, &auditorylab_core::Component)> {
        self.removed.as_ref().map(|r| (r.index, &r.component))
    }

    pub(crate) fn restore_admission(&self, row: RowKey) -> Result<u64, EditRefusal> {
        self.admit_document(row)?;
        let removed = self.removed.as_ref().ok_or(EditRefusal::NothingRemoved)?;
        let generation = removed
            .generation
            .checked_add(1)
            .ok_or(EditRefusal::IdentitiesExhausted)?;
        self.draft.check_addition(removed.component.form)?;
        Ok(generation)
    }
    /// Put the most recently removed component back where it was, with its
    /// identity and the text that was staged for it.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document, when nothing was
    /// removed, or at the component limit, which keeps the component for a
    /// later restore.
    pub fn restore_removed(&mut self, row: RowKey) -> Result<ComponentId, EditRefusal> {
        let generation = self.restore_admission(row)?;
        let removed = self
            .removed
            .take()
            .unwrap_or_else(|| unreachable!("restore admission keeps the removed component"));
        let complete_undo = Arc::ptr_eq(&removed.saved, &self.saved)
            && self.draft.content.same_editable_content(&removed.after);
        if let Err(refusal) = self.draft.restore(removed.index, removed.component) {
            self.removed = Some(removed);
            return Err(refusal);
        }
        self.row_generations.insert(removed.id, generation);
        if complete_undo {
            let document = self.draft.content.document.clone();
            self.draft.content = removed.before;
            self.draft.content.document = document;
        }
        if !removed.staged.is_empty() {
            self.staged.extend(removed.staged);
            self.input_revision += 1;
        }
        self.draft_revision += 1;
        Ok(removed.id)
    }

    /// Whether the document differs from its saved content or has staged text.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.has_staged() || !self.draft.content_matches(&self.saved)
    }

    /// Record successfully published content as this document's saved
    /// baseline. Returns `false`, changing nothing, for another document.
    pub fn record_saved(&mut self, document: DocumentId, content: &Recipe) -> bool {
        if document != self.document || content.document().id != self.draft.content.document.id {
            return false;
        }
        self.saved_nominal = Some(crate::core::rhythm::nominal(&content.sound().components));
        if &self.draft.content.document != content.document() {
            self.draft.content.document = content.document().clone();
            self.candidate = None;
            self.save_check = None;
        }
        if !self.saved.same_document(content) {
            self.saved = Arc::new(content.clone());
        }
        true
    }

    /// The committed values' evaluation at `rate`.
    pub fn committed_analysis(&mut self, rate: SampleRate) -> Arc<Analysis> {
        let key = AnalysisKey::new(&self.draft, rate);
        if let Some(analysis) = self.cached(&key) {
            return analysis;
        }
        let analysis = Arc::new(Analysis::new(&self.draft, rate));
        self.store(key, analysis)
    }

    /// The candidate's evaluation at `rate`, for diagnostics and readiness.
    pub fn candidate_analysis(&mut self, rate: SampleRate) -> Arc<Analysis> {
        if self.staged.is_empty() {
            return self.committed_analysis(rate);
        }
        let candidate = self.candidate();
        let key = AnalysisKey::new(&candidate.draft, rate);
        if let Some(analysis) = self.cached(&key) {
            return analysis;
        }
        let analysis = Arc::new(Analysis::new(&candidate.draft, rate));
        self.store(key, analysis)
    }

    fn cached(&self, key: &AnalysisKey) -> Option<Arc<Analysis>> {
        self.analyses
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, analysis)| Arc::clone(analysis))
    }

    fn store(&mut self, key: AnalysisKey, analysis: Arc<Analysis>) -> Arc<Analysis> {
        #[cfg(test)]
        {
            self.evaluations += 1;
        }
        if self.analyses.len() == CACHED_ANALYSES {
            self.analyses.remove(0);
        }
        self.analyses.push((key, Arc::clone(&analysis)));
        analysis
    }

    /// How many evaluations ran, for checking that unchanged inputs reuse results.
    #[cfg(test)]
    pub(crate) const fn evaluations(&self) -> u64 {
        self.evaluations
    }

    /// Whether the candidate can be saved: complete and serializable within
    /// the size limit, regardless of acoustic validity.
    ///
    /// # Errors
    /// Returns the reason it cannot.
    pub fn save_readiness(&mut self) -> Result<(), SaveRefusal> {
        self.serialized_candidate().map(|_| ())
    }
    pub(crate) fn serialized_candidate(&mut self) -> Result<Arc<str>, SaveRefusal> {
        if let Some((draft, input, result)) = &self.save_check
            && *draft == self.draft_revision
            && *input == self.input_revision
        {
            return result.clone();
        }
        let candidate = self.candidate();
        let result = if candidate.is_complete() {
            #[cfg(test)]
            {
                self.serializations += 1;
            }
            candidate
                .draft
                .to_toml()
                .map(Arc::<str>::from)
                .map_err(SaveRefusal::Document)
        } else {
            Err(SaveRefusal::Input(candidate.problems.clone()))
        };
        self.save_check = Some((self.draft_revision, self.input_revision, result.clone()));
        result
    }

    /// How many save checks serialized the candidate.
    #[cfg(test)]
    pub(crate) const fn serializations(&self) -> u64 {
        self.serializations
    }
}

/// Why a candidate cannot be saved.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveRefusal {
    /// Staged input does not parse or numbers are not finite.
    Input(Vec<(EditingField, InputProblem)>),
    /// The document does not serialize within the reader's limits.
    Document(Error),
}

fn apply_number(
    draft: &mut Draft,
    field: EditingField,
    parameter: ParameterId,
    text: &str,
    problems: &mut Vec<(EditingField, InputProblem)>,
) {
    match text.trim().parse::<f64>() {
        Ok(display) => {
            let value = parameter.descriptor().unit.to_model(display);
            draft.set_value(field, value);
            if !value.is_finite() {
                problems.push((field, InputProblem::Nonfinite));
            }
        }
        Err(_) => problems.push((field, InputProblem::Unparseable)),
    }
}

#[cfg(test)]
mod tests;
