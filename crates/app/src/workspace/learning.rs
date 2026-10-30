//! Learning is a visible surface with no implicit working-source authority.
use super::{
    Command, DocumentAction, EditorCommand, Effect, FileCommand, Refusal, View, Workspace,
    WorkspaceActions,
};
use crate::learning::{LearningCommand, LearningState};

impl Workspace {
    /// The optional visible learning destination; `view()` retains the return context.
    pub const fn learning(&self) -> Option<LearningState> {
        self.learning
    }

    pub(super) fn leave_learning(&mut self) {
        if self.learning.take().is_some() {
            self.effects.push(Effect::LearningDeparted);
            self.changed();
        }
    }

    pub(super) fn apply_learning(&mut self, command: LearningCommand) -> Result<(), Refusal> {
        match command {
            LearningCommand::Open(topic) => {
                self.learning = Some(LearningState {
                    topic,
                    construction: self
                        .learning
                        .map_or(crate::learning::Construction::SteadyTone, |state| {
                            state.construction
                        }),
                    band: self
                        .learning
                        .map_or(crate::core::rhythm::Band::Alpha, |state| state.band),
                });
                self.changed();
            }
            LearningCommand::SelectConstruction(construction) => {
                if let Some(state) = &mut self.learning {
                    state.construction = construction;
                    self.changed();
                }
            }
            LearningCommand::SelectBand(band) => {
                if let Some(state) = &mut self.learning {
                    state.band = band;
                    self.changed();
                }
            }
            LearningCommand::ReturnToWork => self.set_view(self.view),
            LearningCommand::BrowseLibrary => self.set_view(View::Library),
            LearningCommand::CreateExample(example) => {
                if self.learning.is_none() {
                    return Err(Refusal::SelectionChanged);
                }
                self.start_allowed()?;
                let candidate = example
                    .recipe()
                    .map_err(|error| Refusal::Document(Box::new(error)))?;
                self.request(DocumentAction::LearningExample(candidate))?;
            }
        }
        Ok(())
    }

    pub(super) fn learning_command_admission(&self, command: &Command) -> Result<(), Refusal> {
        if self.learning.is_none() {
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
