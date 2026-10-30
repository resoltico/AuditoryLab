//! Portable editable content, construction and provenance.
use super::{
    ComponentId, EditRefusal, EditingField, Error, Finite, MetadataField, Origin, Recipe,
    RecipeBuilder, RecipeSnapshot, SampleRate, StructureError, TargetCheck,
};

/// An authored document with complete portable metadata and stable owner IDs.
/// Committed settings may be finite but acoustically invalid and remain repairable.
#[derive(Clone, Debug)]
pub struct Draft {
    pub(super) recipe: RecipeBuilder,
    pub(super) next_id: Option<u32>,
    pub(super) based_on: Option<RecipeSnapshot>,
}
impl Draft {
    /// Create a provisional random identity for a silent sixty-second document.
    ///
    /// # Errors
    /// Returns an actionable error if secure identity generation fails.
    pub fn new() -> Result<Self, Error> {
        Ok(Self::empty(crate::identity::fresh_document()?))
    }
    pub(super) fn empty(document: auditorylab_core::DocumentIdentity) -> Self {
        let mut content = Recipe::builder(document);
        content.content.description = Some(auditorylab_core::provenance::Description {
            title: crate::core::vocabulary::UNTITLED_SOUND.into(),
            notes: Some(String::new()),
        });
        Self {
            recipe: content,
            next_id: Some(1),
            based_on: None,
        }
    }
    /// Open editable owned content without changing its identity or declared dates.
    #[must_use]
    pub fn from_recipe(content: Recipe) -> Self {
        let content = content.into_builder();
        let next_id = content
            .content
            .sound
            .components()
            .iter()
            .map(|c| c.id.get())
            .max()
            .unwrap_or(0)
            .checked_add(1);
        Self {
            recipe: content,
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
                crate::core::vocabulary::copy_note(snapshot.recipe()),
            )
            .map_err(Error::Recipe)?;
        let mut draft = Self::from_recipe(content);
        draft.set_title(crate::core::vocabulary::copy_title(&snapshot.title()));
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
        self.recipe.content.sound.components()
    }
    /// One authored form by its primary owner identity.
    #[must_use]
    pub fn component(&self, id: ComponentId) -> Option<&auditorylab_core::ComponentForm> {
        self.recipe.content.sound.component(id).map(|c| &c.form)
    }

