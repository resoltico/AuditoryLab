//! Typed user input; handlers retain their owner-specific admission/status rules.
use super::library::{LibraryJob, LibraryRequest};
use super::{
    Action, ActionView, Arc, AudioRequest, AudioStatus, Capabilities, CarrierKind, ChannelRouting,
    ChoiceOption, ChoiceView, ControlChoices, ControlMenu, ControlValue, DocumentAction,
    DocumentId, DocumentLocation, Draft, EditorSession, EditorState, EditorView, Effect,
    EntryHandle, EntryMutation, Error, FacetCounts, FieldHandle, FileKind, FileNotice, FileStatus,
    FilterInput, LibraryBinding, LibraryData, LibraryOutcome, LibraryStatus, LibraryView,
    ModulationKind, MonitorView, MotionPreference, OutputState, OutputView, PathPurpose,
    PlaybackControl, PlaybackPhase, PromptAnswer, PromptId, PromptState, QueryFacet, Recipe,
    RecipeSnapshot, Refusal, RowKey, SampleRate, SaveFeedback, SelectionBinding, TaskId,
    TransportView, View, Workspace, WorkspaceActions, audio, controls, editor, file, file_name,
    library,
};
use crate::core::rhythm::{Band, Completeness, Method};
use crate::core::{provenance::ConditionRole, provenance::Population};
use crate::discovery::query::{Direction, GroupBy, RhythmState, Scope, SortBy};
use crate::document::origin::{LibraryKey, VersionToken};
use crate::services::library::{Library, Mutation, WriteOutcome};

use crate::discovery::{collection::CatalogQuery, query::RhythmQuery};
use crate::workspace::monitor::WaveformScale;

/// One requested workspace transition. Display labels are never command input.
#[derive(Debug, Clone)]
pub enum Command {
    /// Answer the currently open frontend prompt; stale answers change nothing.
    Answer {
        prompt: PromptId,
        answer: PromptAnswer,
    },
    /// Resolve a value retained from an app-issued control menu.
    ChooseControl { row: RowKey, value: ControlValue },
    /// Change the interaction surface.
    SetView(View),
    /// Optional teaching navigation and captured example creation.
    Learning(crate::learning::LearningCommand),
    /// Edit the current document or request its replacement.
    Editor(EditorCommand),
    /// Query or act on the current library listing.
    Library(LibraryCommand),
    /// Control the audio task.
    Audio(AudioCommand),
    /// Request file work.
    File(FileCommand),
    /// Change monitor/preview policy.
    Output(OutputCommand),
    /// Play device-check tones and record what the listener heard.
    DeviceCheck(DeviceCheckCommand),
    /// Request the shared close decision.
    Close,
}
/// Device-check input; answers stay in this session.
#[derive(Debug, Clone, Copy)]
pub enum DeviceCheckCommand {
    /// Select a test and play its tone while Device check is the current view.
    Play(crate::device_check::DeviceTest),
    /// Record what the listener reported for a test.
    Answer(
        crate::device_check::DeviceTest,
        crate::device_check::DeviceAnswer,
    ),
    /// Forget every answer.
    Clear,
}
/// Document input whose identities remain bound to the captured editor lifetime.
#[derive(Debug, Clone)]
pub enum EditorCommand {
    New,
    CopySelected,
    SetExportRate(SampleRate),
    Stage {
        field: FieldHandle,
        text: String,
    },
    Restore(FieldHandle),
    CommitField {
        field: FieldHandle,
        text: String,
    },
    Commit,
    Slide {
        field: FieldHandle,
        position: f64,
    },
    SetRouting {
        row: RowKey,
        route: ChannelRouting,
    },
    SetCarrier {
        row: RowKey,
        kind: CarrierKind,
    },
    SetModulation {
        row: RowKey,
        kind: Option<ModulationKind>,
    },
    AddConstruction {
        row: RowKey,
        kind: crate::document::construction::CreationKind,
    },
    SetPulseWidth {
        row: RowKey,
        modulation: bool,
        duty: bool,
    },
    SetPulseShape {
        row: RowKey,
        modulation: bool,
        shaped: bool,
    },
    AddComponent {
        row: RowKey,
        kind: CarrierKind,
    },
    AddPair {
        row: RowKey,
        kind: crate::core::PairKind,
    },
    RemoveComponent(RowKey),
    RestoreComponent(RowKey),
    ShowBasedOnSource,
}
/// Movement through unique visible entries, independent of display grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    Previous,
    Next,
    First,
    Last,
}
/// Library input; row actions carry only opaque app-issued handles.
#[derive(Debug, Clone)]
pub enum LibraryCommand {
    SetQuery(Box<CatalogQuery>),
    SetFacets(Vec<(QueryFacet, String)>),
    SetFilters(Box<FilterInput>),
    ClearFilters,
    ClearRhythmFilters,
    ResetSort,
    Search(String),
    SetRhythm(RhythmQuery),
    Select(EntryHandle),
    Navigate(Navigation),
    Refresh,
    Initialize,
    EditSelected,
    EditEntry(EntryHandle),
    AddSelected,
    AddEntry(EntryHandle),
    Mutate {
        entry: EntryHandle,
        mutation: EntryMutation,
    },
    CloseSelected,
    CloseEntry(EntryHandle),
    ShowSaved,
}
/// Audio admission is evaluated against the state used to start/stop work.
#[derive(Debug, Clone, Copy)]
pub enum AudioCommand {
    Play,
    /// Play the Library selection only while that surface remains current.
    PlaySelected,
    Toggle,
    Stop,
}
/// File requests capture their content before asynchronous work starts.
#[derive(Debug, Clone, Copy)]
pub enum FileCommand {
    Import,
    OpenRecording,
    /// Measure the opened recording with captured options.
    AnalyzeRecording(crate::recording::AnalysisOptions),
    /// Cancel the active measurement.
    CancelAnalysis,
    Save,
    SaveCopy,
    ExportRecipe,
    ExportWav,
    CancelExport,
}
/// Monitor settings do not redefine domain validation or audio admission.
#[derive(Debug, Clone, Copy)]
pub enum OutputCommand {
    SetMotion(MotionPreference),
    SetHold(bool),
    FollowMotion,
    SetWaveform { span_ms: u32, scale: WaveformScale },
    Preview,
}

impl Workspace {
    /// Query current command admission without allocating identities, emitting effects or
    /// retaining an admitted capture. Cached candidate/validation facts may be populated.
    /// Availability is advisory; execution and prompt continuation query current state again.
    ///
    /// # Errors
    /// Returns the same typed refusal used when executing this request now.
    pub fn admission(&mut self, command: &Command) -> Result<(), Refusal> {
        self.learning_command_admission(command)?;
        match command {
            Command::File(command) => self.file_admission(*command),
            Command::Editor(command) => self.editor_command_admission(command),
            Command::Library(command) => self.library_command_admission(command),
            Command::Output(OutputCommand::Preview) => self.preview_admission(),
            Command::ChooseControl { row, value } => {
                if self.control_choices().iter().any(|menu| {
                    menu.row == *row
                        && menu
                            .view
                            .options
                            .iter()
                            .any(|option| option.key == *value && option.enabled)
                }) {
                    Ok(())
                } else {
                    Err(Refusal::SelectionChanged)
                }
            }
            Command::DeviceCheck(DeviceCheckCommand::Play(_)) => {
                if self.view != View::DeviceCheck {
                    return Err(Refusal::NoSoundInView);
                }
                self.play_readiness()
            }
            Command::DeviceCheck(DeviceCheckCommand::Answer(test, answer)) => {
                if test.answers().contains(answer) {
                    Ok(())
                } else {
                    Err(Refusal::UnofferedAnswer(
                        crate::device_check::UnofferedAnswer {
                            test: *test,
                            answer: *answer,
                        },
                    ))
                }
            }
            Command::Audio(AudioCommand::Stop) => self.audio.stop_admission().map(|_| ()),
            Command::Audio(AudioCommand::Toggle) if self.audio.busy() => {
                self.audio.stop_admission().map(|_| ())
            }
            Command::Audio(AudioCommand::PlaySelected) if self.view != View::Library => Ok(()),
            Command::Audio(_) => self.play_readiness(),
            Command::Learning(crate::learning::LearningCommand::CreateExample(_)) => {
                if !self.learning_visible {
                    return Err(Refusal::SelectionChanged);
                }
                self.start_allowed()
            }
            Command::Answer { .. }
            | Command::SetView(_)
            | Command::Close
            | Command::Learning(_)
            | Command::Output(_)
            | Command::DeviceCheck(DeviceCheckCommand::Clear) => Ok(()),
        }
    }

    fn file_admission(&mut self, command: FileCommand) -> Result<(), Refusal> {
        match command {
            FileCommand::Import | FileCommand::OpenRecording => self.import_admission(),
            FileCommand::AnalyzeRecording(_) => {
                self.import_admission()?;
                self.recording
                    .as_ref()
                    .map(|_| ())
                    .ok_or(Refusal::NoSelection)
            }
            FileCommand::CancelExport => {
                if self.exporting() {
                    Ok(())
                } else {
                    Err(Refusal::NoExport)
                }
            }
            FileCommand::CancelAnalysis => {
                if self.analyzing_recording() {
                    Ok(())
                } else {
                    Err(Refusal::FileBusy)
                }
            }
            FileCommand::Save => self.save_readiness(),
            FileCommand::SaveCopy => self.save_copy_readiness(),
            FileCommand::ExportRecipe => self.export_recipe_readiness(),
            FileCommand::ExportWav => self.export_readiness(),
        }
    }

