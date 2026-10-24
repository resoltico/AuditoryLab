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
use crate::{Error, ErrorKind, Origin, RecipeSnapshot, controls, presentation};
use auditorylab_core::{
    About, CarrierKind, Component, ComponentId, Ears, ModulationKind, ParameterId, Recipe,
    SampleRate, Signal, Stimulus, StimulusError, Target, TargetCheck,
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
            .map_err(|_| Error::new(ErrorKind::InvalidRecipe, "document identities exhausted"))
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

/// A field of one particular document. Commands carry the handle captured
/// when the field was rendered, so a late callback cannot edit a replacement
/// document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldHandle {
    /// The document the field belongs to.
    pub document: DocumentId,
    /// The field within that document.
    pub field: EditingField,
}

/// Why an editing command was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusal {
    /// The command addresses a document that is no longer open.
    StaleDocument,
    /// The component no longer exists.
    UnknownComponent,
    /// The component's carrier or modulation does not have the quantity.
    Inapplicable,
    /// The document already has [`Stimulus::MAX_COMPONENTS`] components.
    ComponentLimit,
    /// No unused component identity remains in this document.
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

/// Editable recipe values with stable component identities. A draft is not
/// necessarily finite or acoustically valid.
#[derive(Clone, Debug)]
pub struct Draft {
    title: String,
    summary: String,
    duration: f64,
    fade: f64,
    components: Vec<(ComponentId, Component)>,
    next_id: u32,
    based_on: Option<RecipeSnapshot>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            title: "Untitled sound".into(),
            summary: String::new(),
            duration: 60.0,
            fade: 0.1,
            components: Vec::new(),
            next_id: 1,
            based_on: None,
        }
    }
}

impl Draft {
    /// An empty, silent draft of sixty seconds with a 0.1-second fade.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Copy a snapshot's sound parameters; keep the snapshot as attributed
    /// source context, separately from the draft's own preserved notes.
    /// Components get the identities their positions give in the snapshot's
    /// own targets.
    #[must_use]
    pub fn from_snapshot(snapshot: &RecipeSnapshot) -> Self {
        let mut draft = Self::new();
        draft.title = format!("{} (edited)", snapshot.title());
        let recipe = snapshot.recipe();
        draft.summary = recipe
            .about
            .as_ref()
            .and_then(|a| a.summary.clone())
            .unwrap_or_default();
        draft.based_on = Some(match &recipe.source {
            Some(source) => RecipeSnapshot::new(
                Origin::Stream("Retained source".into()),
                Recipe {
                    stimulus: source.stimulus.clone(),
                    about: source.about.clone(),
                    source: None,
                },
            ),
            None => snapshot.clone(),
        });
        draft.duration = snapshot.stimulus().duration_seconds;
        draft.fade = snapshot.stimulus().fade_seconds;
        for (index, component) in snapshot.stimulus().components.iter().enumerate() {
            // Loaded recipes are bounded by Recipe::MAX_BYTES, far below u32 positions.
            if let Some(id) = ComponentId::numbered(index) {
                draft.components.push((id, *component));
                draft.next_id = id.get().saturating_add(1);
            }
        }
        draft
    }

    /// The snapshot this draft was copied from, kept for attribution.
    #[must_use]
    pub const fn based_on(&self) -> Option<&RecipeSnapshot> {
        self.based_on.as_ref()
    }

    /// Components and their stable identities in display order.
    #[must_use]
    pub fn components(&self) -> &[(ComponentId, Component)] {
        &self.components
    }

    /// One component by identity.
    #[must_use]
    pub fn component(&self, id: ComponentId) -> Option<&Component> {
        self.components
            .iter()
            .find(|(key, _)| *key == id)
            .map(|(_, component)| component)
    }

    fn component_mut(&mut self, id: ComponentId) -> Option<&mut Component> {
        self.components
            .iter_mut()
            .find(|(key, _)| *key == id)
            .map(|(_, component)| component)
    }

