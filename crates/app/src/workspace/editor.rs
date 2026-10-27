//! The editable document, location and editor admission/transitions.
use super::*;
#[derive(Debug, Clone, Default)]
pub(super) enum DocumentLocation {
    #[default]
    Unsaved,
    Managed {
        key: crate::document::origin::LibraryKey,
        token: crate::document::origin::VersionToken,
    },
    External(crate::document::origin::ObservedFile),
    Detached(crate::document::origin::LibraryKey),
}
impl DocumentLocation {
    pub(super) fn managed(
        &self,
    ) -> Option<(
        &crate::document::origin::LibraryKey,
        crate::document::origin::VersionToken,
    )> {
        match self {
            Self::Managed { key, token } => Some((key, *token)),
            _ => None,
        }
    }
    pub(super) fn detached(&self) -> Option<&crate::document::origin::LibraryKey> {
        match self {
            Self::Detached(key) => Some(key),
            _ => None,
        }
    }
    pub(super) fn external(&self) -> Option<&crate::document::origin::ObservedFile> {
        match self {
            Self::External(file) => Some(file),
            _ => None,
        }
    }
    pub(super) fn saved_token(
        &self,
        key: &crate::document::origin::LibraryKey,
    ) -> Option<crate::document::origin::VersionToken> {
        self.managed()
            .filter(|(saved, _)| *saved == key)
            .map(|(_, token)| token)
    }
}
pub(super) struct EditorState {
    pub(super) session: EditorSession,
    pub(super) export_rate: SampleRate,
    pub(super) status: EditStatus,
    pub(super) location: DocumentLocation,
}
impl EditorState {
    pub(super) fn new() -> Result<Self, crate::document::error::EditorError> {
        Ok(Self {
            session: EditorSession::new(Draft::new()?)?,
            export_rate: SampleRate::DEFAULT,
            status: EditStatus::Idle,
            location: DocumentLocation::Unsaved,
        })
    }
    pub(super) fn editing_admission(
        &self,
        prompts: &Prompts,
        pending: bool,
        files: &FileState,
    ) -> Result<(), Refusal> {
        prompts.admit_start(pending)?;
        if files.blocks_edit() {
            return Err(Refusal::FileBusy);
        }
        Ok(())
    }
    pub(super) fn finish_edit(&mut self, result: &Result<(), Refusal>) {
        self.status = match result {
            Ok(()) | Err(Refusal::Input(_)) => EditStatus::Idle,
            Err(refusal) => EditStatus::Refused(refusal.clone()),
        };
    }
    pub(super) fn install(&mut self, session: EditorSession) {
        self.session = session;
        self.location = DocumentLocation::Unsaved;
        self.status = EditStatus::Idle;
    }
}