    fn editor_command_admission(&self, command: &EditorCommand) -> Result<(), Refusal> {
        match command {
            EditorCommand::New => self.start_allowed(),
            EditorCommand::CopySelected => self.copy_admission(),
            EditorCommand::ShowBasedOnSource | EditorCommand::SetExportRate(_) => Ok(()),
            EditorCommand::AddComponent { row, kind } => self.component_add_admission(*row, *kind),
            EditorCommand::AddPair { row, kind } => self.pair_add_admission(*row, *kind),
            EditorCommand::AddConstruction { row, kind } => {
                self.editing_allowed()?;
                self.editor.addition_admission(*row, kind.component())
            }
            EditorCommand::RestoreComponent(row) => self.restore_component_admission(*row),
            EditorCommand::Stage { field, .. }
            | EditorCommand::CommitField { field, .. }
            | EditorCommand::Restore(field)
            | EditorCommand::Slide { field, .. } => {
                self.editing_allowed()?;
                self.editor
                    .session()
                    .admit_field(*field)
                    .map_err(Refusal::Edit)
            }
            EditorCommand::SetRouting { row, .. }
            | EditorCommand::SetCarrier { row, .. }
            | EditorCommand::SetModulation { row, .. }
            | EditorCommand::SetPulseWidth { row, .. }
            | EditorCommand::SetPulseShape { row, .. }
            | EditorCommand::RemoveComponent(row) => {
                self.editing_allowed()?;
                self.editor
                    .session()
                    .admit_component(*row)
                    .map(|_| ())
                    .map_err(Refusal::Edit)
            }
            EditorCommand::Commit => self.editing_allowed(),
        }
    }

    fn library_command_admission(&self, command: &LibraryCommand) -> Result<(), Refusal> {
        match command {
            LibraryCommand::Initialize => self.library_job_admission(),
            LibraryCommand::EditSelected => {
                self.start_allowed()?;
                self.selected().map(|_| ()).ok_or(Refusal::NoSelection)
            }
            LibraryCommand::AddSelected => self.add_to_library_readiness(),
            LibraryCommand::Select(entry) | LibraryCommand::CloseEntry(entry) => {
                self.resolve_entry_handle(entry, false).map(|_| ())
            }
            LibraryCommand::EditEntry(entry) => {
                self.start_allowed()?;
                let (key, version, trashed) = self.resolve_entry_handle(entry, false)?;
                if self.library.selected_key() != Some(&key) {
                    return Err(Refusal::SelectionChanged);
                }
                if matches!(key, LibraryKey::External(_))
                    && self
                        .selected()
                        .is_some_and(|snapshot| snapshot.external_file().is_some())
                {
                    return Ok(());
                }
                self.library
                    .loaded()
                    .filter(|record| {
                        record.key == key
                            && Some(record.version_token) == version
                            && !trashed
                            && !self.library.requires_read(&key)
                    })
                    .map(|_| ())
                    .ok_or(Refusal::SelectionChanged)
            }
            LibraryCommand::AddEntry(entry) => {
                let (key, _, _) = self.resolve_entry_handle(entry, false)?;
                if self.library.selected_key() != Some(&key) {
                    return Err(Refusal::SelectionChanged);
                }
                self.add_to_library_readiness()
            }
            LibraryCommand::Mutate { entry, mutation } => {
                let purge = matches!(mutation, EntryMutation::RequestPurge);
                let (key, version, trashed) = self.resolve_entry_handle(entry, purge)?;
                crate::storage::library::require_user(&key)
                    .map_err(|error| Refusal::Document(Box::new(error.into())))?;
                if version.is_none() || (purge && !trashed) {
                    return Err(if purge {
                        Refusal::PurgeSelectionChanged
                    } else {
                        Refusal::SelectionChanged
                    });
                }
                if matches!(mutation, EntryMutation::Trash) {
                    self.trash_request_admission(&key)
                } else {
                    self.library_job_admission()
                }
            }
            LibraryCommand::SetQuery(_)
            | LibraryCommand::SetFacets(_)
            | LibraryCommand::SetFilters(_)
            | LibraryCommand::ClearFilters
            | LibraryCommand::ClearRhythmFilters
            | LibraryCommand::ResetSort
            | LibraryCommand::Search(_)
            | LibraryCommand::SetRhythm(_)
            | LibraryCommand::Navigate(_)
            | LibraryCommand::Refresh
            | LibraryCommand::CloseSelected
            | LibraryCommand::ShowSaved => Ok(()),
        }
    }

    /// Apply one typed user command through its owning admission handler.
    ///
    /// # Errors
    /// Returns the typed refusal while retaining its owner-specific status.
    pub fn apply(&mut self, command: Command) -> Result<(), Refusal> {
        self.learning_command_admission(&command)?;
        // Harmless callbacks for a surface that is no longer current perform no action.
        if matches!(&command, Command::DeviceCheck(DeviceCheckCommand::Play(_)))
            && self.view != View::DeviceCheck
        {
            return Ok(());
        }
        if matches!(&command, Command::Output(OutputCommand::Preview))
            && self.preview_admission().is_err()
        {
            return Ok(());
        }
        // This is a fresh policy query, never an availability-time permission token.
        // The owner handlers retain their action-state and resource-boundary checks.
        if let Err(refusal) = self.admission(&command) {
            match &command {
                Command::Audio(_) => self
                    .audio
                    .record_status(AudioStatus::Refused(refusal.clone())),
                Command::File(_)
                | Command::Library(_)
                | Command::Editor(EditorCommand::New | EditorCommand::CopySelected) => self
                    .files
                    .record_status(FileStatus::Refused(refusal.clone())),
                Command::Editor(_) => self.editor.finish_edit(&Err(refusal.clone())),
                _ => return Err(refusal),
            }
            self.changed();
            return Err(refusal);
        }
        match command {
            Command::Learning(command) => self.apply_learning(command),
            Command::Answer { prompt, answer } => {
                if self.respond_prompt(prompt, answer) {
                    self.poll();
                }
                Ok(())
            }
            Command::ChooseControl { row, value } => self.choose_control(row, value),
            Command::SetView(view) => {
                self.set_view(view);
                Ok(())
            }
            Command::Close => self.close(),
            Command::DeviceCheck(command) => self.apply_device_check(command),
            Command::Editor(command) => self.apply_editor(command),
            Command::Library(command) => self.apply_library(command),
            Command::Audio(command) => match self.audio.apply(command) {
                Ok(audio::AudioTransition::Play) => self.play(),
                Ok(audio::AudioTransition::PlaySelected) => {
                    if self.view == View::Library {
                        self.play()
                    } else {
                        Ok(())
                    }
                }
                result => {
                    self.changed();
                    result.map(|_| ())
                }
            },
            Command::File(command) => {
                if matches!(command, FileCommand::Save | FileCommand::SaveCopy)
                    && self.view != View::Workbench
                {
                    return Ok(());
                }
                match self.files.apply(command)? {
                    Some(file::FileRequest::Import) => self.import(),
                    Some(file::FileRequest::OpenRecording) => self.open_recording(),
                    Some(file::FileRequest::AnalyzeRecording(options)) => {
                        self.analyze_recording(options)
                    }
                    Some(file::FileRequest::Save) => self.save(),
                    Some(file::FileRequest::SaveCopy) => self.save_independent_copy(),
                    Some(file::FileRequest::ExportRecipe) => self.export_recipe(),
                    Some(file::FileRequest::ExportWav) => self.export(),
                    None => Ok(()),
                }
            }
            Command::Output(command) => {
                match command {
                    OutputCommand::SetMotion(preference) => self.set_motion_preference(preference),
                    OutputCommand::SetHold(held) => self.set_hold(held),
                    OutputCommand::FollowMotion => self.follow_motion(),
                    OutputCommand::SetWaveform { span_ms, scale } => {
                        self.set_waveform(span_ms, scale);
                    }
                    OutputCommand::Preview => self.preview_current(),
                }
                Ok(())
            }
        }
    }