    /// Add a default component, refusing beyond [`Stimulus::MAX_COMPONENTS`]
    /// or when identities are exhausted.
    ///
    /// # Errors
    /// Returns [`EditRefusal::ComponentLimit`] or [`EditRefusal::IdentitiesExhausted`].
    pub fn add(&mut self, kind: CarrierKind) -> Result<ComponentId, EditRefusal> {
        if self.components.len() >= Stimulus::MAX_COMPONENTS {
            return Err(EditRefusal::ComponentLimit);
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(EditRefusal::IdentitiesExhausted)?;
        let id = ComponentId::new(self.next_id);
        self.next_id = next;
        self.components
            .push((id, controls::default_component(kind)));
        Ok(id)
    }

    /// Replace an existing component; returns `false` for an unknown identity.
    pub fn replace(&mut self, id: ComponentId, component: Component) -> bool {
        self.component_mut(id)
            .map(|value| *value = component)
            .is_some()
    }

    /// Remove a component without reusing its identity, returning its
    /// position and value, or `None` for an unknown identity.
    pub fn remove(&mut self, id: ComponentId) -> Option<(usize, Component)> {
        let index = self.components.iter().position(|(key, _)| *key == id)?;
        Some((index, self.components.remove(index).1))
    }

    /// Put a removed component back at its position, or at the end if fewer
    /// components remain, under the identity it had.
    fn restore(
        &mut self,
        index: usize,
        id: ComponentId,
        component: Component,
    ) -> Result<(), EditRefusal> {
        if self.components.len() >= Stimulus::MAX_COMPONENTS {
            return Err(EditRefusal::ComponentLimit);
        }
        let index = index.min(self.components.len());
        self.components.insert(index, (id, component));
        Ok(())
    }

    /// Editable title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }

    /// The document's own notes.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Set the notes.
    pub fn set_summary(&mut self, summary: String) {
        self.summary = summary;
    }

    /// Duration of saved recipes and exported audio, in seconds.
    #[must_use]
    pub const fn duration_seconds(&self) -> f64 {
        self.duration
    }

    /// Set the saved and exported duration.
    pub fn set_duration_seconds(&mut self, seconds: f64) {
        self.duration = seconds;
    }

    /// Edge fade of saved recipes and exported audio, in seconds.
    #[must_use]
    pub const fn fade_seconds(&self) -> f64 {
        self.fade
    }

    /// Set the saved and exported fade.
    pub fn set_fade_seconds(&mut self, seconds: f64) {
        self.fade = seconds;
    }

    /// A numeric field's model value, if the field exists.
    #[must_use]
    pub fn value(&self, field: EditingField) -> Option<f64> {
        match field {
            EditingField::Parameter(id, parameter) => parameter.read(self.component(id)?),
            EditingField::Metadata(MetadataField::Duration) => Some(self.duration),
            EditingField::Metadata(MetadataField::Fade) => Some(self.fade),
            EditingField::Metadata(MetadataField::Title | MetadataField::Summary) => None,
        }
    }

    fn set_value(&mut self, field: EditingField, value: f64) -> bool {
        match field {
            EditingField::Parameter(id, parameter) => self
                .component_mut(id)
                .is_some_and(|component| parameter.write(component, value)),
            EditingField::Metadata(MetadataField::Duration) => {
                self.duration = value;
                true
            }
            EditingField::Metadata(MetadataField::Fade) => {
                self.fade = value;
                true
            }
            EditingField::Metadata(MetadataField::Title | MetadataField::Summary) => false,
        }
    }

    fn has_field(&self, field: EditingField) -> Result<(), EditRefusal> {
        match field {
            EditingField::Parameter(id, parameter) => {
                let component = self.component(id).ok_or(EditRefusal::UnknownComponent)?;
                parameter
                    .read(component)
                    .map(|_| ())
                    .ok_or(EditRefusal::Inapplicable)
            }
            EditingField::Metadata(_) => Ok(()),
        }
    }

    /// The draft's sound as a stimulus.
    #[must_use]
    pub fn stimulus(&self) -> Stimulus {
        Stimulus {
            duration_seconds: self.duration,
            fade_seconds: self.fade,
            components: self.components.iter().map(|(_, c)| *c).collect(),
        }
    }

    /// Persist current parameters and notes, with research attributed to one
    /// retained original source content anchor.
    #[must_use]
    pub fn recipe(&self) -> Recipe {
        Recipe {
            stimulus: self.stimulus(),
            about: Some(About {
                title: self.title.clone(),
                summary: Some(self.summary.clone()),
                research: None,
            }),
            source: self
                .based_on
                .as_ref()
                .map(|snapshot| auditorylab_core::Source {
                    stimulus: snapshot.stimulus().clone(),
                    about: snapshot.about().cloned(),
                }),
        }
    }

    /// An immutable snapshot of [`Draft::recipe`].
    #[must_use]
    pub fn snapshot(&self) -> RecipeSnapshot {
        RecipeSnapshot::new(Origin::Draft(self.title.clone()), self.recipe())
    }

    /// Whether the draft's saved content equals `recipe`: title, notes,
    /// duration, fade and components in order, ignoring identities.
    #[must_use]
    pub fn content_matches(&self, recipe: &Recipe) -> bool {
        let about = recipe.about.as_ref();
        let own = self.recipe();
        own.source == recipe.source
            && own.about == recipe.about
            && about.is_some_and(|a| a.title == self.title)
            && about.and_then(|a| a.summary.as_deref()).unwrap_or("") == self.summary
            && recipe.stimulus.duration_seconds.to_bits() == self.duration.to_bits()
            && recipe.stimulus.fade_seconds.to_bits() == self.fade.to_bits()
            && recipe.stimulus.components.len() == self.components.len()
            && recipe
                .stimulus
                .components
                .iter()
                .zip(&self.components)
                .all(|(a, (_, b))| a == b)
    }

    /// The draft's component pairs for target compilation.
    #[must_use]
    pub fn target_check(&self, rate: SampleRate) -> TargetCheck {
        TargetCheck::new(rate, &self.components)
    }

    /// Serialize the exact draft within the reader's byte limit, without file
    /// I/O. Finite values that are acoustically invalid serialize.
    ///
    /// # Errors
    /// Returns [`ErrorKind::InvalidRecipe`] for non-finite numbers, a writer
    /// failure or text longer than [`Recipe::MAX_BYTES`].
    pub fn to_toml(&self) -> Result<String, Error> {
        serialize(&self.recipe())
    }
}

/// Serialize finite recipe content within the reader's byte limit.
///
/// # Errors
/// Returns [`ErrorKind::InvalidRecipe`] for non-finite numbers, a writer
/// failure or text longer than [`Recipe::MAX_BYTES`].
pub(crate) fn serialize(recipe: &Recipe) -> Result<String, Error> {
    if recipe
        .source
        .as_ref()
        .is_some_and(|s| s.stimulus.nonfinite().next().is_some())
    {
        return Err(Error::new(
            ErrorKind::InvalidRecipe,
            "retained source numbers must be finite; correct the source document before deriving a sound",
        ));
    }
    if recipe.stimulus.nonfinite().next().is_some() {
        return Err(Error::new(
            ErrorKind::InvalidRecipe,
            "recipe numbers must be finite",
        ));
    }
    let text = recipe
        .to_toml()
        .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, e.to_string(), e))?;
    if text.len() > Recipe::MAX_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidRecipe,
            format!("recipe exceeds {} bytes", Recipe::MAX_BYTES),
        ));
    }
    Ok(text)
}

