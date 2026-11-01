//! Staging, interpretation lifetimes, captured saves and atomic edit transactions.
use super::{
    Analysis, AnalysisKey, Arc, BTreeMap, Candidate, ChannelRouting, CommitRefusal, ComponentId,
    DocumentId, Draft, EditRefusal, EditingField, Error, FieldHandle, InputProblem, MetadataField,
    ParameterId, Recipe, RecipeBuilder, RowKey, SampleRate, SaveRefusal,
};

/// Bounded memo of evaluations: the selected export rate, the live device
/// rate and their candidate and committed forms.
const CACHED_ANALYSES: usize = 4;

/// The most recently removed component, with the text that was staged for it.
#[derive(Debug)]
pub(super) struct Removed {
    pub(super) generation: u64,
    pub(super) index: usize,
    pub(super) id: ComponentId,
    pub(super) component: auditorylab_core::Component,
    pub(super) before: RecipeBuilder,
    pub(super) after: RecipeBuilder,
    pub(super) saved: Arc<Recipe>,
    pub(super) staged: Vec<(EditingField, String)>,
}

/// One open document and its editing state.
#[derive(Debug)]
pub struct EditorSession {
    pub(super) document: DocumentId,
    pub(super) row_generations: BTreeMap<ComponentId, u64>,
    // Birth generation of each continuously applicable semantic field. Menus
    // require the exact row generation; surviving field edits retain their lifetime.
    pub(super) field_births: BTreeMap<(ComponentId, ParameterId), u64>,
    pub(super) draft: Draft,
    pub(super) staged: BTreeMap<EditingField, String>,
    pub(super) draft_revision: u64,
    pub(super) input_revision: u64,
    pub(super) saved: Arc<Recipe>,
    pub(super) saved_nominal: Option<crate::core::rhythm::Analysis>,
    pub(super) nominal: Option<(u64, Arc<crate::core::rhythm::Analysis>)>,
    pub(super) analyses: Vec<(AnalysisKey, Arc<Analysis>)>,
    pub(super) candidate: Option<(u64, u64, Arc<Candidate>)>,
    pub(super) save_check: Option<(u64, u64, Result<Arc<str>, SaveRefusal>)>,
    pub(super) removed: Option<Removed>,
    #[cfg(test)]
    pub(super) evaluations: u64,
    #[cfg(test)]
    pub(super) serializations: u64,
}

