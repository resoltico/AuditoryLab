//! Learning is a visible surface with no implicit working-source authority.
use super::{
    Command, DocumentAction, EditorCommand, Effect, FileCommand, Refusal, View, Workspace,
    WorkspaceActions,
};
use crate::learning::{LearningCommand, LearningState};

impl Workspace {
    /// The optional visible learning destination; `view()` retains the return context.
    pub const fn learning(&self) -> Option<LearningState> {
        if self.learning_visible {
            Some(self.learning)
        } else {
            None
        }
    }

    pub(super) fn leave_learning(&mut self) {
        if self.learning_visible {
            self.learning_visible = false;
            self.effects.push(Effect::LearningDeparted);
            self.changed();
        }
    }

    pub(super) fn apply_learning(&mut self, command: LearningCommand) -> Result<(), Refusal> {
        match command {
            LearningCommand::Resume => {
                if !self.learning_visible {
                    self.learning_visible = true;
                    self.changed();
                }
            }
            LearningCommand::Open(topic) => {
                if !self.learning_visible || self.learning.topic != topic {
                    self.learning.topic = topic;
                    self.learning_visible = true;
                    self.changed();
                }
            }
            LearningCommand::SelectConstruction(construction) => {
                if self.learning_visible && self.learning.construction != construction {
                    self.learning.construction = construction;
                    self.changed();
                }
            }
            LearningCommand::SelectBand(band) => {
                if self.learning_visible && self.learning.band != band {
                    self.learning.band = band;
                    self.changed();
                }
            }
            LearningCommand::BrowseLibrary => self.set_view(View::Library),
            LearningCommand::CreateExample(example) => {
                if !self.learning_visible {
                    return Err(Refusal::SelectionChanged);
                }
                self.start_allowed()?;
                let candidate = self.resolve_learning_capture(example.recipe())?;
                self.request(DocumentAction::LearningExample(candidate))?;
            }
        }
        Ok(())
    }

    /// Preserve a construction producer failure in the normal visible file status.
    pub(super) fn resolve_learning_capture(
        &mut self,
        captured: Result<crate::core::Recipe, crate::services::OperationError>,
    ) -> Result<crate::core::Recipe, Refusal> {
        captured.map_err(|error| {
            self.files
                .record_status(super::FileStatus::Failed(error.clone()));
            self.changed();
            Refusal::Document(Box::new(error))
        })
    }

    pub(super) fn learning_command_admission(&self, command: &Command) -> Result<(), Refusal> {
        if !self.learning_visible {
            return Ok(());
        }
        let allowed = match command {
            Command::Audio(super::AudioCommand::Toggle) => self.audio.busy(),
            Command::Output(super::OutputCommand::Preview) => false,
            Command::Learning(_)
            | Command::SetView(_)
            | Command::Answer { .. }
            | Command::Close
            | Command::Audio(super::AudioCommand::Stop)
            | Command::File(FileCommand::CancelExport | FileCommand::Import)
            | Command::Editor(EditorCommand::New)
            | Command::Output(_) => true,
            _ => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(Refusal::LearningVisible)
        }
    }

    pub(super) fn suppress_learning_actions(view: &mut WorkspaceActions) {
        view.play_reason = Some(Refusal::LearningVisible);
        view.save_reason = Some(Refusal::LearningVisible);
        view.export_reason = Some(Refusal::LearningVisible);
        view.copy_reason = Some(Refusal::LearningVisible);
        view.save_copy = Err(Refusal::LearningVisible);
        view.export_recipe = Err(Refusal::LearningVisible);
        view.editing = Err(Refusal::LearningVisible);
        view.preview = Err(Refusal::LearningVisible);
        view.library = Err(Refusal::LearningVisible);
        view.add_to_library = Err(Refusal::LearningVisible);
        view.trash = Err(Refusal::LearningVisible);
        view.restore_component = Err(Refusal::LearningVisible);
        view.add_component_reason = Some(Refusal::LearningVisible);
        view.pair_add_reason = Some(Refusal::LearningVisible);
        view.play_target.clear();
    }
}
