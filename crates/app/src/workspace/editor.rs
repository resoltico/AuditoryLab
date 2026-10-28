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
    session: EditorSession,
    export_rate: SampleRate,
    status: EditStatus,
    location: DocumentLocation,
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
    ) -> Result<(), Refusal> {
        self.session.admit_document(row).map_err(Refusal::Edit)?;
        self.session
            .draft()
            .admit_new_component(form)
            .map(|_| ())
            .map_err(Refusal::Edit)
    }
    pub(super) fn restore_admission(&self, row: RowKey) -> Result<(), Refusal> {
        self.session
            .restore_admission(row)
            .map(|_| ())
            .map_err(Refusal::Edit)
    }

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

    pub(super) fn dirty_managed(&self, key: &crate::document::origin::LibraryKey) -> bool {
        self.session.dirty()
            && self
                .location
                .managed()
                .is_some_and(|(saved, _)| saved == key)
    }

    pub(super) const fn session(&self) -> &EditorSession {
        &self.session
    }
    pub(super) const fn export_rate(&self) -> SampleRate {
        self.export_rate
    }
    pub(super) const fn status(&self) -> &EditStatus {
        &self.status
    }
    pub(super) const fn location(&self) -> &DocumentLocation {
        &self.location
    }
    pub(super) fn locate(&mut self, location: DocumentLocation) {
        self.location = location;
    }
    pub(super) fn candidate(&mut self) -> Arc<crate::document::editing::Candidate> {
        self.session.candidate()
    }
    pub(super) fn candidate_analysis(
        &mut self,
        rate: SampleRate,
    ) -> Arc<crate::document::editing::Analysis> {
        self.session.candidate_analysis(rate)
    }
    pub(super) fn committed_analysis(
        &mut self,
        rate: SampleRate,
    ) -> Arc<crate::document::editing::Analysis> {
        self.session.committed_analysis(rate)
    }
    pub(super) fn nominal(&mut self) -> Arc<crate::core::rhythm::Analysis> {
        self.session.nominal()
    }
    pub(super) fn record_saved(&mut self, document: DocumentId, recipe: &Recipe) -> bool {
        self.session.record_saved(document, recipe)
    }

    pub(super) fn slider_command(
        field: FieldHandle,
        position: f64,
        rate: SampleRate,
    ) -> Result<EditorCommand, Refusal> {
        let EditingField::Parameter(_, parameter) = field.field else {
            return Err(Refusal::Edit(EditRefusal::Inapplicable));
        };
        let value = controls::slider_value(parameter, rate, position)
            .ok_or(Refusal::Edit(EditRefusal::Inapplicable))?;
        Ok(EditorCommand::CommitField {
            field,
            text: value.to_string(),
        })
    }
    pub(super) fn apply(&mut self, command: EditorCommand, rate: SampleRate) -> EditorTransition {
        use EditorTransition as T;
        match command {
            EditorCommand::New => T::Request(EditorRequest::New),
            EditorCommand::CopySelected => T::Request(EditorRequest::CopySelected),
            EditorCommand::ShowBasedOnSource => T::Request(EditorRequest::ShowBasedOnSource),
            EditorCommand::SetExportRate(rate) => T::RateChanged(self.set_export_rate(rate)),
            EditorCommand::Stage { field, text } => T::Staged(self.stage(field, &text)),
            EditorCommand::Restore(field) => T::Staged(self.restore(field).map(|_| ())),
            EditorCommand::CommitField { field, text } => {
                T::Committed(self.commit_field(field, &text))
            }
            EditorCommand::Commit => T::Committed(self.commit()),
            EditorCommand::Slide { field, position } => {
                match Self::slider_command(field, position, rate) {
                    Ok(command) => self.apply(command, rate),
                    Err(refusal) => T::Refused(refusal),
                }
            }
            EditorCommand::SetEars { row, ears } => T::Committed(self.set_ears(row, ears)),
            EditorCommand::SetCarrier { row, kind } => T::Committed(self.set_carrier(row, kind)),
            EditorCommand::SetModulation { row, kind } => {
                T::Committed(self.set_modulation(row, kind, rate))
            }
            EditorCommand::AddComponent { row, kind } => {
                T::Committed(self.add_component(row, kind))
            }
            EditorCommand::AddPair { row, kind } => T::Committed(self.add_pair(row, kind)),
            EditorCommand::RemoveComponent(row) => T::Committed(self.remove_component(row)),
            EditorCommand::RestoreComponent(row) => T::Committed(self.restore_component(row)),
        }
    }
}

pub(super) enum EditorRequest {
    New,
    CopySelected,
    ShowBasedOnSource,
}
pub(super) enum EditorTransition {
    Request(EditorRequest),
    RateChanged(bool),
    Staged(Result<(), Refusal>),
    Committed(Result<(), Refusal>),
    Refused(Refusal),
}