    /// Check the authored and expanded capacity for one additional component.
    ///
    /// # Errors
    /// Reports current counts and the requested voice cost if either limit is exceeded.
    pub fn check_addition(&self, form: auditorylab_core::ComponentForm) -> Result<(), EditRefusal> {
        self.recipe
            .content
            .sound
            .check_addition(form)
            .map_err(|error| self.structure_refusal(error, form.voices()))
    }
    pub(super) fn structure_refusal(
        &self,
        error: StructureError,
        additional_voices: usize,
    ) -> EditRefusal {
        match error {
            StructureError::VoiceBudget { .. } => EditRefusal::BudgetExceeded {
                components: self.components().len(),
                voices: self.components().iter().map(|c| c.form.voices()).sum(),
                additional_voices,
            },
            StructureError::Identity { .. } => unreachable!(
                "checked sounds, monotonic new ids and removed owners preserve uniqueness"
            ),
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
        self.recipe
            .content
            .sound
            .push(auditorylab_core::Component { id, form })
            .map_err(|error| self.structure_refusal(error, form.voices()))?;
        self.next_id = id.get().checked_add(1);
        self.demote_evidence();
        Ok(id)
    }
    /// Replace an authored form while preserving its owner.
    ///
    /// # Errors
    /// Refuses a replacement that exceeds either sound budget.
    pub fn replace(
        &mut self,
        id: ComponentId,
        form: auditorylab_core::ComponentForm,
    ) -> Result<bool, EditRefusal> {
        let Some(before) = self.component(id).copied() else {
            return Ok(false);
        };
        if before == form {
            return Ok(true);
        }
        let replaced = self
            .recipe
            .content
            .sound
            .replace(id, form)
            .map_err(|error| {
                self.structure_refusal(error, form.voices().saturating_sub(before.voices()))
            })?;
        if replaced {
            self.demote_evidence();
        }
        Ok(replaced)
    }
    /// Remove one authored form without reusing its owner identity.
    pub fn remove(&mut self, id: ComponentId) -> Option<(usize, auditorylab_core::Component)> {
        let removed = self.recipe.content.sound.remove(id)?;
        self.demote_evidence();
        Some(removed)
    }
    pub(super) fn restore(
        &mut self,
        index: usize,
        component: auditorylab_core::Component,
    ) -> Result<(), EditRefusal> {
        let index = index.min(self.components().len());
        self.recipe
            .content
            .sound
            .insert(index, component)
            .map_err(|error| self.structure_refusal(error, component.form.voices()))?;
        self.demote_evidence();
        Ok(())
    }
    /// Current user-visible title; missing imported descriptions remain unmodified.
    #[must_use]
    pub fn title(&self) -> &str {
        self.recipe
            .content
            .description
            .as_ref()
            .map_or(crate::core::vocabulary::UNTITLED_SOUND, |d| {
                d.title.as_str()
            })
    }
    /// Set current title; a title edit does not change research scope.
    pub fn set_title(&mut self, title: String) {
        self.recipe
            .content
            .description
            .get_or_insert_with(|| auditorylab_core::provenance::Description {
                title: String::new(),
                notes: None,
            })
            .title = title;
    }
    /// Own notes, independent of attributed source findings.
    #[must_use]
    pub fn summary(&self) -> &str {
        self.recipe
            .content
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
        self.recipe
            .content
            .description
            .get_or_insert(auditorylab_core::provenance::Description { title, notes: None })
            .notes = Some(summary);
    }
    /// Requested finite duration.
    #[must_use]
    pub const fn duration_seconds(&self) -> f64 {
        self.recipe.content.timing.duration_seconds.get()
    }
    /// Set finite duration; invalid finite values remain editable for repair.
    pub fn set_duration_seconds(&mut self, value: Finite) {
        if self.recipe.content.timing.duration_seconds != value {
            self.demote_evidence();
        }
        self.recipe.content.timing.duration_seconds = value;
    }
    /// Requested endpoint fade.
    #[must_use]
    pub const fn fade_seconds(&self) -> f64 {
        self.recipe.content.timing.fade_seconds.get()
    }
    /// Set endpoint fade.
    pub fn set_fade_seconds(&mut self, value: Finite) {
        if self.recipe.content.timing.fade_seconds != value {
            self.demote_evidence();
        }
        self.recipe.content.timing.fade_seconds = value;
    }
    /// Applicable semantic quantity, preserving exact binary64 values.
    #[must_use]
    pub fn value(&self, field: EditingField) -> Option<f64> {
        match field {
            EditingField::Parameter(id, p) => self.component(id)?.get(p).map(Finite::get),
            EditingField::Metadata(MetadataField::Duration) => Some(self.duration_seconds()),
            EditingField::Metadata(MetadataField::Fade) => Some(self.fade_seconds()),
            EditingField::Metadata(_) => None,
        }
    }
    pub(super) fn set_value(&mut self, field: EditingField, value: Finite) -> bool {
        match field {
            EditingField::Parameter(id, p) => {
                let Some(mut form) = self.component(id).copied() else {
                    return false;
                };
                if !form.set(p, value) {
                    return false;
                }
                self.replace(id, form).unwrap_or_else(|_| {
                    unreachable!("a quantity edit retains identity and lane count")
                })
            }
            EditingField::Metadata(MetadataField::Duration) => {
                self.set_duration_seconds(value);
                true
            }
            EditingField::Metadata(MetadataField::Fade) => {
                self.set_fade_seconds(value);
                true
            }
            EditingField::Metadata(_) => false,
        }
    }
    pub(super) fn has_field(&self, field: EditingField) -> Result<(), EditRefusal> {
        match field {
            EditingField::Parameter(id, p) => self
                .component(id)
                .ok_or(EditRefusal::UnknownComponent)?
                .get(p)
                .map(|_| ())
                .ok_or(EditRefusal::Inapplicable),
            EditingField::Metadata(_) => Ok(()),
        }
    }
    /// Complete authored sound, with pair intent and stable owners retained.
    #[must_use]
    pub fn sound(&self) -> auditorylab_core::Sound {
        self.recipe.content.sound.clone()
    }
    /// Immutable full portable candidate. Rendition edits demote current-condition evidence.
    ///
    /// # Errors
    /// Returns the portable-contract failure for an incomplete editable candidate.
    pub fn recipe(&self) -> Result<Recipe, Error> {
        self.recipe.clone().build().map_err(Error::Recipe)
    }
    pub(super) fn demote_evidence(&mut self) {
        for evidence in &mut self.recipe.content.evidence {
            evidence.scope = auditorylab_core::provenance::EvidenceScope::Ancestor;
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
        self.recipe.document.id == recipe.document().id && self.recipe.content() == recipe.content()
    }
    /// Compile all expanded pair lanes as one atomic target candidate.
    #[must_use]
    pub fn check(&self, rate: SampleRate) -> TargetCheck {
        self.recipe.content.sound.check(rate)
    }
    /// Serialize a complete finite candidate without I/O.
    ///
    /// # Errors
    /// Rejects metadata contract violations or output exceeding the portable byte limit.
    pub fn to_toml(&self) -> Result<String, Error> {
        serialize(&self.recipe()?).map_err(Error::Recipe)
    }
}
/// Serialize finite recipe content within the reader's byte limit.
///
/// # Errors
/// Returns a recipe error for a writer
/// failure or text longer than [`Recipe::MAX_BYTES`].
pub(crate) fn serialize(recipe: &Recipe) -> Result<String, crate::core::RecipeError> {
    recipe.to_toml()
}