    /// Whether a file task occupies its slot.
    #[must_use]
    pub const fn file_busy(&self) -> bool {
        self.files.busy()
    }
    /// Whether the running file task is an export.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.files.exporting()
    }
    /// Choose a recipe file to open. Opening may run beside audio.
    ///
    /// # Errors
    /// Returns the admission refusal.
    pub(super) fn import(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let id = self.prompt(PromptState::Import);
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::OpenRecipe,
            suggested_name: None,
        });
        self.changed();
        Ok(())
    }
    /// Choose a recording to identify. Opening may run beside audio.
    ///
    /// # Errors
    /// Returns the admission refusal.
    pub(super) fn open_recording(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let id = self.prompt(PromptState::OpenRecording);
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::OpenRecording,
            suggested_name: None,
        });
        self.changed();
        Ok(())
    }
    fn apply_device_check(&mut self, command: DeviceCheckCommand) -> Result<(), Refusal> {
        match command {
            DeviceCheckCommand::Play(test) => {
                if self.view != View::DeviceCheck {
                    return Ok(());
                }
                if self.device_check.select(test) {
                    self.changed();
                }
                self.play()
            }
            DeviceCheckCommand::Answer(test, answer) => {
                self.device_check.record(test, answer)?;
                self.changed();
                Ok(())
            }
            DeviceCheckCommand::Clear => {
                if self.device_check.clear() {
                    self.changed();
                }
                Ok(())
            }
        }
    }
    /// Device-check tests, answers and their interpretation.
    #[must_use]
    pub fn device_check_view(&self) -> crate::workspace::DeviceCheckView {
        let check = self.device_check.check();
        crate::workspace::DeviceCheckView {
            tests: crate::device_check::DeviceTest::ALL
                .into_iter()
                .map(|test| crate::workspace::DeviceTestView {
                    test,
                    answer: check.answer(test),
                    selected: test == self.device_check.selected(),
                })
                .collect(),
            assessment: check.assessment(),
            answered: check.started(),
        }
    }
    fn analyze_recording(
        &mut self,
        options: crate::recording::AnalysisOptions,
    ) -> Result<(), Refusal> {
        self.import_admission()?;
        let path = self
            .recording
            .as_ref()
            .ok_or(Refusal::NoSelection)?
            .path
            .clone();
        // Options are validated again by the worker against its captured input.
        self.start_file(FileKind::AnalyzeRecording {
            path,
            options,
            cancel: crate::job::Cancel::default(),
            progress: crate::job::Progress::default(),
        });
        self.changed();
        Ok(())
    }
    /// Last complete findings, retained through failed or cancelled re-analysis.
    pub fn recording_analysis(&self) -> Option<&crate::recording::AnalyzedRecording> {
        self.recording_analysis.as_ref()
    }
    /// Whether a recording measurement currently occupies the file slot.
    pub fn analyzing_recording(&self) -> bool {
        self.files
            .task()
            .is_some_and(|task| matches!(task.kind, FileKind::AnalyzeRecording { .. }))
    }
    /// Completed fraction once the decoder has established the selected window.
    pub fn recording_progress(&self) -> f64 {
        if let Some(super::FileTask {
            kind: FileKind::AnalyzeRecording { progress, .. },
            ..
        }) = self.files.task()
        {
            #[expect(
                clippy::cast_precision_loss,
                reason = "frame counts form a display-only progress ratio"
            )]
            let ratio = progress.frames() as f64 / progress.total().max(1) as f64;
            ratio.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
    /// The most recently opened recording, kept until another one opens.
    #[must_use]
    pub const fn recording(&self) -> Option<&crate::recording::RecordingFile> {
        self.recording.as_ref()
    }
    /// The captured recipe the current view's preview and Play use, if any.
    pub(super) fn shown(&self) -> Option<&RecipeSnapshot> {
        match self.view {
            View::Library => self.selected(),
            View::DeviceCheck => Some(self.device_check.tone()),
            View::Workbench | View::Recordings => None,
        }
    }
    /// Commit and capture the open document, then choose where to save it.
    /// The captured content is what is written, whatever is edited later.
    ///
    /// # Errors
    /// Returns the admission or readiness refusal.
    pub(super) fn save(&mut self) -> Result<(), Refusal> {
        let result = self.save_inner();
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused((refusal).clone()));
        }
        self.changed();
        result
    }
    pub(super) fn save_inner(&mut self) -> Result<(), Refusal> {
        let content = self.prepare_save()?;
        self.editor.commit()?;
        self.submit_live();
        self.start_managed_save(self.editor.session().document(), content)?;
        Ok(())
    }
    /// Export captured portable content without saving or forking the document.
    ///
    /// # Errors
    /// Refuses occupied slots or an unserializable draft; no playback is started.
    pub(super) fn export_recipe(&mut self) -> Result<(), Refusal> {
        let (text, title) = self.prepare_export_recipe()?;
        let prompt = self.prompt(PromptState::ExportRecipe(text));
        self.effects.push(Effect::ChoosePath {
            prompt,
            purpose: PathPurpose::ExportRecipe,
            suggested_name: Some(file_name(&title, "toml")),
        });
        self.changed();
        Ok(())
    }
    /// Validate the current source at the selected export rate, then choose
    /// where to write it. Export needs idle audio and an idle file slot.
    ///
    /// # Errors
    /// Returns the admission or validity refusal.
    pub(super) fn export(&mut self) -> Result<(), Refusal> {
        let result = self.export_inner();
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused((refusal).clone()));
        }
        self.changed();
        result
    }
    pub(super) fn export_inner(&mut self) -> Result<(), Refusal> {
        let prepared = self.prepare_export()?;
        if self.view == View::Workbench {
            self.editor.commit()?;
            self.submit_live();
        }
        let (sound, name) = prepared.capture();
        let id = self.prompt(PromptState::Export(sound));
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::ExportWav,
            suggested_name: Some(name),
        });
        Ok(())
    }

    pub(super) fn copy_admission(&self) -> Result<(), Refusal> {
        self.start_allowed()?;
        self.library.copy_admission()
    }
    pub(super) fn copy_content_admission(snapshot: &RecipeSnapshot) -> Result<(), Refusal> {
        library::LibraryState::copy_recipe_admission(snapshot)
    }
    pub(super) fn import_admission(&self) -> Result<(), Refusal> {
        self.prompts.admit_start(self.pending.is_some())?;
        self.files.idle_admission()
    }
    fn save_readiness(&mut self) -> Result<(), Refusal> {
        self.library.record_destination_admission()?;
        self.import_admission()?;
        self.editor.save_readiness()
    }
    fn save_copy_readiness(&mut self) -> Result<(), Refusal> {
        self.library.record_destination_admission()?;
        self.import_admission()?;
        // The candidate preview is cached by its semantic input revision.
        self.editor.save_copy_readiness()
    }
    fn export_readiness(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        self.audio.idle_admission()?;
        match self.view {
            View::Library => self.library.export_readiness(self.editor.export_rate()),
            View::Workbench => self.editor.export_readiness(),
            View::DeviceCheck | View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    pub(super) fn prepare_save(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.library.record_destination_admission()?;
        self.import_admission()?;
        self.editor.prepare_save()
    }
    pub(super) fn prepare_save_copy(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.library.record_destination_admission()?;
        self.import_admission()?;
        self.editor.save_copy_readiness()?;
        let recipe = self.editor.prepare_recipe()?;
        Ok(recipe)
    }

    pub(super) fn prepare_export(&mut self) -> Result<file::PreparedExport, Refusal> {
        self.import_admission()?;
        self.audio.idle_admission()?;
        match self.view {
            View::Library => self.library.prepare_export(self.editor.export_rate()),
            View::Workbench => self.editor.prepare_export(),
            View::DeviceCheck | View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    fn export_recipe_readiness(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        match self.view {
            View::Library => self.library.recipe_readiness(),
            View::Workbench => self.editor.recipe_readiness(),
            View::DeviceCheck | View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    pub(super) fn prepare_export_recipe(&mut self) -> Result<(Arc<str>, String), Refusal> {
        self.import_admission()?;
        match self.view {
            View::Library => self.library.prepare_recipe_text(),
            View::Workbench => self.editor.prepare_recipe_text(),
            View::DeviceCheck | View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    pub(super) fn prepare_audio(&mut self) -> Result<AudioRequest, Refusal> {
        self.audio.play_admission()?;
        self.prompts.admit_start(self.pending.is_some())?;
        if self.files.exporting() {
            return Err(Refusal::Exporting);
        }
        match self.view {
            View::Library => self.library.prepare_audio(),
            View::Workbench => self.editor.prepare_live(),
            View::DeviceCheck => Ok(self.device_check.prepare_audio()),
            View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    pub(super) fn preview_admission(&self) -> Result<(), Refusal> {
        OutputState::preview_admission(&self.audio)
    }
    pub(super) fn component_add_admission(
        &self,
        row: RowKey,
        kind: CarrierKind,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        self.editor
            .addition_admission(row, controls::default_component(kind))
    }
    pub(super) fn pair_add_admission(
        &self,
        row: RowKey,
        kind: crate::core::PairKind,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        self.editor
            .addition_admission(row, controls::default_pair(kind))
    }
    pub(super) fn restore_component_admission(&self, row: RowKey) -> Result<(), Refusal> {
        self.editing_allowed()?;
        self.editor.restore_admission(row)
    }

    pub(super) fn library_job_admission(&self) -> Result<(), Refusal> {
        self.import_admission()?;
        self.library.save_destination_admission()
    }
    fn add_to_library_readiness(&self) -> Result<(), Refusal> {
        self.library.add_readiness()?;
        self.library_job_admission()?;
        self.library.record_destination_admission()
    }
    pub(super) fn prepare_add_to_library(&self) -> Result<Arc<Recipe>, Refusal> {
        let recipe = self.library.prepare_add()?;
        self.library_job_admission()?;
        self.library.record_destination_admission()?;
        Ok(recipe)
    }
    pub(super) fn trash_request_admission(
        &self,
        key: &crate::document::origin::LibraryKey,
    ) -> Result<(), Refusal> {
        crate::storage::library::require_user(key)
            .map_err(|error| Refusal::Document(Box::new(error.into())))?;
        if self.editor.dirty_managed(key) {
            self.start_allowed()
        } else {
            self.library_job_admission()
        }
    }

    /// The capabilities in effect.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.audio.capabilities()
    }
    /// Whether an audio task occupies its slot.
    #[must_use]
    pub const fn audio_busy(&self) -> bool {
        self.audio.busy()
    }
    /// The bound live session's actual device rate, if any.
    #[must_use]
    pub fn live_rate(&self) -> Option<SampleRate> {
        self.audio.live_rate(self.editor.session().document())
    }

    /// Play what the current view shows: the selected recipe to its end, or
    /// the open document live. Playing replaces current audio after it
    /// stops. Preparation is requested from fully parsed finite values; the
    /// definitive check happens at the device's actual rate before the
    /// stream starts, independently of the export rate.
    ///
    /// # Errors
    /// Returns the admission or input refusal.
    pub(super) fn play(&mut self) -> Result<(), Refusal> {
        let result = self.play_inner();
        self.audio.record_status(match &result {
            Ok(()) => AudioStatus::Idle,
            Err(refusal) => AudioStatus::Refused(refusal.clone()),
        });
        self.changed();
        result
    }

    pub(super) fn play_inner(&mut self) -> Result<(), Refusal> {
        let request = self.audio_request()?;
        if self.audio.busy() {
            self.schedule(Action::StartAudio(request));
        } else {
            self.start_audio(request);
        }
        Ok(())
    }
    pub(super) fn audio_request(&mut self) -> Result<AudioRequest, Refusal> {
        let request = self.prepare_audio()?;
        if matches!(request, AudioRequest::Live { .. }) {
            self.editor.commit()?;
        }
        Ok(request)
    }
    /// Whether Play has a complete candidate and admission, before device discovery.
    pub(crate) fn play_readiness(&mut self) -> Result<(), Refusal> {
        self.audio.play_admission()?;
        self.prompts.admit_start(self.pending.is_some())?;
        if self.files.exporting() {
            return Err(Refusal::Exporting);
        }
        match self.view {
            View::Library => self.library.audio_readiness(),
            View::Workbench => self.editor.live_readiness(),
            View::DeviceCheck => Ok(()),
            View::Recordings => Err(Refusal::NoSoundInView),
        }
    }
    /// Stop control of the current audio session, for a native control that
    /// must stop exactly that session.
    #[must_use]
    pub fn audio_control(&self) -> Option<PlaybackControl> {
        self.audio.task().map(|task| task.control.clone())
    }
    /// Phase of the current audio session, if one is bound.
    #[must_use]
    pub fn audio_phase(&self) -> Option<PlaybackPhase> {
        self.audio.task().map(|t| t.control.phase())
    }

    /// The open document's editor.
    #[must_use]
    pub const fn editor(&self) -> &EditorSession {
        self.editor.session()
    }
    /// The selected inspection and export rate; device rates are separate.
    #[must_use]
    pub const fn export_rate(&self) -> SampleRate {
        self.editor.export_rate()
    }

    pub(super) fn editing_allowed(&self) -> Result<(), Refusal> {
        if self.audio.rejected_live(self.editor.session().document()) {
            return Err(Refusal::AudioPlaying);
        }
        self.prompts.admit_start(self.pending.is_some())?;
        if self.files.blocks_edit() {
            return Err(Refusal::FileBusy);
        }
        Ok(())
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

    /// The live session's actual rate, when a live task is bound to the open
    /// document; otherwise the selected export rate. Field diagnostics use it.
    #[must_use]
    pub fn diagnostic_rate(&self) -> SampleRate {
        self.live_rate().unwrap_or(self.editor.export_rate())
    }
    /// Show the recipe the open document was copied from, by its retained
    /// snapshot rather than by its path.
    pub(super) fn show_based_on_source(&mut self) {
        if let Some(source) = self.editor.session().draft().based_on().cloned() {
            self.reveal_retained_source(source);
        }
    }

    /// Current options for one library facet. Keys retain meaning across projections.
    #[must_use]
    pub fn library_choices(&self, facet: QueryFacet) -> ChoiceView {
        let q = self.catalog_query();
        let mut options = Vec::new();
        let mut option = |key, label, selected| {
            options.push(ChoiceOption {
                key,
                label,
                selected,
                enabled: true,
            });
        };
        macro_rules! values {
            ($kind:ty, $selected:expr) => {
                for value in <$kind>::ALL {
                    option(value.key(), value.label(), ($selected)(value));
                }
            };
        }
        match facet {
            QueryFacet::Scope => values!(Scope, |v| q.scope == v),
            QueryFacet::Sort => values!(SortBy, |v| q.sort == v),
            QueryFacet::Direction => {
                option(
                    "automatic",
                    crate::vocabulary::AUTOMATIC_DIRECTION.label,
                    q.direction.is_none(),
                );
                values!(Direction, |v| q.direction == Some(v));
            }
            QueryFacet::Group => values!(GroupBy, |v| q.group_by == v),
            QueryFacet::Band => {
                option(
                    "any",
                    crate::vocabulary::ALL_SOUNDS.label,
                    q.rhythm.bands.is_empty() && q.rhythm.state.is_none(),
                );
                values!(Band, |v| q.rhythm.bands.contains(&v));
                values!(RhythmState, |v| q.rhythm.state == Some(v));
            }
            QueryFacet::Method => {
                option(
                    "any",
                    crate::vocabulary::ALL_METHODS.label,
                    q.rhythm.methods.is_empty(),
                );
                values!(Method, |v| q.rhythm.methods.contains(&v));
            }
            QueryFacet::Population => {
                option(
                    "any",
                    crate::vocabulary::ANY_POPULATION.label,
                    q.populations.is_empty(),
                );
                values!(Population, |v| q.populations.contains(&v));
            }
            QueryFacet::Role => {
                option(
                    "any",
                    crate::vocabulary::ANY_CONDITION_ROLE.label,
                    q.roles.is_empty(),
                );
                values!(ConditionRole, |v| q.roles.contains(&v));
            }
            QueryFacet::Analysis => {
                option(
                    "any",
                    crate::vocabulary::ANY_ANALYSIS_COVERAGE.label,
                    q.analysis.is_none(),
                );
                values!(Completeness, |v| q.analysis == Some(v));
            }
        }
        ChoiceView { options }
    }
    pub(super) fn edit_facets(&mut self, facets: Vec<(QueryFacet, String)>) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        self.assign_facets(&mut query, facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }
    fn assign_facets(
        &self,
        query: &mut CatalogQuery,
        facets: Vec<(QueryFacet, String)>,
    ) -> Result<(), Refusal> {
        for (facet, key) in facets {
            // Accept only actual offered keys, including the explicit absence key.
            if !self
                .library_choices(facet)
                .options
                .iter()
                .any(|option| option.key == key)
            {
                return Err(Refusal::SelectionChanged);
            }
            match facet {
                QueryFacet::Scope => {
                    query.scope = key.parse().map_err(|_| Refusal::SelectionChanged)?;
                }
                QueryFacet::Sort => {
                    query.sort = key.parse().map_err(|_| Refusal::SelectionChanged)?;
                }
                QueryFacet::Direction => query.direction = key.parse().ok(),
                QueryFacet::Group => {
                    query.group_by = key.parse().map_err(|_| Refusal::SelectionChanged)?;
                }
                QueryFacet::Band => {
                    query.rhythm.bands = key.parse::<Band>().into_iter().collect();
                    query.rhythm.state = key.parse().ok();
                }
                QueryFacet::Method => query.rhythm.methods = key.parse().into_iter().collect(),
                QueryFacet::Population => query.populations = key.parse().into_iter().collect(),
                QueryFacet::Role => query.roles = key.parse().into_iter().collect(),
                QueryFacet::Analysis => query.analysis = key.parse().ok(),
            }
        }
        Ok(())
    }
    pub(super) fn edit_filters(&mut self, input: FilterInput) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        let ranges = (|| {
            use crate::discovery::query::CalendarField;
            let [created, modified, added] = input.dates;
            Ok::<_, crate::discovery::query::QueryError>([
                crate::discovery::calendar::local_range(
                    CalendarField::Created,
                    &created[0],
                    &created[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Modified,
                    &modified[0],
                    &modified[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Added,
                    &added[0],
                    &added[1],
                )?,
            ])
        })();
        let [created, modified, added] = ranges.map_err(|error| {
            self.library_filter_error(error.clone());
            Refusal::Document(Box::new(error.into()))
        })?;
        query.created = created;
        query.modified = modified;
        query.added = added;
        query.studies = if input.study.trim().is_empty() {
            Vec::new()
        } else {
            vec![input.study]
        };
        self.assign_facets(&mut query, input.facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }

    fn pulse_control_menus(
        &self,
        row: RowKey,
        pulse: crate::core::PulseSpec,
        modulation: bool,
    ) -> [ControlChoices; 2] {
        let duty = matches!(pulse.width, crate::core::PulseWidth::DutyCycle { .. });
        let shaped = matches!(pulse.shape, crate::core::PulseShape::RaisedCosine { .. });
        [
            self.control_menu(
                if modulation {
                    ControlMenu::ModulationWidth
                } else {
                    ControlMenu::SourceWidth
                },
                row,
                [false, true]
                    .map(|duty| (ControlValue::PulseWidth { modulation, duty }, ""))
                    .to_vec(),
                Some(ControlValue::PulseWidth { modulation, duty }),
            ),
            self.control_menu(
                if modulation {
                    ControlMenu::ModulationShape
                } else {
                    ControlMenu::SourceShape
                },
                row,
                [false, true]
                    .map(|shaped| (ControlValue::PulseShape { modulation, shaped }, ""))
                    .to_vec(),
                Some(ControlValue::PulseShape { modulation, shaped }),
            ),
        ]
    }

    /// Every menu, including its current selection and typed command payloads.
    #[must_use]
    pub fn control_choices(&self) -> Vec<ControlChoices> {
        let document = self.editor.session().document_key();
        let mut menus = Vec::new();
        for component in self.editor.session().draft().components() {
            let row = self
                .editor
                .session()
                .component_key(component.id)
                .unwrap_or_else(|| unreachable!("component"));
            menus.push(
                self.control_menu(
                    ControlMenu::ChannelRouting,
                    row,
                    ChannelRouting::ALL
                        .into_iter()
                        .map(|v| (ControlValue::ChannelRouting(v), v.label()))
                        .collect(),
                    component.form.route().map(ControlValue::ChannelRouting),
                ),
            );
            menus.push(
                self.control_menu(
                    ControlMenu::Carrier,
                    row,
                    CarrierKind::ALL
                        .into_iter()
                        .map(|v| (ControlValue::Carrier(v), v.label()))
                        .collect(),
                    component.form.primitive_kind().map(ControlValue::Carrier),
                ),
            );
            menus.push(
                self.control_menu(
                    ControlMenu::Modulation,
                    row,
                    controls::modulation_choices()
                        .into_iter()
                        .map(|v| {
                            (
                                ControlValue::Modulation(v),
                                v.map_or(
                                    crate::core::vocabulary::NO_MODULATION.label,
                                    ModulationKind::label,
                                ),
                            )
                        })
                        .collect(),
                    Some(ControlValue::Modulation(component.form.modulation_kind())),
                ),
            );
            let source_pulse = match component.form.source {
                crate::core::SoundSource::PulseTrain { pulse, .. } => Some(pulse),
                _ => None,
            };
            let modulation_pulse = match component.form.modulation {
                Some(crate::core::Modulation::Pulsed { pulse }) => Some(pulse),
                _ => None,
            };
            for (modulation, pulse) in [(false, source_pulse), (true, modulation_pulse)] {
                if let Some(pulse) = pulse {
                    menus.extend(self.pulse_control_menus(row, pulse, modulation));
                }
            }
        }
        menus.extend(self.document_control_menus(document));
        menus
    }

    fn document_control_menus(&self, document: RowKey) -> [ControlChoices; 4] {
        [
            self.control_menu(
                ControlMenu::AddComponent,
                document,
                crate::document::construction::CreationKind::ALL
                    .into_iter()
                    .map(|kind| (ControlValue::Creation(kind), kind.term().label))
                    .collect(),
                None,
            ),
            self.control_menu(
                ControlMenu::MonitorSpan,
                document,
                controls::MONITOR_SPANS_MS
                    .into_iter()
                    .map(|v| (ControlValue::MonitorSpan(v), ""))
                    .collect(),
                Some(ControlValue::MonitorSpan(self.audio.output().span_ms())),
            ),
            self.control_menu(
                ControlMenu::WaveformScale,
                document,
                crate::workspace::monitor::WaveformScale::ALL
                    .into_iter()
                    .map(|v| (ControlValue::WaveformScale(v), ""))
                    .collect(),
                Some(ControlValue::WaveformScale(self.audio.output().scale())),
            ),
            self.control_menu(
                ControlMenu::ExportRate,
                document,
                controls::EXPORT_RATES
                    .into_iter()
                    .map(|v| (ControlValue::ExportRate(v), ""))
                    .collect(),
                Some(ControlValue::ExportRate(self.editor.export_rate())),
            ),
        ]
    }

    fn control_menu(
        &self,
        menu: ControlMenu,
        row: RowKey,
        values: Vec<(ControlValue, &'static str)>,
        selected: Option<ControlValue>,
    ) -> ControlChoices {
        ControlChoices {
            menu,
            row,
            view: ChoiceView {
                options: values
                    .into_iter()
                    .map(|(key, label)| ChoiceOption {
                        key,
                        label,
                        selected: selected == Some(key),
                        enabled: match key {
                            ControlValue::Creation(kind) => {
                                self.editing_allowed().is_ok()
                                    && self
                                        .editor
                                        .addition_admission(row, kind.component())
                                        .is_ok()
                            }
                            ControlValue::MonitorSpan(_) | ControlValue::WaveformScale(_) => {
                                !self.audio.output().held()
                            }
                            _ => true,
                        },
                    })
                    .collect(),
            },
        }
    }
    pub(super) fn choose_control(
        &mut self,
        row: RowKey,
        value: ControlValue,
    ) -> Result<(), Refusal> {
        if !self.control_choices().iter().any(|menu| {
            menu.row == row
                && menu
                    .view
                    .options
                    .iter()
                    .any(|o| o.key == value && o.enabled)
        }) {
            return Err(Refusal::SelectionChanged);
        }
        let command = match value {
            ControlValue::ChannelRouting(route) => {
                Command::Editor(EditorCommand::SetRouting { row, route })
            }
            ControlValue::Carrier(kind) => Command::Editor(EditorCommand::SetCarrier { row, kind }),
            ControlValue::Modulation(kind) => {
                Command::Editor(EditorCommand::SetModulation { row, kind })
            }
            ControlValue::Creation(kind) => {
                Command::Editor(EditorCommand::AddConstruction { row, kind })
            }
            ControlValue::PulseWidth { modulation, duty } => {
                Command::Editor(EditorCommand::SetPulseWidth {
                    row,
                    modulation,
                    duty,
                })
            }
            ControlValue::PulseShape { modulation, shaped } => {
                Command::Editor(EditorCommand::SetPulseShape {
                    row,
                    modulation,
                    shaped,
                })
            }
            ControlValue::ExportRate(rate) => Command::Editor(EditorCommand::SetExportRate(rate)),
            ControlValue::MonitorSpan(span_ms) => Command::Output(OutputCommand::SetWaveform {
                span_ms,
                scale: self.audio.output().scale(),
            }),
            ControlValue::WaveformScale(scale) => Command::Output(OutputCommand::SetWaveform {
                span_ms: self.audio.output().span_ms(),
                scale,
            }),
        };
        self.apply(command)
    }

    /// Record the platform's reduced-motion preference.
    pub(super) fn set_motion_preference(&mut self, preference: MotionPreference) {
        if self.audio.set_motion(preference) {
            self.changed();
        }
    }
    /// Hold or release the displayed capture, overriding the platform
    /// preference until [`Workspace::follow_motion`].
    pub(super) fn set_hold(&mut self, held: bool) {
        self.audio.set_override(Some(held));
        self.changed();
    }
    /// Follow the platform's motion preference again.
    pub(super) fn follow_motion(&mut self) {
        self.audio.set_override(None);
        self.changed();
    }
    /// Choose the waveform span in milliseconds and its display scale.
    pub(super) fn set_waveform(&mut self, span_ms: u32, scale: WaveformScale) {
        if self.audio.set_waveform(span_ms, scale) {
            self.changed();
        }
    }
    /// Stop showing the last session's output and preview the current source.
    pub(super) fn preview_current(&mut self) {
        if self.preview_admission().is_ok() {
            self.audio.unbind_output();
            self.changed();
        }
    }
    /// The timing model of what the current view explains: the open
    /// document's accepted live revision, its committed content, or the
    /// selected recipe. `None` when that source is not valid.
    #[must_use]
    pub fn model_view(&self) -> Option<Arc<crate::workspace::monitor::ModelView>> {
        self.audio.output().model().cloned()
    }

    /// The open document's rows, values and diagnostics. Diagnostics use
    /// the bound live session's rate when there is one, otherwise the
    /// selected export rate; each is labelled.
    pub fn editor_view(&mut self) -> EditorView {
        let rate = self.diagnostic_rate();
        let live = self.live_rate().is_some();
        let unrelated_title = (self.view == View::Workbench)
            .then(|| self.audio.unrelated_title(self.editor.session().document()))
            .flatten();
        self.editor.view(rate, live, unrelated_title)
    }

    /// The library rows and the selected recipe at the export rate.
    #[must_use]
    pub fn library_view(&self) -> Arc<LibraryView> {
        self.library.view(self.editor.export_rate())
    }

    /// Action availability, from the same admission rules the commands use.
    pub fn action_view(&mut self) -> WorkspaceActions {
        let play = self.admission(&Command::Audio(AudioCommand::Play));
        let save = self.admission(&Command::File(FileCommand::Save));
        let save_copy = self.admission(&Command::File(FileCommand::SaveCopy));
        let export_recipe = self.admission(&Command::File(FileCommand::ExportRecipe));
        let export = self.admission(&Command::File(FileCommand::ExportWav));
        let copy = self.admission(&Command::Editor(EditorCommand::CopySelected));
        let operation_refusal = match self.view {
            View::Workbench => save
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Input(_) | Refusal::Document(_)))
                .cloned(),
            View::Library => copy
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Document(_)))
                .cloned(),
            View::DeviceCheck | View::Recordings => None,
        };
        let row = self.editor.session().document_key();
        let component_add = self.admission(&Command::Editor(EditorCommand::AddComponent {
            row,
            kind: CarrierKind::Tone,
        }));
        let pair_add = self.admission(&Command::Editor(EditorCommand::AddPair {
            row,
            kind: crate::core::PairKind::Binaural,
        }));
        let restore_component =
            self.admission(&Command::Editor(EditorCommand::RestoreComponent(row)));
        let playing = self.audio.busy();
        let exporting = self.exporting();
        let progress = self.export_progress();
        let mut view = WorkspaceActions {
            operations: Vec::new(),
            import: self.admission(&Command::File(FileCommand::Import)),
            library: self.admission(&Command::Library(LibraryCommand::Initialize)),
            add_to_library: self.admission(&Command::Library(LibraryCommand::AddSelected)),
            trash: self
                .selected_entry_handle()
                .ok_or(Refusal::NoSelection)
                .and_then(|entry| {
                    self.admission(&Command::Library(LibraryCommand::Mutate {
                        entry,
                        mutation: EntryMutation::Trash,
                    }))
                }),
            start: self.admission(&Command::Editor(EditorCommand::New)),
            preview: self.admission(&Command::Output(OutputCommand::Preview)),
            save_copy,
            export_recipe,
            editing: self.admission(&Command::Editor(EditorCommand::Commit)),
            restore_component,
            playing,
            copy_reason: copy.err(),
            operation_refusal,
            play_reason: play.as_ref().err().cloned(),
            save_reason: save.err(),
            export_reason: export.err(),
            pair_add_reason: pair_add.err(),
            add_component_reason: component_add.err(),
            removed_component: self
                .editor
                .session()
                .removed()
                .map(|(_, component)| *component),
            file_busy: self.file_busy(),
            exporting,
            transport_action: if playing {
                crate::vocabulary::Operation::Stop
            } else {
                match self.view {
                    View::Workbench => crate::vocabulary::Operation::PlayLive,
                    View::DeviceCheck => crate::vocabulary::Operation::PlayTestTone,
                    View::Library | View::Recordings => {
                        crate::vocabulary::Operation::PlaySelectedSound
                    }
                }
            },
            play_target: if self.learning_visible {
                String::new()
            } else {
                match self.view {
                    View::Workbench => self.editor.session().draft().title().to_owned(),
                    View::Library | View::DeviceCheck | View::Recordings => {
                        self.shown().map(RecipeSnapshot::title).unwrap_or_default()
                    }
                }
            },
            progress,
            file_status: self.files.status().clone(),
            edit_status: self.editor.status().clone(),
            audio_status: self.audio.status().clone(),
            file_activity: self
                .files
                .task()
                .map(|task| super::status::FileActivity::from_kind(&task.kind)),
        };
        view.operations = self.operation_views(&view);
        view
    }

    fn selected_mutation_admission(&mut self, mutation: EntryMutation) -> Result<(), Refusal> {
        let entry = self.selected_entry_handle().ok_or(Refusal::NoSelection)?;
        self.admission(&Command::Library(LibraryCommand::Mutate {
            entry,
            mutation,
        }))
    }
    fn operation_views(&mut self, view: &WorkspaceActions) -> Vec<ActionView> {
        crate::vocabulary::Operation::ALL
            .into_iter()
            .map(|operation| {
                use crate::vocabulary::Operation as O;
                let enabled = match operation {
                    O::Save => self.view == View::Workbench && view.can_save(),
                    O::SaveCopy => self.view == View::Workbench && view.can_save_copy(),
                    O::NewSound => view.can_new(),
                    O::EditSavedRecipe | O::EditExternalFile => {
                        self.selected_entry_handle().is_some_and(|entry| {
                            self.admission(&Command::Library(LibraryCommand::EditEntry(entry)))
                                .is_ok()
                        })
                    }
                    O::EditCopy => view.can_copy(),
                    O::OpenRecipe => self.admission(&Command::File(FileCommand::Import)).is_ok(),
                    O::OpenRecording => self
                        .admission(&Command::File(FileCommand::OpenRecording))
                        .is_ok(),
                    O::ExportRecipe => view.can_export_recipe(),
                    O::Export => view.can_export(),
                    O::Play | O::PlaySelectedSound | O::StopCurrentAndPlayRecipe => {
                        self.view == View::Library && view.can_play()
                    }
                    O::PlayLive => self.view == View::Workbench && view.can_play(),
                    O::PlayTestTone => self.view == View::DeviceCheck && view.can_play(),
                    O::ClearDeviceAnswers => {
                        self.device_check.check().started()
                            && self
                                .admission(&Command::DeviceCheck(DeviceCheckCommand::Clear))
                                .is_ok()
                    }
                    O::Stop => view.playback_busy(),
                    O::CancelExport => view.exporting,
                    O::AnalyzeRecording => self
                        .admission(&Command::File(FileCommand::AnalyzeRecording(
                            crate::recording::AnalysisOptions::default(),
                        )))
                        .is_ok(),
                    O::AddComponent | O::RemoveComponent => view.can_edit(),
                    O::UndoRemove => view.can_restore_component(),
                    O::LibraryAdd => view.can_add_to_library(),
                    O::LibraryTrash => view.can_request_trash(),
                    O::LibraryInit => self
                        .admission(&Command::Library(LibraryCommand::Initialize))
                        .is_ok(),
                    O::LibraryRename => self
                        .selected_mutation_admission(EntryMutation::Rename {
                            title: self
                                .selected()
                                .map(RecipeSnapshot::title)
                                .unwrap_or_default(),
                        })
                        .is_ok(),
                    O::LibraryRestore => self
                        .selected_mutation_admission(EntryMutation::Restore)
                        .is_ok(),
                    O::LibraryPurge => self
                        .selected_mutation_admission(EntryMutation::RequestPurge)
                        .is_ok(),
                    O::CloseOpenFile => {
                        !self.learning_visible
                            && self
                                .library
                                .selected_key()
                                .is_some_and(|key| matches!(key, LibraryKey::External(_)))
                    }
                    O::ShowSaved => !self.learning_visible && self.library.has_saved_entry(),
                    O::LibraryRefresh | O::LibraryRead => !self.learning_visible,
                    O::DeviceCheck => true,
                    O::List
                    | O::ListTrash
                    | O::Describe
                    | O::Inspect
                    | O::Recipe
                    | O::LibraryPath
                    | O::LibraryCheck
                    | O::LibraryUpdate => false,
                };
                ActionView { operation, enabled }
            })
            .collect()
    }
    /// The running export's completed fraction, from 0 to 1; 0 otherwise.
    #[must_use]
    pub fn export_progress(&self) -> f64 {
        match self.files.task() {
            Some(super::FileTask {
                kind: FileKind::Export {
                    frames, progress, ..
                },
                ..
            }) if *frames > 0 => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Nonnegative frame counts form a display-only ratio"
                )]
                let ratio = progress.frames() as f64 / *frames as f64;
                ratio.clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// The displayed capture.
    #[must_use]
    pub fn output_view(&self) -> OutputView {
        self.audio.output().view()
    }

    /// The transport and monitor text for the bound session or preview.
    #[must_use]
    pub fn transport_view(&self) -> TransportView {
        use super::monitor::{monitor_warning, output_monitor_state};
        let state = &self.audio.output();
        let reader = state.reader();
        let output = state.view();
        let monitor = reader.map(|reader| MonitorView {
            status: reader.status(),
            descriptor: reader.descriptor().clone(),
            accepted: reader.accepted().1,
            age: reader.age(),
            warning: monitor_warning(reader, state.span_ms()),
        });
        TransportView {
            state: output_monitor_state(
                reader,
                output.capture.as_deref(),
                output.held,
                self.audio.busy(),
            ),
            output,
            monitor,
            export_rate: self.editor.export_rate(),
            task_title: self.audio.task().map(|task| task.title.clone()),
            failed_title: state.failed_title().map(str::to_owned),
            motion: state.motion(),
            hold_override: state.hold_overridden(),
            model: state.model().cloned(),
        }
    }

    /// The selected inspection and export rate's export choices.
    #[must_use]
    pub const fn export_rates() -> [SampleRate; 5] {
        controls::EXPORT_RATES
    }

    /// Issue the current visible row's opaque handle without parsing display data.
    #[must_use]
    pub fn entry_handle(&self, key: &LibraryKey) -> Option<EntryHandle> {
        self.library.handle_for_key(key)
    }
    /// Handle of the current indexed selection, if it belongs to this listing.
    #[must_use]
    pub fn selected_entry_handle(&self) -> Option<EntryHandle> {
        self.library.selected_entry_handle()
    }
    fn resolve_entry_handle(
        &self,
        handle: &EntryHandle,
        purge: bool,
    ) -> Result<(LibraryKey, Option<VersionToken>, bool), Refusal> {
        self.library.resolve_entry_handle(handle, purge)
    }

    /// Create a desktop workspace connected to the configured persistent root.
    /// Filesystem reads run on the existing file worker; initialization is explicit
    /// for overrides. A missing default root is created by the first publication.
    ///
    /// # Errors
    /// Returns identity or curated-catalog errors without falling back to file saves.
    pub fn with_library(
        catalog: Vec<RecipeSnapshot>,
        capabilities: Capabilities,
        store: Library,
    ) -> Result<Self, Error> {
        Self::with_binding(catalog, capabilities, LibraryBinding::from_library(store))
    }
    pub(super) fn reveal_retained_source(&mut self, snapshot: RecipeSnapshot) {
        match self.library.reveal_retained_source(snapshot) {
            Ok(()) => {
                self.view = View::Library;
                self.effects.push(Effect::ShowView(View::Library));
                self.files
                    .record_status(FileStatus::Notice(FileNotice::RetainedSource));
            }
            Err(error) => self.files.record_status(FileStatus::Failed(error)),
        }
        self.changed();
    }
    /// Surface a native filter error while retaining the prior valid query.
    pub(super) fn library_filter_error(&mut self, error: crate::discovery::query::QueryError) {
        self.library.library_filter_error(error);
        self.changed();
    }
    /// Disjunctive unique counts from the shared query, with reference intervals.
    #[must_use]
    pub fn library_facets(&self) -> FacetCounts {
        self.library.library_facets()
    }

    /// Current persistent query; native filters and the CLI use the same contract.
    #[must_use]
    pub fn catalog_query(&self) -> &CatalogQuery {
        self.library.query()
    }
    /// Replace valid collection, date, evidence and sorting restrictions together.
    ///
    /// # Errors
    /// An invalid range leaves the previous query and selection intact.
    pub(super) fn set_catalog_query(
        &mut self,
        query: Box<CatalogQuery>,
    ) -> Result<(), crate::discovery::query::QueryError> {
        self.library.apply(library::LibraryInput::Query(query))?;
        self.load_library_selection();
        self.changed();
        Ok(())
    }
    /// Select only a key that still belongs to the rendered query. Old row
    /// positions and stale callbacks cannot address replacement entries.
    pub(super) fn select_entry(&mut self, entry: &EntryHandle) -> Result<(), Refusal> {
        let result = self
            .resolve_entry_handle(entry, false)
            .map(|(key, _, _)| key);
        match result {
            Ok(key) => {
                self.library.select_key(key);
                self.load_library_selection();
                self.changed();
                Ok(())
            }
            Err(refusal) => {
                self.files
                    .record_status(FileStatus::Refused(refusal.clone()));
                self.changed();
                Err(refusal)
            }
        }
    }
    /// Keyboard movement through unique visible sounds, skipping virtual headings.
    pub(super) fn navigate_library(&mut self, direction: i32) {
        self.library
            .apply(library::LibraryInput::Navigate(direction))
            .unwrap_or_else(|_| unreachable!("navigation has no validation failure"));
        self.load_library_selection();
        self.changed();
    }
    /// Request a rescan without dropping stale rows or the open draft on failure.
    pub(super) fn refresh_library(&mut self) {
        self.library
            .apply(library::LibraryInput::Refresh)
            .unwrap_or_else(|_| unreachable!("refresh has no validation failure"));
        self.continue_library();
        self.changed();
    }
    /// Explicitly initialize an override root. No scan initializes storage.
    pub(super) fn initialize_library(&mut self) -> Result<(), Refusal> {
        self.managed_job(LibraryRequest::Initialize)?;
        Ok(())
    }
    /// Open the captured User version in the editor, retaining its identity.
    /// Unsaved changes use the existing Save/Discard/Cancel decision.
    pub(super) fn edit_selected(&mut self) -> Result<(), Refusal> {
        if let Some(snapshot) = self
            .selected()
            .filter(|s| s.external_file().is_some())
            .cloned()
        {
            return self.request(DocumentAction::EditExternal(snapshot));
        }
        let record = {
            self.library
                .loaded()
                .filter(|r| Some(&r.key) == self.library.selected_key())
        }
        .cloned();
        if let Some(record) = record.filter(|r| !r.trashed) {
            self.request(DocumentAction::EditManaged(record))
        } else {
            self.copy_selected()
        }
    }
    /// Edit only the managed version bound to the activated control.
    pub(super) fn edit_entry(&mut self, entry: &EntryHandle) -> Result<(), Refusal> {
        let result = (|| {
            let (key, version, trashed) = self.resolve_entry_handle(entry, false)?;
            if self.library.selected_key() != Some(&key) {
                return Err(Refusal::SelectionChanged);
            }
            if matches!(key, LibraryKey::External(_))
                && self
                    .selected()
                    .is_some_and(|snapshot| snapshot.external_file().is_some())
            {
                return self.edit_selected();
            }
            let record = self
                .library
                .loaded()
                .filter(|record| {
                    record.key == key
                        && Some(record.version_token) == version
                        && !trashed
                        && !self.library.requires_read(&key)
                })
                .cloned()
                .ok_or(Refusal::SelectionChanged)?;
            self.request(DocumentAction::EditManaged(record))
        })();
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
            self.changed();
        }
        result
    }
    /// Save the current complete draft under a fresh independent User identity.
    ///
    /// # Errors
    /// Refuses incomplete drafts or occupied slots without replacing the document.
    pub(super) fn save_independent_copy(&mut self) -> Result<(), Refusal> {
        let result = self.save_independent_copy_inner();
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused((refusal).clone()));
        }
        self.changed();
        result
    }
    fn save_independent_copy_inner(&mut self) -> Result<(), Refusal> {
        let content = self.prepare_save_copy()?;
        let snapshot = RecipeSnapshot::new(
            crate::document::recipe::Origin::Draft(self.editor.session().draft().title().into()),
            (*content).clone(),
        );
        let draft =
            Draft::from_snapshot(&snapshot).map_err(|e| Refusal::Document(Box::new(e.into())))?;

        let editor =
            EditorSession::new(draft).map_err(|error| Refusal::Document(Box::new(error.into())))?;
        self.install_document(editor);
        self.save()
    }
    /// Add only the temporary snapshot named by the activated control.
    ///
    /// # Errors
    /// Refuses a changed/hidden selection rather than adding a replacement row.
    pub(super) fn add_entry(&mut self, entry: &EntryHandle) -> Result<(), Refusal> {
        let result = (|| {
            let (key, _, _) = self.resolve_entry_handle(entry, false)?;
            if self.library.selected_key() != Some(&key) {
                return Err(Refusal::SelectionChanged);
            }
            self.add_selected_to_library()
        })();
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
        }
        self.changed();
        result
    }
    /// Add a temporary opened document to User without modifying its source file.
    ///
    /// # Errors
    /// Refuses non-temporary selections or occupied task slots.
    pub(super) fn add_selected_to_library(&mut self) -> Result<(), Refusal> {
        let recipe = self.prepare_add_to_library()?;
        self.managed_job(LibraryRequest::Write {
            document: None,
            mutation: Mutation::Add(recipe),
        })?;
        Ok(())
    }
    pub(super) fn discard_then_trash(&mut self, key: LibraryKey, expect: VersionToken) {
        let operation = LibraryRequest::DiscardTrash {
            key,
            expect,
            document: self.editor.session().document(),
            draft: self.editor.session().draft_revision(),
            input: self.editor.session().input_revision(),
        };
        if let Err(refusal) = self.managed_job(operation) {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
        }
    }
    /// Execute one managed action using the key and token captured by the UI.
    ///
    /// # Errors
    /// Built-ins, stale gestures, unsafe dirty-document removal and invalid input
    /// are refused before filesystem access. Purge requires separate confirmation.
    pub(super) fn mutate_entry(
        &mut self,
        entry: &EntryHandle,
        mutation: EntryMutation,
    ) -> Result<(), Refusal> {
        let result = self.mutate_entry_inner(entry, mutation);
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
        }
        self.changed();
        result
    }
    fn mutate_entry_inner(
        &mut self,
        entry: &EntryHandle,
        mutation: EntryMutation,
    ) -> Result<(), Refusal> {
        let purge = matches!(mutation, EntryMutation::RequestPurge);
        let (key, version, trashed) = self.resolve_entry_handle(entry, purge)?;
        crate::storage::library::require_user(&key)
            .map_err(|error| Refusal::Document(Box::new(error.into())))?;
        let expect = version.ok_or(if purge {
            Refusal::PurgeSelectionChanged
        } else {
            Refusal::SelectionChanged
        })?;
        if matches!(mutation, EntryMutation::Trash) {
            self.trash_request_admission(&key)?;
            if self.editor.dirty_managed(&key) {
                return self.request(DocumentAction::TrashSaved { key, expect });
            }
        }
        let mutation = match mutation {
            EntryMutation::RequestPurge => {
                self.library_job_admission()?;
                if !trashed {
                    return Err(Refusal::PurgeSelectionChanged);
                }
                let title = self
                    .library
                    .entries()
                    .iter()
                    .find(|entry| entry.key == key)
                    .ok_or(Refusal::PurgeSelectionChanged)?
                    .title
                    .clone();
                let prompt = self.prompt(PromptState::Purge {
                    key: key.clone(),
                    expect,
                });
                self.effects.push(Effect::AskPurge {
                    prompt,
                    key: key.to_string(),
                    title,
                });
                return Ok(());
            }
            EntryMutation::Rename { title } => Mutation::Rename {
                key,
                expect: Some(expect),
                title,
            },
            EntryMutation::Trash => Mutation::Trash {
                key,
                expect: Some(expect),
            },
            EntryMutation::Restore => Mutation::Restore {
                key,
                expect: Some(expect),
            },
        };
        self.managed_job(LibraryRequest::Write {
            document: None,
            mutation,
        })?;
        Ok(())
    }
    pub(super) fn purge_confirmed(
        &mut self,
        key: LibraryKey,
        expect: VersionToken,
    ) -> Result<(), Refusal> {
        let result = self
            .managed_job(LibraryRequest::Write {
                document: None,
                mutation: Mutation::Purge {
                    key,
                    expect: Some(expect),
                    confirmed: true,
                },
            })
            .map(|_| ());
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
        }
        self.changed();
        result
    }
    /// Close only a still-selected temporary entry captured at activation.
    pub(super) fn close_entry(&mut self, entry: &EntryHandle) -> Result<(), Refusal> {
        let result = self
            .resolve_entry_handle(entry, false)
            .and_then(|(key, _, _)| {
                if self.library.selected_key() != Some(&key) {
                    return Err(Refusal::SelectionChanged);
                }
                self.close_open_file();
                Ok(())
            });
        if let Err(refusal) = &result {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
            self.changed();
        }
        result
    }
    /// Close the selected temporary entry without changing its external file.
    pub(super) fn close_open_file(&mut self) {
        if self.library.close_open_file() {
            self.changed();
        }
    }
    /// Reveal the last confirmed saved key explicitly, clearing filters only on
    /// this command; saving itself preserves the current query.
    pub(super) fn show_saved(&mut self) -> bool {
        match self.library.show_saved() {
            Ok(false) => false,
            Ok(true) => {
                self.load_library_selection();
                self.changed();
                true
            }
            Err(()) => {
                self.files
                    .record_status(FileStatus::Notice(FileNotice::MissingSavedEntry));
                self.changed();
                false
            }
        }
    }

    /// Selected indexed identity and observed token; absence is represented by None.
    #[must_use]
    pub fn library_binding(&self) -> Option<SelectionBinding<'_>> {
        self.library.library_binding()
    }
    fn managed_job(&mut self, operation: LibraryRequest) -> Result<TaskId, Refusal> {
        self.library_job_admission()?;
        let job = self.library.job(operation)?;
        let id = self.start_file(FileKind::Managed(job));
        self.changed();
        Ok(id)
    }
    pub(super) fn start_managed_save(
        &mut self,
        document: DocumentId,
        recipe: Arc<Recipe>,
    ) -> Result<TaskId, Refusal> {
        let p = &self.library;
        if let Some(key) = self.editor.location().detached() {
            return Err(Refusal::DetachedDraft(key.clone()));
        }
        if let Some(expected) = self.editor.location().external().cloned() {
            return self.managed_job(LibraryRequest::ExternalSave {
                document,
                expected,
                recipe,
            });
        }
        if self.editor.location().managed().is_none()
            && p.entries()
                .iter()
                .any(|e| e.key == LibraryKey::User(recipe.document().id))
        {
            return Err(Refusal::ExistingSavedIdentity(recipe.document().id));
        }
        let mutation = self.editor.location().managed().map_or_else(
            || Mutation::Create(recipe.clone()),
            |(key, expect)| Mutation::Update {
                key: key.clone(),
                expect: Some(expect),
                recipe: recipe.clone(),
            },
        );
        self.managed_job(LibraryRequest::Write {
            document: Some(document),
            mutation,
        })
    }
    pub(super) fn continue_library(&mut self) {
        if self.exiting {
            return;
        }
        if self.files.busy() || self.prompts.current.is_some() || self.pending.is_some() {
            return;
        }
        if self.library.take_refresh() {
            let _ = self.managed_job(LibraryRequest::Scan);
            return;
        }
        self.load_library_selection();
    }
    pub(super) fn load_library_selection(&mut self) {
        if self.files.busy() || self.prompts.current.is_some() || self.pending.is_some() {
            return;
        }
        if let Some(request) = self.library.load_request() {
            let _ = self.managed_job(request);
        }
    }

    pub(super) fn finish_managed(
        &mut self,
        job: &LibraryJob,
        result: Result<LibraryOutcome, Error>,
    ) -> (FileStatus, Option<bool>) {
        let document = job.document();
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                return (
                    FileStatus::Library(LibraryStatus::Failed {
                        error,
                        publishing: job.may_publish(),
                        context: job.context(),
                    }),
                    document.map(|_| false),
                );
            }
        };
        match result {
            LibraryOutcome::ExternalWritten(outcome) => {
                self.accept_external_outcome(document, *outcome)
            }

            LibraryOutcome::Inventory(inventory) => (
                FileStatus::Library(LibraryStatus::Inventory(
                    self.library.accept_inventory(inventory),
                )),
                None,
            ),
            LibraryOutcome::Loaded(record) => {
                let p = &mut self.library;
                p.accept_record(*record);
                (FileStatus::Library(LibraryStatus::Loaded), None)
            }
            LibraryOutcome::Written(outcome) => {
                let mut result = self.accept_managed_outcome(document, *outcome);
                if let Some((document, draft, input)) = job.discard_revision()
                    && self.editor.session().document() == document
                    && self.editor.session().draft_revision() == draft
                    && self.editor.session().input_revision() == input
                {
                    result = self
                        .finish_discard_reset(result, Draft::new().and_then(EditorSession::new));
                }
                result
            }
        }
    }
    pub(super) fn finish_discard_reset(
        &mut self,
        mut result: (FileStatus, Option<bool>),
        editor: Result<EditorSession, crate::document::error::EditorError>,
    ) -> (FileStatus, Option<bool>) {
        match editor {
            Ok(editor) => self.install_document(editor),
            Err(error) => {
                if let FileStatus::Library(LibraryStatus::Written { feedback, .. }) = &mut result.0
                {
                    feedback.push(SaveFeedback::EditorReset(error));
                }
                result.1 = Some(false);
            }
        }
        result
    }

    fn accept_external_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: crate::storage::external::ExternalOutcome,
    ) -> (FileStatus, Option<bool>) {
        let Some(document) = document else {
            return (
                FileStatus::Library(LibraryStatus::InvalidOwner),
                Some(false),
            );
        };
        let snapshot = outcome.snapshot();
        let same_document = self.editor.session().document() == document;
        if same_document && !self.editor.record_saved(document, &outcome.recipe) {
            return (
                FileStatus::Library(LibraryStatus::InvalidOwner),
                Some(false),
            );
        }
        if same_document {
            self.editor.locate(
                snapshot
                    .external_file()
                    .cloned()
                    .map_or(DocumentLocation::Unsaved, DocumentLocation::External),
            );
        }
        self.library
            .replace_external(outcome.previous.path(), snapshot);
        (
            FileStatus::Library(LibraryStatus::ExternalWritten {
                status: outcome.status,
                diagnostics: outcome.diagnostics,
            }),
            Some(true),
        )
    }

    pub(super) fn accept_managed_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: WriteOutcome,
    ) -> (FileStatus, Option<bool>) {
        let mut feedback = Vec::new();
        let key = outcome.key.clone();
        let title = outcome
            .record
            .as_ref()
            .map(|record| record.snapshot.title());

        if let Some(record) = outcome.record {
            if !self
                .editor
                .record_managed(document, &key, outcome.kind, &record, &mut feedback)
            {
                return (
                    FileStatus::Library(LibraryStatus::InvalidOwner),
                    Some(false),
                );
            }
            self.library.accept_record(record);
        } else {
            self.library.remove_entry(&key);
        }
        self.library.mark_saved(key.clone());
        (
            FileStatus::Library(LibraryStatus::Written {
                key,
                title,
                status: outcome.status,
                document: document.is_some(),
                diagnostics: outcome.diagnostics,
                feedback,
            }),
            document.map(|_| true),
        )
    }

    /// Loaded in-memory Built-in and temporary snapshots. Managed User summaries
    /// appear in `library_view`; only selected User content is loaded in full.
    #[must_use]
    pub fn catalog(&self) -> &[RecipeSnapshot] {
        self.library.catalog()
    }
    /// The selected recipe, when the search shows it. A search that hides
    /// every recipe leaves nothing selected.
    #[must_use]
    pub fn selected(&self) -> Option<&RecipeSnapshot> {
        self.library.selected()
    }
    #[must_use]
    pub fn query(&self) -> &str {
        &self.library.query().search
    }
    /// Search titles, IDs, research text and component meaning. When the
    /// selection stops matching, the first match is selected.
    pub(super) fn set_query(&mut self, query: &str) {
        self.library
            .apply(library::LibraryInput::Search(query.into()))
            .unwrap_or_else(|_| unreachable!("search has no validation failure"));
        self.load_library_selection();
        self.changed();
    }
    /// Change nominal facets without rebinding any playback or editor snapshot.
    pub(super) fn set_rhythm_query(&mut self, query: crate::discovery::query::RhythmQuery) {
        self.library
            .apply(library::LibraryInput::Rhythm(query))
            .unwrap_or_else(|_| unreachable!("rhythm choices are typed"));
        self.load_library_selection();
        self.changed();
    }
    /// Current shared nominal restrictions, retained when text search changes.
    #[must_use]
    pub const fn rhythm_query(&self) -> &crate::discovery::query::RhythmQuery {
        &self.library.query().rhythm
    }

    /// Borrow the exact currently indexed library and captured edit location.
    #[must_use]
    pub fn library_data(&self) -> LibraryData<'_> {
        self.library.library_data(self.editor.location())
    }

    fn apply_library(&mut self, command: LibraryCommand) -> Result<(), Refusal> {
        match command {
            LibraryCommand::SetFacets(facets) => self.edit_facets(facets),
            LibraryCommand::SetFilters(input) => self.edit_filters(*input),
            LibraryCommand::ClearFilters => {
                let query = self.catalog_query().cleared_filters();
                self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))?;
                self.effects.push(Effect::ResetLibraryInputs);
                Ok(())
            }
            LibraryCommand::ClearRhythmFilters => {
                let query = self.catalog_query().cleared_rhythm();
                self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
            }
            LibraryCommand::ResetSort => {
                let query = self.catalog_query().reset_sort();
                self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
            }
            LibraryCommand::SetQuery(query) => self.set_catalog_query(query).map_err(|error| {
                self.library_filter_error(error.clone());
                Refusal::Document(Box::new(error.into()))
            }),
            LibraryCommand::Search(text) => {
                self.set_query(&text);
                Ok(())
            }
            LibraryCommand::SetRhythm(query) => {
                self.set_rhythm_query(query);
                Ok(())
            }
            LibraryCommand::Select(entry) => self.select_entry(&entry),
            LibraryCommand::Navigate(direction) => {
                self.navigate_library(match direction {
                    Navigation::Previous => -1,
                    Navigation::Next => 1,
                    Navigation::First => -2,
                    Navigation::Last => 2,
                });
                Ok(())
            }
            LibraryCommand::Refresh => {
                self.refresh_library();
                Ok(())
            }
            LibraryCommand::Initialize => self.initialize_library(),
            LibraryCommand::EditSelected => self.edit_selected(),
            LibraryCommand::EditEntry(entry) => self.edit_entry(&entry),
            LibraryCommand::AddSelected => self.add_selected_to_library(),
            LibraryCommand::AddEntry(entry) => self.add_entry(&entry),
            LibraryCommand::Mutate { entry, mutation } => self.mutate_entry(&entry, mutation),
            LibraryCommand::CloseSelected => {
                self.close_open_file();
                Ok(())
            }
            LibraryCommand::CloseEntry(entry) => self.close_entry(&entry),
            LibraryCommand::ShowSaved => {
                if self.show_saved() {
                    self.effects.push(Effect::ResetLibraryInputs);
                }
                Ok(())
            }
        }
    }
    fn apply_editor(&mut self, command: EditorCommand) -> Result<(), Refusal> {
        let rate = self.diagnostic_rate();
        let command = if let EditorCommand::Slide { field, position } = command {
            EditorState::slider_command(field, position, rate)?
        } else {
            command
        };
        match &command {
            EditorCommand::New
            | EditorCommand::CopySelected
            | EditorCommand::ShowBasedOnSource
            | EditorCommand::SetExportRate(_) => {}
            EditorCommand::AddConstruction { row, kind } => {
                self.editing_allowed()?;
                self.editor.addition_admission(*row, kind.component())?;
            }
            EditorCommand::AddComponent { row, kind } => {
                self.component_add_admission(*row, *kind)?;
            }
            EditorCommand::AddPair { row, kind } => self.pair_add_admission(*row, *kind)?,
            EditorCommand::RestoreComponent(row) => self.restore_component_admission(*row)?,
            _ => self.editing_allowed()?,
        }
        match self.editor.apply(command, rate) {
            editor::EditorTransition::Request(request) => match request {
                editor::EditorRequest::New => self.new_document(),
                editor::EditorRequest::CopySelected => self.copy_selected(),
                editor::EditorRequest::ShowBasedOnSource => {
                    let revealed = self.editor().draft().based_on().is_some();
                    self.show_based_on_source();
                    if revealed {
                        self.effects.push(Effect::ResetLibraryInputs);
                    }
                    Ok(())
                }
            },
            editor::EditorTransition::RateChanged(changed) => {
                if changed {
                    self.changed();
                }
                Ok(())
            }
            editor::EditorTransition::Staged(result) => {
                self.changed();
                result
            }
            editor::EditorTransition::Committed(result) => self.after_edit(result),
            editor::EditorTransition::Refused(refusal) => Err(refusal),
        }
    }
}
