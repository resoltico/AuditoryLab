//! Side-effect-free operation preparation shared by actions and execution.
use super::*;

pub(super) enum PreparedExport {
    Library(Sound, String),
    Draft(Draft, Arc<auditorylab_core::Signal>, String),
}

impl PreparedExport {
    pub(super) fn capture(self) -> (Sound, String) {
        match self {
            Self::Library(sound, name) => (sound, name),
            Self::Draft(draft, signal, name) => (Sound::from_parts(draft.snapshot(), signal), name),
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
        if count > Stimulus::MAX_COMPONENTS {
            return Err(Refusal::TooManyComponents(count));
        }
        if let Some((component, parameter)) = snapshot.source_stimulus().nonfinite().next() {
            return Err(Refusal::SourceNotFinite {
                component,
                parameter,
            });
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
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
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
                Ok(PreparedExport::Draft(
                    candidate.draft().clone(),
                    signal,
                    name,
                ))
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
                let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
                if snapshot.stimulus().nonfinite().next().is_some() {
                    return Err(Refusal::NotFinite);
                }
                Ok(AudioRequest::Finite(snapshot.clone()))
            }
            View::Workbench => {
                let candidate = self.editor.candidate();
                if !candidate.is_complete() {
                    return Err(Refusal::Input(candidate.problems().to_vec()));
                }
                Ok(AudioRequest::Live {
                    document: self.editor.document(),
                    draft: candidate.draft().clone(),
                })
            }
        }
    }
}
