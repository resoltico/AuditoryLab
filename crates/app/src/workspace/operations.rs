//! Side-effect-free operation preparation shared by actions and execution.
use super::*;

pub(super) enum PreparedExport {
    Library(Rendition, String),
    Draft(
        Arc<crate::editing::Candidate>,
        Arc<auditorylab_core::Signal>,
        String,
    ),
}

impl PreparedExport {
    pub(super) fn capture(self) -> (Rendition, String) {
        match self {
            Self::Library(sound, name) => (sound, name),
            Self::Draft(draft, signal, name) => (
                Rendition::from_parts(draft.draft().snapshot(), signal),
                name,
            ),
        }
    }
}

impl Workspace {
    pub(super) fn copy_admission(&self) -> Result<(), Refusal> {
        self.start_allowed()?;
        Self::copy_content_admission(self.selected().ok_or(Refusal::NoSelection)?)
    }

    pub(super) fn copy_content_admission(snapshot: &RecipeSnapshot) -> Result<(), Refusal> {
        let count = snapshot.stimulus().components.len();
        if count > Sound::MAX_COMPONENTS {
            return Err(Refusal::TooManyComponents(count));
        }
        if let Some((component, parameter)) = snapshot.stimulus().nonfinite().next() {
            return Err(Refusal::NonfiniteContent {
                component,
                parameter,
            });
        }
        snapshot
            .recipe()
            .validate()
            .map_err(|e| Refusal::Document(e.to_string()))?;
        if snapshot.recipe().lineage.len() >= auditorylab_core::provenance::MAX_LINEAGE {
            return Err(Refusal::Document(
                "Lineage capacity reached; review metadata before making a copy.".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn import_admission(&self) -> Result<(), Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        Ok(())
    }

    pub(super) fn prepare_save(&mut self) -> Result<Arc<Recipe>, Refusal> {
        if let Some(error) = &self.persistent_config_error {
            return Err(Refusal::Document(format!(
                "User Library unavailable: {error}"
            )));
        }
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        if let Some(key) = self.persistent.as_ref().and_then(|p| p.detached.as_ref()) {
            return Err(Refusal::Document(format!(
                "Detached draft from {key}. Edit the restored saved entry or Save independent copy."
            )));
        }
        if self.has_unconfirmed_publication() {
            return Err(Refusal::Document("Publication unconfirmed. Recheck before retrying, reload an observed version, or save an independent copy.".into()));
        }
        self.prepare_recipe()
    }
    pub(super) fn prepare_recipe(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.editor
            .save_readiness()
            .map_err(|refusal| match refusal {
                SaveRefusal::Input(problems) => Refusal::Input(problems),
                SaveRefusal::Document(message) => Refusal::Document(message),
            })?;
        Ok(Arc::new(self.editor.candidate().draft().recipe()))
    }

    pub(super) fn prepare_export(&mut self) -> Result<PreparedExport, Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        if self.audio.is_some() {
            return Err(Refusal::AudioPlaying);
        }
        let rate = self.export_rate;
        match self.view {
            View::Library => {
                let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
                let name = snapshot.suggested_output_name("wav");
                let sound = snapshot.realize(rate).map_err(|error| Refusal::Invalid {
                    title: snapshot.title(),
                    rate,
                    error: Some(error),
                })?;
                Ok(PreparedExport::Library(sound, name))
            }
            View::Workbench => {
                let candidate = self.editor.candidate();
                if !candidate.is_complete() {
                    return Err(Refusal::Input(candidate.problems().to_vec()));
                }
                let analysis = self.editor.candidate_analysis(rate);
                let signal = analysis.signal().cloned().ok_or_else(|| Refusal::Invalid {
                    title: candidate.draft().title().to_owned(),
                    rate,
                    error: analysis.finite_error().cloned(),
                })?;
                let name = file_name(candidate.draft().title(), "wav");
                Ok(PreparedExport::Draft(candidate, signal, name))
            }
        }
    }

    pub(super) fn prepare_audio(&mut self) -> Result<AudioRequest, Refusal> {
        if !self.capabilities.device_playback {
            return Err(Refusal::DevicePlaybackUnavailable);
        }
        self.start_allowed()?;
        if self.exporting() {
            return Err(Refusal::Exporting);
        }
        match self.view {
            View::Library => {
                if self.selected_unreadable() {
                    return Err(Refusal::Document("Saved entry is unreadable; the retained snapshot is for inspection. Refresh or recover an independent copy before playback.".into()));
                }
                if self.selected_trashed() {
                    return Err(Refusal::Document(
                        "Restore this saved recipe before playback.".into(),
                    ));
                }
                let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
                if snapshot.stimulus().nonfinite().next().is_some() {
                    return Err(Refusal::NotFinite);
                }
                let store = self
                    .persistent
                    .as_ref()
                    .filter(|p| p.selected_user())
                    .map(|p| p.store_clone());
                Ok(AudioRequest::Finite {
                    snapshot: Box::new(snapshot.clone()),
                    store,
                })
            }
            View::Workbench => {
                let candidate = self.editor.candidate();
                if !candidate.is_complete() {
                    return Err(Refusal::Input(candidate.problems().to_vec()));
                }
                Ok(AudioRequest::Live {
                    document: self.editor.document(),
                    candidate,
                })
            }
        }
    }
}