impl EditorState {
    pub(super) fn stage(&mut self, handle: FieldHandle, text: &str) -> Result<(), Refusal> {
        self.session.stage(handle, text).map_err(Refusal::Edit)
    }
    pub(super) fn restore(&mut self, handle: FieldHandle) -> Result<String, Refusal> {
        self.session.restore(handle).map_err(Refusal::Edit)
    }
    pub(super) fn commit_field(&mut self, handle: FieldHandle, text: &str) -> Result<(), Refusal> {
        match self.session.commit_field(handle, text) {
            Err(refusal) => Err(Refusal::Edit(refusal)),
            Ok(Err(CommitRefusal { problems })) => Err(Refusal::Input(problems)),
            Ok(Ok(_)) => Ok(()),
        }
    }
    pub(super) fn commit(&mut self) -> Result<(), Refusal> {
        self.session
            .commit()
            .map(|_| ())
            .map_err(|CommitRefusal { problems }| Refusal::Input(problems))
    }
    pub(super) fn set_ears(&mut self, row: RowKey, ears: Ears) -> Result<(), Refusal> {
        self.session.set_ears(row, ears).map_err(Refusal::Edit)
    }
    pub(super) fn set_carrier(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.session
            .set_carrier(row, super::controls::default_component(kind))
            .map_err(Refusal::Edit)
    }
    pub(super) fn set_modulation(
        &mut self,
        row: RowKey,
        kind: Option<ModulationKind>,
        rate: SampleRate,
    ) -> Result<(), Refusal> {
        self.session
            .set_modulation(
                row,
                kind.map(|kind| super::controls::default_modulation(kind, rate)),
            )
            .map_err(Refusal::Edit)
    }
    pub(super) fn add_component(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.session
            .add_component(row, super::controls::default_component(kind))
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
    pub(super) fn add_pair(
        &mut self,
        row: RowKey,
        kind: auditorylab_core::PairKind,
    ) -> Result<(), Refusal> {
        self.session
            .add_component(row, super::controls::default_pair(kind))
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
    pub(super) fn remove_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.session.remove_component(row).map_err(Refusal::Edit)
    }
    pub(super) fn restore_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.session
            .restore_removed(row)
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
}

impl EditorState {
    fn save_refusal(refusal: SaveRefusal) -> Refusal {
        match refusal {
            SaveRefusal::Input(problems) => Refusal::Input(problems),
            SaveRefusal::Document(error) => Refusal::Document(Box::new(error.into())),
        }
    }
    pub(super) fn prepare_recipe(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.session.save_readiness().map_err(Self::save_refusal)?;
        self.session
            .candidate()
            .draft()
            .recipe()
            .map(Arc::new)
            .map_err(|error| Refusal::Document(Box::new(error.into())))
    }
    pub(super) fn prepare_save(&mut self) -> Result<Arc<Recipe>, Refusal> {
        if let Some(key) = self.location.detached() {
            return Err(Refusal::DetachedDraft(key.clone()));
        }
        self.prepare_recipe()
    }
    pub(super) fn prepare_recipe_text(&mut self) -> Result<(Arc<str>, String), Refusal> {
        let text = self
            .session
            .serialized_candidate()
            .map_err(Self::save_refusal)?;
        Ok((text, self.session.candidate().draft().title().into()))
    }
    pub(super) fn prepare_live(&mut self) -> Result<AudioRequest, Refusal> {
        let candidate = self.session.candidate();
        if !candidate.is_complete() {
            return Err(Refusal::Input(candidate.problems().to_vec()));
        }
        Ok(AudioRequest::Live {
            document: self.session.document(),
            candidate,
        })
    }
    pub(super) fn prepare_export(&mut self) -> Result<file::PreparedExport, Refusal> {
        let rate = self.export_rate;
        let candidate = self.session.candidate();
        if !candidate.is_complete() {
            return Err(Refusal::Input(candidate.problems().to_vec()));
        }
        let analysis = self.session.candidate_analysis(rate);
        let signal = analysis.signal().cloned().ok_or_else(|| Refusal::Invalid {
            title: candidate.draft().title().into(),
            rate,
            error: analysis.finite_error().cloned(),
        })?;
        Ok(file::PreparedExport::Draft {
            origin: crate::document::recipe::Origin::Draft(candidate.draft().title().into()),
            recipe: candidate
                .draft()
                .recipe()
                .map_err(|error| Refusal::Document(Box::new(error.into())))?,
            signal,
            name: file_name(candidate.draft().title(), "wav"),
        })
    }
    pub(super) fn addition_admission(
        &self,
        row: RowKey,
        form: crate::core::ComponentForm,
        prompts: &Prompts,
        pending: bool,
        files: &FileState,
    ) -> Result<(), Refusal> {
        self.editing_admission(prompts, pending, files)?;
        self.session.admit_document(row).map_err(Refusal::Edit)?;
        self.session
            .draft()
            .admit_new_component(form)
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
    pub(super) fn restore_admission(
        &self,
        row: RowKey,
        prompts: &Prompts,
        pending: bool,
        files: &FileState,
    ) -> Result<(), Refusal> {
        self.editing_admission(prompts, pending, files)?;
        self.session
            .restore_admission(row)
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
}

impl EditorState {
    pub(super) fn record_managed(
        &mut self,
        document: Option<DocumentId>,
        key: &crate::document::origin::LibraryKey,
        kind: crate::services::library::MutationKind,
        record: &crate::services::library::LoadedRecord,
        feedback: &mut Vec<SaveFeedback>,
    ) -> bool {
        use crate::services::library::MutationKind;
        if document.is_some()
            && let Err(error) = record.snapshot.at(self.export_rate)
        {
            feedback.push(SaveFeedback::InvalidAtRate {
                rate: self.export_rate,
                error,
            });
        }
        if let Some(document) = document {
            let same_document = self.session.document() == document;
            if same_document
                && !self
                    .session
                    .record_saved(document, record.snapshot.recipe())
            {
                return false;
            }
            if same_document {
                self.location = DocumentLocation::Managed {
                    key: key.clone(),
                    token: record.version_token,
                };
            }
        } else if self
            .location
            .managed()
            .is_some_and(|(saved, _)| saved == key)
        {
            // Membership changes do not alter portable history or the live draft.
            // A dirty rename keeps its old expected token so Save exposes conflict.
            if matches!(kind, MutationKind::Trash) {
                self.location = DocumentLocation::Detached(key.clone());
            } else if !self.session.dirty() {
                match EditorSession::new(Draft::from_recipe(record.snapshot.recipe().clone())) {
                    Ok(editor) => {
                        self.session = editor;
                        self.status = EditStatus::Idle;
                        self.session
                            .record_saved(self.session.document(), record.snapshot.recipe());
                        self.location = DocumentLocation::Managed {
                            key: key.clone(),
                            token: record.version_token,
                        };
                    }
                    Err(error) => feedback.push(SaveFeedback::EditorRefresh(error)),
                }
            }
        }
        true
    }
    pub(super) fn set_export_rate(&mut self, rate: SampleRate) -> bool {
        if self.export_rate == rate {
            return false;
        }
        self.export_rate = rate;
        true
    }
}

impl Workspace {
    /// The open document's editor.
    #[must_use]
    pub const fn editor(&self) -> &EditorSession {
        &self.editor.session
    }
    /// The selected inspection and export rate; device rates are separate.
    #[must_use]
    pub const fn export_rate(&self) -> SampleRate {
        self.editor.export_rate
    }
    /// Select the inspection and export rate. A device's actual rate is
    /// unaffected.
    pub fn set_export_rate(&mut self, rate: SampleRate) {
        if self.editor.set_export_rate(rate) {
            self.changed();
        }
    }
    pub(super) fn editing_allowed(&self) -> Result<(), Refusal> {
        self.editor
            .editing_admission(&self.prompts, self.pending.is_some(), &self.files)
    }
    /// Whether editing commands are accepted now.
    #[must_use]
    pub fn can_edit(&self) -> bool {
        self.editing_allowed().is_ok()
    }
    pub(super) fn after_edit(&mut self, result: Result<(), Refusal>) -> Result<(), Refusal> {
        if result.is_ok() {
            self.submit_live();
        }
        self.editor.finish_edit(&result);
        self.changed();
        result
    }
    /// Stage typed text. Staging changes diagnostics and readiness only.
    ///
    /// # Errors
    /// Returns the refusal for stale handles or while editing is blocked.
    pub fn stage(&mut self, handle: FieldHandle, text: &str) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.stage(handle, text);
        self.changed();
        result
    }
    /// Discard a field's staged text; returns its committed text.
    ///
    /// # Errors
    /// Returns the refusal for stale handles.
    pub fn restore(&mut self, handle: FieldHandle) -> Result<String, Refusal> {
        self.editing_allowed()?;
        let result = self.editor.restore(handle);
        self.changed();
        result
    }
    /// Stage a field's text and commit the whole candidate.
    ///
    /// # Errors
    /// Returns the refusal, including every field that kept the candidate
    /// staged.
    pub fn commit_field(&mut self, handle: FieldHandle, text: &str) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.commit_field(handle, text);
        self.after_edit(result)
    }
    /// Commit the whole candidate.
    ///
    /// # Errors
    /// Returns every field that kept the candidate staged.
    pub fn commit(&mut self) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.commit();
        self.after_edit(result)
    }
    /// Apply a slider gesture: the shared comfortable span and gesture
    /// rounding give the value, which is staged and committed.
    ///
    /// # Errors
    /// Returns the refusal for stale handles or a blocked commit.
    pub fn slide(&mut self, handle: FieldHandle, position: f64) -> Result<(), Refusal> {
        let EditingField::Parameter(_, parameter) = handle.field else {
            return Err(Refusal::Edit(EditRefusal::Inapplicable));
        };
        let Some(value) = controls::slider_value(parameter, self.diagnostic_rate(), position)
        else {
            return Err(Refusal::Edit(EditRefusal::Inapplicable));
        };
        self.commit_field(handle, &value.to_string())
    }
    /// Route a component to other ears.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_ears(&mut self, row: RowKey, ears: Ears) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.set_ears(row, ears);
        self.after_edit(result)
    }
    /// Change a component's carrier kind.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_carrier(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.set_carrier(row, kind);
        self.after_edit(result)
    }
    /// Change a component's modulation kind, or remove its modulation.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_modulation(
        &mut self,
        row: RowKey,
        kind: Option<ModulationKind>,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let rate = self.diagnostic_rate();
        let result = self.editor.set_modulation(row, kind, rate);
        self.after_edit(result)
    }
    /// Add a default component.
    ///
    /// # Errors
    /// Returns the refusal at the component limit or while editing is blocked.
    pub fn add_component(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.component_add_admission(row, kind)?;
        let result = self.editor.add_component(row, kind);
        self.after_edit(result)
    }
    /// Add a complete pair as one committed edit/live target submission.
    ///
    /// # Errors
    /// Refuses stale documents, busy editing or the expanded voice budget.
    pub fn add_pair(
        &mut self,
        row: RowKey,
        kind: auditorylab_core::PairKind,
    ) -> Result<(), Refusal> {
        self.pair_add_admission(row, kind)?;
        let result = self.editor.add_pair(row, kind);
        self.after_edit(result)
    }
    /// Remove a component.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn remove_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self.editor.remove_component(row);
        self.after_edit(result)
    }
    /// Put the most recently removed component back where it was.
    ///
    /// # Errors
    /// Returns the refusal when nothing was removed, at the component limit
    /// or while editing is blocked.
    pub fn restore_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.restore_component_admission(row)?;
        let result = self.editor.restore_component(row);
        self.after_edit(result)
    }
    /// The live session's actual rate, when a live task is bound to the open
    /// document; otherwise the selected export rate. Field diagnostics use it.
    #[must_use]
    pub fn diagnostic_rate(&self) -> SampleRate {
        self.live_rate().unwrap_or(self.editor.export_rate)
    }
    /// Show the recipe the open document was copied from, by its retained
    /// snapshot rather than by its path.
    pub fn show_based_on_source(&mut self) {
        if let Some(source) = self.editor.session.draft().based_on().cloned() {
            self.reveal_retained_source(source);
        }
    }
}

impl EditorState {
    pub(super) fn dirty_managed(&self, key: &crate::document::origin::LibraryKey) -> bool {
        self.session.dirty()
            && self
                .location
                .managed()
                .is_some_and(|(saved, _)| saved == key)
    }
}