impl EditorSession {
    /// Open a document. A new blank draft is clean; any other content is
    /// dirty until saved.
    ///
    /// # Errors
    /// Refuses exhausted document identities.
    pub fn new(draft: Draft) -> Result<Self, Error> {
        let saved = Draft::empty(draft.recipe.document.clone()).recipe()?;

        let row_generations = draft
            .components()
            .iter()
            .map(|component| (component.id, 0))
            .collect();
        let field_births = draft
            .components()
            .iter()
            .flat_map(|component| {
                ParameterId::COMPONENT
                    .into_iter()
                    .filter(|p| component.form.get(*p).is_some())
                    .map(|p| ((component.id, p), 0))
            })
            .collect();
        Ok(Self {
            document: DocumentId::allocate()?,
            row_generations,
            field_births,
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
        let analysis = Arc::new(crate::core::rhythm::nominal(&self.draft.sound()));
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

    pub(super) fn admit(&self, document: DocumentId) -> Result<(), EditRefusal> {
        if document == self.document {
            Ok(())
        } else {
            Err(EditRefusal::StaleDocument)
        }
    }
    pub(super) fn admit_row(&self, row: RowKey) -> Result<Option<ComponentId>, EditRefusal> {
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
    pub(super) fn admit_component(&self, row: RowKey) -> Result<ComponentId, EditRefusal> {
        self.admit_row(row)?.ok_or(EditRefusal::Inapplicable)
    }
    pub(crate) fn admit_document(&self, row: RowKey) -> Result<(), EditRefusal> {
        if self.admit_row(row)?.is_some() {
            Err(EditRefusal::Inapplicable)
        } else {
            Ok(())
        }
    }
    pub(super) fn admit_field(&self, handle: FieldHandle) -> Result<(), EditRefusal> {
        self.admit(handle.row.document)?;
        match (handle.row.component, handle.field) {
            (None, EditingField::Metadata(_)) => self.draft.has_field(handle.field),
            (Some(owner), EditingField::Parameter(id, p)) if owner == id => {
                let current = self
                    .row_generations
                    .get(&id)
                    .ok_or(EditRefusal::UnknownComponent)?;
                let birth = self
                    .field_births
                    .get(&(id, p))
                    .ok_or(EditRefusal::StaleRow)?;
                if handle.row.generation < *birth || handle.row.generation > *current {
                    return Err(EditRefusal::StaleRow);
                }
                self.draft.has_field(handle.field)
            }
            _ => Err(EditRefusal::Inapplicable),
        }
    }

    pub(super) fn refresh_fields(
        &mut self,
        id: ComponentId,
        form: auditorylab_core::ComponentForm,
        generation: u64,
    ) {
        self.field_births
            .retain(|(owner, p), _| *owner != id || form.get(*p).is_some());
        for p in ParameterId::COMPONENT {
            if form.get(p).is_some() {
                self.field_births.entry((id, p)).or_insert(generation);
            }
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
            EditingField::Parameter(_, parameter) => {
                self.draft.value(field).map_or_else(String::new, |value| {
                    crate::document::input::parameter_text(parameter, value)
                })
            }
            EditingField::Metadata(metadata) => {
                match (metadata.parameter(), self.draft.value(field)) {
                    (Some(parameter), Some(value)) => {
                        crate::document::input::parameter_text(parameter, value)
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

    pub(super) fn parse(&self) -> Candidate {
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
        self.admit_field(handle)?;
        // No text-edited event means an unchanged representation is not a new number.
        // Display-unit conversion is not invertible for every finite model value.
        if self.staged(handle.field).is_none() && text == self.committed_text(handle.field) {
            return Ok(self.commit());
        }
        self.stage(handle, text)?;
        Ok(self.commit())
    }

    pub(super) fn structural(
        &mut self,
        row: RowKey,
        change: impl FnOnce(&mut auditorylab_core::ComponentForm),
    ) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        let before = self
            .draft
            .component(id)
            .copied()
            .ok_or(EditRefusal::UnknownComponent)?;
        let mut after = before;
        change(&mut after);
        if after != before {
            let schema_changed = ParameterId::COMPONENT
                .into_iter()
                .any(|p| before.get(p).is_some() != after.get(p).is_some());
            let next_generation = if schema_changed {
                Some(
                    self.row_generations
                        .get(&id)
                        .ok_or(EditRefusal::UnknownComponent)?
                        .checked_add(1)
                        .ok_or(EditRefusal::IdentitiesExhausted)?,
                )
            } else {
                None
            };
            if !self.draft.replace(id, after)? {
                return Err(EditRefusal::UnknownComponent);
            }
            if let Some(generation) = next_generation {
                self.row_generations.insert(id, generation);
                self.refresh_fields(id, after, generation);
            }
            let staged = self.staged.len();
            self.staged.retain(|field, _| match field {
                EditingField::Parameter(owner, p) if *owner == id => after.get(*p).is_some(),
                _ => true,
            });
            if staged != self.staged.len() {
                self.input_revision += 1;
            }
            self.draft_revision += 1;
        }
        Ok(())
    }

    /// Change a component’s digital channel routing.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn set_routing(&mut self, row: RowKey, route: ChannelRouting) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        if self.draft.component(id).is_some_and(|c| {
            matches!(
                c,
                auditorylab_core::ComponentForm {
                    source: auditorylab_core::SoundSource::BinauralPair { .. },
                    modulation: _
                }
            )
        }) {
            return Err(EditRefusal::Inapplicable);
        }
        self.structural(row, |c| match c {
            auditorylab_core::ComponentForm {
                source:
                    auditorylab_core::SoundSource::Tone {
                        route: destination, ..
                    }
                    | auditorylab_core::SoundSource::PulseTrain {
                        route: destination, ..
                    }
                    | auditorylab_core::SoundSource::MonauralPair {
                        route: destination, ..
                    },
                modulation: _,
            } => *destination = route,
            auditorylab_core::ComponentForm {
                source: auditorylab_core::SoundSource::BinauralPair { .. },
                modulation: _,
            } => {}
        })
    }

    /// Give a component another carrier kind with that kind's defaults.
    /// Staged text of quantities the new carrier lacks is discarded.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale component lifetime or unknown component.
    pub fn set_carrier(
        &mut self,
        row: RowKey,
        template: auditorylab_core::ComponentForm,
    ) -> Result<(), EditRefusal> {
        let kind = template.primitive_kind().ok_or(EditRefusal::Inapplicable)?;
        self.structural(row, |form| {
            if form.primitive_kind() == Some(kind) {
                return;
            }
            let mut replacement = template;
            if form.primitive_kind().is_some() {
                let route = form
                    .route()
                    .unwrap_or_else(|| unreachable!("primitive route"));
                match &mut replacement {
                    auditorylab_core::ComponentForm {
                        source:
                            auditorylab_core::SoundSource::Tone {
                                route: destination, ..
                            }
                            | auditorylab_core::SoundSource::PulseTrain {
                                route: destination, ..
                            },
                        modulation: _,
                    } => *destination = route,
                    _ => unreachable!("default primitive"),
                }
                let level = form
                    .get(ParameterId::PeakLevel)
                    .unwrap_or_else(|| unreachable!("primitive level"));
                replacement.set(ParameterId::PeakLevel, level);
                replacement.set_modulation(form.modulation());
            }
            *form = replacement;
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
        modulation: Option<auditorylab_core::Modulation>,
    ) -> Result<(), EditRefusal> {
        self.structural(row, |form| {
            if form.modulation_kind() != modulation.map(|value| value.kind()) {
                form.set_modulation(modulation);
            }
        })
    }

    // Only the requested window's staged quantities participate in interpretation
    // changes. Other staged text stays staged, including incomplete input.
    pub(super) fn prepared_window(
        &self,
        id: ComponentId,
        modulation: bool,
    ) -> Result<(auditorylab_core::ComponentForm, [EditingField; 2]), EditRefusal> {
        let mut form = *self
            .draft
            .component(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let current = *pulse_definition(&mut form, modulation).ok_or(EditRefusal::Inapplicable)?;
        let rate = if modulation {
            ParameterId::ModulationPulseRate
        } else {
            ParameterId::SourcePulseRate
        };
        let width = match (modulation, current.width) {
            (true, auditorylab_core::PulseWidth::Duration { .. }) => {
                ParameterId::ModulationPulseDuration
            }
            (false, auditorylab_core::PulseWidth::Duration { .. }) => {
                ParameterId::SourcePulseDuration
            }
            (true, auditorylab_core::PulseWidth::DutyCycle { .. }) => ParameterId::ModulationDuty,
            (false, auditorylab_core::PulseWidth::DutyCycle { .. }) => ParameterId::SourceDuty,
        };
        let related = [
            EditingField::Parameter(id, rate),
            EditingField::Parameter(id, width),
        ];
        let candidate = self.parse();
        if candidate
            .problems
            .iter()
            .any(|(field, _)| related.contains(field))
        {
            return Err(EditRefusal::PulseConversion);
        }
        let mut parsed = *candidate
            .draft
            .component(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let parsed = *pulse_definition(&mut parsed, modulation).ok_or(EditRefusal::Inapplicable)?;
        let pulse = pulse_definition(&mut form, modulation).ok_or(EditRefusal::Inapplicable)?;
        pulse.rate_hz = parsed.rate_hz;
        pulse.width = parsed.width;
        Ok((form, related))
    }
    pub(super) fn consume_window_staging(&mut self, fields: [EditingField; 2]) {
        let mut changed = false;
        for field in fields {
            changed |= self.staged.remove(&field).is_some();
        }
        if changed {
            self.input_revision += 1;
        }
    }

    /// Change a pulse width interpretation without reinterpreting staged text.
    pub fn set_pulse_width_mode(
        &mut self,
        row: RowKey,
        modulation: bool,
        duty: bool,
    ) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        let mut current = *self
            .draft
            .component(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let pulse = pulse_definition(&mut current, modulation).ok_or(EditRefusal::Inapplicable)?;
        if matches!(pulse.width, auditorylab_core::PulseWidth::DutyCycle { .. }) == duty {
            return Ok(());
        }
        let (mut form, related) = self.prepared_window(id, modulation)?;
        let pulse = pulse_definition(&mut form, modulation).ok_or(EditRefusal::Inapplicable)?;
        *pulse = crate::document::construction::convert_width(*pulse, duty)
            .ok_or(EditRefusal::PulseConversion)?;
        self.structural(row, |value| *value = form)?;
        self.consume_window_staging(related);
        Ok(())
    }

    /// Change a pulse shape, retaining diagnosed sub-frame edges as editable candidates.
    pub fn set_pulse_shape(
        &mut self,
        row: RowKey,
        modulation: bool,
        shaped: bool,
    ) -> Result<(), EditRefusal> {
        let id = self.admit_component(row)?;
        let mut form = *self
            .draft
            .component(id)
            .ok_or(EditRefusal::UnknownComponent)?;
        let current = *pulse_definition(&mut form, modulation).ok_or(EditRefusal::Inapplicable)?;
        if matches!(
            current.shape,
            auditorylab_core::PulseShape::RaisedCosine { .. }
        ) == shaped
        {
            return Ok(());
        }
        let related = if shaped {
            let prepared = self.prepared_window(id, modulation).map_err(|error| {
                if error == EditRefusal::PulseConversion {
                    EditRefusal::PulseWindow
                } else {
                    error
                }
            })?;
            form = prepared.0;
            Some(prepared.1)
        } else {
            None
        };
        let pulse = pulse_definition(&mut form, modulation).ok_or(EditRefusal::Inapplicable)?;
        *pulse = crate::document::construction::set_pulse_shape(*pulse, shaped)
            .ok_or(EditRefusal::PulseWindow)?;
        self.structural(row, |value| *value = form)?;
        if let Some(related) = related {
            self.consume_window_staging(related);
        }
        Ok(())
    }

    /// Add a default component.
    ///
    /// # Errors
    /// Returns [`EditRefusal`] for a stale document, either capacity limit or
    /// exhausted identities.
    pub fn add_component(
        &mut self,
        row: RowKey,
        form: auditorylab_core::ComponentForm,
    ) -> Result<ComponentId, EditRefusal> {
        self.admit_document(row)?;
        let form = distinct_carrier(form, self.draft.components());
        let id = self.draft.add_node(form)?;
        self.row_generations.insert(id, 0);
        self.refresh_fields(id, form, 0);
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
        let before = self.draft.recipe.clone();
        let (index, component) = self.draft.remove(id).ok_or(EditRefusal::UnknownComponent)?;
        let after = self.draft.recipe.clone();
        let owned = |field: &EditingField| {
            matches!(field, EditingField::Parameter(owner,
            _) if *owner == id)
        };
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
        self.field_births.retain(|(owner, _), _| *owner != id);
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
            && self.draft.recipe.content() == removed.after.content();
        if let Err(refusal) = self.draft.restore(removed.index, removed.component) {
            self.removed = Some(removed);
            return Err(refusal);
        }
        self.row_generations.insert(removed.id, generation);
        self.refresh_fields(removed.id, removed.component.form, generation);
        if complete_undo {
            let document = self.draft.recipe.document.clone();
            self.draft.recipe = removed.before;
            self.draft.recipe.document = document;
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
        if document != self.document || content.document().id != self.draft.recipe.document.id {
            return false;
        }
        self.saved_nominal = Some(crate::core::rhythm::nominal(content.sound()));
        if &self.draft.recipe.document != content.document() {
            self.draft.recipe.document = content.document().clone();
            self.candidate = None;
            self.save_check = None;
        }
        if self.saved.as_ref() != content {
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

    pub(super) fn cached(&self, key: &AnalysisKey) -> Option<Arc<Analysis>> {
        self.analyses
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, analysis)| Arc::clone(analysis))
    }

    pub(super) fn store(&mut self, key: AnalysisKey, analysis: Arc<Analysis>) -> Arc<Analysis> {
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

fn pulse_definition(
    form: &mut auditorylab_core::ComponentForm,
    modulation: bool,
) -> Option<&mut auditorylab_core::PulseSpec> {
    if modulation {
        match &mut form.modulation {
            Some(auditorylab_core::Modulation::Pulsed { pulse }) => Some(pulse),
            _ => None,
        }
    } else {
        match &mut form.source {
            auditorylab_core::SoundSource::PulseTrain { pulse, .. } => Some(pulse),
            _ => None,
        }
    }
}

fn apply_number(
    draft: &mut Draft,
    field: EditingField,
    parameter: ParameterId,
    text: &str,
    problems: &mut Vec<(EditingField, InputProblem)>,
) {
    match crate::document::input::parse_parameter(parameter, text) {
        Ok(value) => {
            draft.set_value(field, value);
        }
        Err(problem) => problems.push((field, problem)),
    }
}

/// Carrier multiples tried, in order, for a new component whose carrier an
/// existing component already uses.
const CARRIER_STEPS: [f64; 8] = [1.0, 1.25, 1.5, 2.0, 0.75, 2.5, 3.0, 0.5];

/// Move a new component's carrier to the first step no existing component
/// uses, so an added default is audibly distinct rather than only louder.
/// Pulse trains keep their rate: changing it would change their rhythm.
fn distinct_carrier(
    mut form: auditorylab_core::ComponentForm,
    existing: &[auditorylab_core::Component],
) -> auditorylab_core::ComponentForm {
    fn carrier(
        source: &mut auditorylab_core::SoundSource,
    ) -> Option<&mut auditorylab_core::Finite> {
        use auditorylab_core::SoundSource as S;
        match source {
            S::Tone { frequency_hz, .. } => Some(frequency_hz),
            S::BinauralPair { first_hz, .. } | S::MonauralPair { first_hz, .. } => Some(first_hz),
            S::PulseTrain { .. } => None,
        }
    }
    let used: Vec<f64> = existing
        .iter()
        .filter_map(|component| {
            let mut source = component.form.source;
            carrier(&mut source).map(|hz| hz.get())
        })
        .collect();
    if let Some(hz) = carrier(&mut form.source) {
        let base = hz.get();
        if let Some(free) = CARRIER_STEPS
            .iter()
            .map(|step| base * step)
            .find(|candidate| !used.contains(candidate))
            .and_then(auditorylab_core::Finite::new)
        {
            *hz = free;
        }
    }
    form
}