/// Save exactly this content with staged publication. Saving cannot be
/// cancelled once started.
///
/// # Errors
/// Returns serialization errors, [`ErrorKind::DestinationExists`] under
/// [`Replace::Never`](crate::Replace::Never), or [`ErrorKind::Write`].
pub fn save_content(
    recipe: &Recipe,
    path: &std::path::Path,
    replace: crate::Replace,
) -> Result<(), Error> {
    use std::io::Write;
    let text = serialize(recipe)?;
    crate::publish::publish(path, replace, &crate::Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|e| {
            Error::caused(ErrorKind::Write, format!("cannot save to {path:?}: {e}"), e)
        })
    })
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
    envelope: Result<(), StimulusError>,
    signal: Option<Arc<Signal>>,
}

impl Analysis {
    fn new(draft: &Draft, rate: SampleRate) -> Self {
        let check = draft.target_check(rate);
        let envelope = Signal::check_envelope(rate, draft.duration, draft.fade);
        let signal = match (&envelope, check.target()) {
            (Ok(()), Some(target)) => target.signal(draft.duration, draft.fade).ok().map(Arc::new),
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
    pub const fn envelope(&self) -> Result<(), &StimulusError> {
        match &self.envelope {
            Ok(()) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// The first finite-signal error in the established order: envelope
    /// before components.
    #[must_use]
    pub fn finite_error(&self) -> Option<&StimulusError> {
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
    components: Vec<(ComponentId, Component)>,
    rate: SampleRate,
}

impl AnalysisKey {
    fn new(draft: &Draft, rate: SampleRate) -> Self {
        Self {
            duration: draft.duration.to_bits(),
            fade: draft.fade.to_bits(),
            components: draft.components.clone(),
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
                .all(|((id, a), (other_id, b))| {
                    id == other_id
                        && a.ears == b.ears
                        && a.carrier.kind() == b.carrier.kind()
                        && a.modulation.map(|m| m.kind()) == b.modulation.map(|m| m.kind())
                        && ParameterId::COMPONENT
                            .iter()
                            .all(|p| p.read(a).map(f64::to_bits) == p.read(b).map(f64::to_bits))
                })
    }
}
impl Eq for AnalysisKey {}

/// Bounded memo of evaluations: the selected export rate, the live device
/// rate and their candidate and committed forms.
const CACHED_ANALYSES: usize = 4;

/// The most recently removed component, with the text that was staged for it.
#[derive(Debug)]
struct Removed {
    index: usize,
    id: ComponentId,
    component: Component,
    staged: Vec<(EditingField, String)>,
}

/// One open document and its editing state.
#[derive(Debug)]
pub struct EditorSession {
    document: DocumentId,
    draft: Draft,
    staged: BTreeMap<EditingField, String>,
    draft_revision: u64,
    input_revision: u64,
    saved: Recipe,
    analyses: Vec<(AnalysisKey, Arc<Analysis>)>,
    candidate: Option<(u64, u64, Arc<Candidate>)>,
    save_check: Option<(u64, u64, Result<(), SaveRefusal>)>,
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
    /// Returns an error only when document identities are exhausted.
    pub fn new(draft: Draft) -> Result<Self, Error> {
        Ok(Self {
            document: DocumentId::allocate()?,
            draft,
            staged: BTreeMap::new(),
            draft_revision: 0,
            input_revision: 0,
            saved: Draft::new().recipe(),
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

    /// A handle for one of this document's fields.
    #[must_use]
    pub const fn handle(&self, field: EditingField) -> FieldHandle {
        FieldHandle {
            document: self.document,
            field,
        }
    }

    fn admit(&self, document: DocumentId) -> Result<(), EditRefusal> {
        if document == self.document {
            Ok(())
        } else {
            Err(EditRefusal::StaleDocument)
        }
    }

    fn admit_field(&self, handle: FieldHandle) -> Result<(), EditRefusal> {
        self.admit(handle.document)?;
        self.draft.has_field(handle.field)
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
            EditingField::Metadata(MetadataField::Title) => self.draft.title.clone(),
            EditingField::Metadata(MetadataField::Summary) => self.draft.summary.clone(),
            EditingField::Parameter(_, parameter) => {
                self.draft.value(field).map_or_else(String::new, |value| {
                    presentation::editable_number(parameter, value)
                })
            }
            EditingField::Metadata(metadata) => {
                match (metadata.parameter(), self.draft.value(field)) {
                    (Some(parameter), Some(value)) => {
                        presentation::editable_number(parameter, value)
                    }
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
                EditingField::Metadata(MetadataField::Title) => draft.title.clone_from(text),
                EditingField::Metadata(MetadataField::Summary) => draft.summary.clone_from(text),
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
        for (index, parameter) in draft.stimulus().nonfinite() {
            let field = match index {
                None if parameter == ParameterId::Duration => {
                    EditingField::Metadata(MetadataField::Duration)
                }
                None => EditingField::Metadata(MetadataField::Fade),
                Some(index) => EditingField::Parameter(draft.components[index].0, parameter),
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
        document: DocumentId,
        id: ComponentId,
        change: impl FnOnce(&mut Component),
    ) -> Result<(), EditRefusal> {
        self.admit(document)?;
        let component = self
            .draft
            .component_mut(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let before = *component;
        change(component);
        if *component != before {
            let after = *component;
            let staged = self.staged.len();
            self.staged.retain(|field, _| match field {
                EditingField::Parameter(owner, parameter) if *owner == id => {
                    parameter.read(&after).is_some()
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
    /// Returns [`EditRefusal`] for a stale document or unknown component.
    pub fn set_ears(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        ears: Ears,
    ) -> Result<(), EditRefusal> {
        self.structural(document, id, |c| c.ears = ears)
    }

    /// Give a component another carrier kind with that kind's defaults.
    /// Staged text of quantities the new carrier lacks is discarded.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document or unknown component.
    pub fn set_carrier(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        kind: CarrierKind,
    ) -> Result<(), EditRefusal> {
        self.structural(document, id, |c| {
            if c.carrier.kind() != kind {
                c.carrier = controls::default_carrier(kind);
            }
        })
    }

    /// Give a component another modulation kind, or none, with defaults.
    /// Staged text of quantities the new modulation lacks is discarded.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document or unknown component.
    pub fn set_modulation(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        kind: Option<ModulationKind>,
        rate: SampleRate,
    ) -> Result<(), EditRefusal> {
        self.structural(document, id, |c| {
            if c.modulation.map(|m| m.kind()) != kind {
                c.modulation = kind.map(|kind| controls::default_modulation(kind, rate));
            }
        })
    }

    /// Add a default component.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document, the component limit or
    /// exhausted identities.
    pub fn add_component(
        &mut self,
        document: DocumentId,
        kind: CarrierKind,
    ) -> Result<ComponentId, EditRefusal> {
        self.admit(document)?;
        let id = self.draft.add(kind)?;
        self.draft_revision += 1;
        Ok(id)
    }

    /// Remove a component and only its staged text.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document or unknown component.
    pub fn remove_component(
        &mut self,
        document: DocumentId,
        id: ComponentId,
    ) -> Result<(), EditRefusal> {
        self.admit(document)?;
        let (index, component) = self.draft.remove(id).ok_or(EditRefusal::UnknownComponent)?;
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
        self.removed = Some(Removed {
            index,
            id,
            component,
            staged,
        });
        self.draft_revision += 1;
        Ok(())
    }

    /// The most recently removed component and the position it had, while it
    /// can still be restored.
    #[must_use]
    pub fn removed(&self) -> Option<(usize, &Component)> {
        self.removed.as_ref().map(|r| (r.index, &r.component))
    }

    /// Put the most recently removed component back where it was, with its
    /// identity and the text that was staged for it.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document, when nothing was
    /// removed, or at the component limit, which keeps the component for a
    /// later restore.
    pub fn restore_removed(&mut self, document: DocumentId) -> Result<ComponentId, EditRefusal> {
        self.admit(document)?;
        let removed = self.removed.take().ok_or(EditRefusal::NothingRemoved)?;
        if let Err(refusal) = self
            .draft
            .restore(removed.index, removed.id, removed.component)
        {
            self.removed = Some(removed);
            return Err(refusal);
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
        if document != self.document {
            return false;
        }
        self.saved = content.clone();
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
                .map(|_| ())
                .map_err(|error| SaveRefusal::Document(error.to_string()))
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveRefusal {
    /// Staged input does not parse or numbers are not finite.
    Input(Vec<(EditingField, InputProblem)>),
    /// The document does not serialize within the reader's limits.
    Document(String),
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
