//! The interactive application state shared by desktop frontends.
//!
//! A [`Workspace`] owns the recipe library and its selection, one open
//! document, the selected export rate, exactly one audio task slot and one
//! file task slot, at most one prompt and at most one queued action. A
//! frontend sends typed commands, delivers the [`Effect`]s it is asked to
//! perform (dialogs, file panels, the answer to a close request) and renders
//! the views. It decides no admission, validity, dirty state or continuation
//! itself.
//!
//! Admission: Save and Import may run beside audio; Export requires idle
//! audio and excludes starting audio. Stop affects only audio; cancelling an
//! export affects only the export. A task's slot stays occupied until its
//! thread has exited and been joined, and an action waiting for a task runs
//! only after that join.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

mod audio;
mod choices;
pub use choices::{
    ChoiceOption, ChoiceView, ControlChoices, ControlMenu, ControlValue, FilterInput, QueryFacet,
};
mod commands;
pub use commands::{
    AudioCommand, Command, EditorCommand, FileCommand, LibraryCommand, Navigation, OutputCommand,
};
pub(crate) use views::CODES as VIEW_CODES;
mod editor;
mod file;
mod library;
mod prompts;
use crate::job::{Cancel, Progress};
use audio::{AudioRequest, AudioState};
use editor::{DocumentLocation, EditorState};
use file::FileState;
use prompts::{PromptState, Prompts};
pub mod output;
pub mod status;
pub(crate) mod tasks;
mod views;

pub use library::{
    EntryHandle, EntryMutation, LibraryBinding, LibraryData, LibraryOutcome, SelectionBinding,
};
pub use output::{MotionPreference, OutputView};
pub use tasks::{
    Executor, FileOutcome, Job, JobContext, JobKind, JobOutcome, LibraryFailure, Runner,
    ShutdownError, ShutdownIssue, TaskFailure, TaskId, WorkerFailure,
};
pub use views::{
    ActionView, ComponentView, EditorView, FacetCounts, FacetKind, FieldDiagnostic,
    LibraryFacetView, LibraryRowView, LibraryView, MetadataView, MonitorView, ParameterView,
    TransportView, WorkspaceActions,
};

use crate::audio::session::{PlaybackControl, PlaybackPhase, Session, StopRequest};
use crate::document::editing::{
    CommitRefusal, EditRefusal, EditingField, FieldHandle, InputProblem, RowKey, SaveRefusal,
};
use crate::document::editing::{DocumentId, Draft, EditorSession};
use crate::document::recipe::file_name;
use crate::document::recipe::{RecipeSnapshot, Rendition};
use auditorylab_core::{CarrierKind, Ears, ModulationKind, Recipe, SampleRate};
use output::OutputState;
use status::{
    AudioStatus, EditStatus, FileActivity, FileNotice, FileStatus, InventoryStatus, LibraryStatus,
    SaveFeedback,
};
use std::path::PathBuf;
use std::sync::Arc;
use tasks::{AudioKind, AudioTask, Conclusion, FileKind, FileTask};
use {crate::outcome::Outcome, crate::services::OperationError as Error};

/// Which part of the workspace the person is using; it decides what Play,
/// Export and the preview act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// The recipe library: Play plays the selected recipe to its end.
    Library,
    /// The open document: Play runs it live until stopped.
    Workbench,
}

/// Capabilities available to a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Whether this workspace offers default-device playback at runtime.
    pub device_playback: bool,
}

impl Capabilities {
    /// Allow device playback; discovery remains in the admitted audio job.
    #[must_use]
    pub const fn available() -> Self {
        Self {
            device_playback: true,
        }
    }

    /// Device playback is disabled by runtime policy for this workspace.
    #[must_use]
    pub const fn no_device_playback() -> Self {
        Self {
            device_playback: false,
        }
    }
}

/// The identity of one prompt; a response to an older prompt is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromptId(u64);

/// What a file panel chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPurpose {
    /// An existing recipe to open.
    OpenRecipe,
    /// A destination for exported audio; replacement follows the panel's
    /// own overwrite confirmation.
    ExportWav,
    /// Export a captured portable recipe without changing ownership or history.
    ExportRecipe,
}

/// Something the frontend must do on the workspace's behalf.
#[derive(Debug)]
pub enum Effect {
    /// Execute an immutable admitted job outside the coordinator.
    Run(Job),
    /// Ask whether to save, discard or keep unsaved changes.
    AskUnsaved(PromptId),
    /// Confirm deletion of one captured trashed identity before acquiring a lock.
    AskPurge {
        /// Prompt receiving the explicit confirmation.
        prompt: PromptId,
        /// Captured stable User key.
        key: String,
        /// Captured title shown with that identity.
        title: String,
    },
    /// Show a file panel.
    ChoosePath {
        /// The prompt the answer belongs to.
        prompt: PromptId,
        /// What the path is for.
        purpose: PathPurpose,
        /// A suggested file name for destinations.
        suggested_name: Option<String>,
    },
    /// The pending close request is decided; approve or deny termination.
    CloseResolved {
        /// Whether the application may close now.
        approved: bool,
    },
    /// Discard staged filter inputs after an explicit app-owned query reset.
    ResetLibraryInputs,
    /// Show another view.
    ShowView(View),
}

/// A frontend's answer to a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAnswer {
    /// Save, then continue.
    Save,
    /// Continue without saving.
    Discard,
    /// Confirm the explicitly described permanent deletion.
    Confirm,
    /// Keep everything as it is.
    Cancel,
    /// The chosen path, or `None` when the panel was cancelled.
    Path(Option<PathBuf>),
    /// The prompt could not be shown.
    Failed(String),
}

/// An executor response correlated by admitted job identity.
pub enum Response {
    /// Audio preparation finished at the actual discovered device rate.
    Prepared {
        /// Admitted audio job.
        job: TaskId,
        /// Unique monitoring reader.
        reader: Box<crate::audio::session::MonitorReader>,
        /// Actual device rate.
        rate: SampleRate,
        /// Live-update control, when playing an editable document.
        live: Option<crate::audio::session::LiveControl>,
    },
    /// The executor has joined this job; its slot may now be released.
    Finished {
        /// Admitted job identity.
        job: TaskId,
        /// Typed operation or worker outcome.
        outcome: JobOutcome,
    },
}

/// Classification of this response, independent of previous operation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseDisposition {
    /// A current response was applied.
    Applied,
    /// The identity is stale or preparation is no longer accepted.
    Ignored,
    /// The response violated its immutable admitted operation contract.
    Rejected,
}

/// Why a command was not carried out.
#[derive(Debug, Clone)]
pub enum Refusal {
    /// The configured persistent library cannot be used.
    LibraryUnavailable(crate::storage::library::LibraryDiagnostic),
    /// Device playback is disabled by this workspace’s runtime policy.
    DevicePlaybackUnavailable,
    /// A prompt is open.
    PromptOpen,
    /// Another action is waiting for tasks to finish.
    ActionPending,
    /// The file slot is occupied.
    FileBusy,
    /// Audio is playing; export needs idle audio.
    AudioPlaying,
    /// A WAV export is running; audio cannot start.
    Exporting,
    /// Nothing is playing.
    NoAudio,
    /// No export is running.
    NoExport,
    /// Staged input does not parse or numbers are not finite.
    Input(Vec<(EditingField, InputProblem)>),
    /// The recipe's numbers are not all finite.
    NotFinite,
    /// The document does not serialize.
    Document(Box<Error>),
    /// A captured selection is no longer current.
    SelectionChanged,
    /// Adding requires an opened temporary file.
    TemporaryRequired,
    /// Permanent deletion requires the current Trash version.
    PurgeSelectionChanged,
    /// A detached draft must not recreate its old saved identity.
    DetachedDraft(crate::document::origin::LibraryKey),
    /// A draft UUID already belongs to a saved entry.
    ExistingSavedIdentity(crate::core::DocumentUuid),
    /// Retained unreadable saved content is inspection-only.
    SavedEntryUnreadable,
    /// A selected Trash entry must be restored before playback.
    RestoreBeforePlayback,
    /// The source is not valid at the export rate.
    Invalid {
        /// The captured sound's title.
        title: String,
        /// The rate used for this check.
        rate: SampleRate,
        /// The acoustic failure, when analysis produced one.
        error: Option<auditorylab_core::SoundError>,
    },
    /// No recipe is selected: the search shows none.
    NoSelection,
    /// An editing command addressed something that no longer exists.
    Edit(EditRefusal),
}

#[derive(Debug, Clone)]
enum DocumentAction {
    New,
    Copy(RecipeSnapshot),
    EditManaged(crate::storage::library::LoadedRecord),
    EditExternal(RecipeSnapshot),
    TrashSaved {
        key: crate::document::origin::LibraryKey,
        expect: crate::document::origin::VersionToken,
    },
    Close,
}

#[derive(Debug, Clone)]
enum Action {
    /// Perform a document action once its dependencies are joined.
    Document(DocumentAction),
    /// Decide a document action again once the file task has finished.
    Retry(DocumentAction),
    /// Start audio once the previous audio task has finished.
    StartAudio(AudioRequest),
    /// Perform a document action if the save published and the document is
    /// still clean.
    AfterSave {
        action: DocumentAction,
        task: TaskId,
        document: DocumentId,
    },
}

impl Action {
    const fn closes(&self) -> bool {
        matches!(
            self,
            Self::Document(DocumentAction::Close)
                | Self::Retry(DocumentAction::Close)
                | Self::AfterSave {
                    action: DocumentAction::Close,
                    ..
                }
        )
    }
}

/// A queued action and the particular tasks it waits for.
#[derive(Debug)]
struct Pending {
    action: Action,
    audio: Option<TaskId>,
    file: Option<TaskId>,
    failed: bool,
    saved: Option<bool>,
}

/// Interactive application state for one window of work.
pub struct Workspace {
    library: library::LibraryState,
    view: View,
    editor: EditorState,
    audio: AudioState,
    files: FileState,
    prompts: Prompts,
    pending: Option<Pending>,
    closing: bool,
    exiting: bool,
    effects: Vec<Effect>,
    next_id: u64,
    revision: u64,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("view", &self.view)
            .field("document", &self.editor.session().document())
            .field("audio", &self.audio.task().map(|t| t.id))
            .field("file", &self.files.task().map(|t| t.id))
            .finish_non_exhaustive()
    }
}

impl Workspace {
    /// A workspace over a library, with a new blank document.
    ///
    /// # Errors
    /// Returns an error only when document identities are exhausted.
    pub fn with_binding(
        catalog: Vec<RecipeSnapshot>,
        capabilities: Capabilities,
        binding: LibraryBinding,
    ) -> Result<Self, Error> {
        let library = library::LibraryState::new(binding, catalog)?;
        let mut workspace = Self {
            library,
            view: View::Library,
            editor: EditorState::new()?,
            audio: AudioState::new(capabilities),
            files: FileState::default(),
            prompts: Prompts::default(),
            pending: None,
            closing: false,
            exiting: false,
            effects: Vec::new(),
            next_id: 1,
            revision: 0,
        };
        workspace.continue_library();
        Ok(workspace)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Increases whenever anything a view shows may have changed, except
    /// continuously changing telemetry such as elapsed time.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Take the effects the frontend must perform, in order.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    #[must_use]
    pub const fn view(&self) -> View {
        self.view
    }

    /// Whether a prompt is open.
    #[must_use]
    pub const fn prompt_open(&self) -> bool {
        self.prompts.is_open()
    }

    /// Whether an action is waiting for tasks.
    #[must_use]
    pub const fn action_pending(&self) -> bool {
        self.pending.is_some()
    }

    // ---- library ----

    /// Switch between the library and the open document.
    fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.changed();
        }
    }

    // ---- editing ----

    /// Send the committed document to a bound live session, compiled at the
    /// session's actual rate. Invalid committed values keep the last valid
    /// accepted target; staged text is never sent.
    fn submit_live(&mut self) {
        self.audio.submit_live(&mut self.editor);
    }

    // ---- prompts and document actions ----

    fn prompt(&mut self, state: PromptState) -> PromptId {
        let id = PromptId(self.next_id());
        self.prompts.open(id, state);
        id
    }

    fn deny(&mut self, closes: bool) {
        if closes && std::mem::take(&mut self.closing) {
            self.effects.push(Effect::CloseResolved { approved: false });
        }
    }

    /// Start a new blank document, asking about unsaved changes first.
    fn new_document(&mut self) -> Result<(), Refusal> {
        self.request(DocumentAction::New)
    }

    /// Open an editable copy of the selected recipe.
    fn copy_selected(&mut self) -> Result<(), Refusal> {
        if let Err(refusal) = self.copy_admission() {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
            self.changed();
            return Err(refusal);
        }
        let snapshot = self
            .selected()
            .cloned()
            .unwrap_or_else(|| unreachable!("copy admission retains the selected snapshot"));
        self.request(DocumentAction::Copy(snapshot))
    }

    /// Request closing the application. Every close entry point uses this
    /// same decision; repeated requests while one is pending coalesce. The
    /// answer arrives as [`Effect::CloseResolved`] after the tasks that
    /// closing depends on have been joined.
    fn close(&mut self) -> Result<(), Refusal> {
        if self.closing {
            return Ok(());
        }
        self.closing = true;
        self.request(DocumentAction::Close)
    }

    fn request(&mut self, action: DocumentAction) -> Result<(), Refusal> {
        let closes = matches!(action, DocumentAction::Close);
        if let Err(refusal) = self.start_allowed() {
            self.files
                .record_status(FileStatus::Refused(refusal.clone()));
            self.deny(closes);
            self.changed();
            return Err(refusal);
        }
        if self.files.busy() {
            self.schedule(Action::Retry(action));
            self.changed();
            return Ok(());
        }
        // Commit staged text when it can be; anything left staged is unsaved.
        let _ = self.editor.commit();
        self.submit_live();
        if self.editor.session().dirty() {
            let id = self.prompt(PromptState::Unsaved(action));
            self.effects.push(Effect::AskUnsaved(id));
        } else {
            self.schedule(Action::Document(action));
        }
        self.changed();
        Ok(())
    }

    /// Deliver executor preparation or joined completion.
    pub fn respond(&mut self, response: Response) -> ResponseDisposition {
        let mut rejected = false;
        match response {
            Response::Prepared {
                job,
                reader,
                rate,
                live,
            } => {
                if self.exiting {
                    return ResponseDisposition::Ignored;
                }
                match self.audio.accept_prepared(job, *reader, rate, live) {
                    ResponseDisposition::Ignored => return ResponseDisposition::Ignored,
                    ResponseDisposition::Rejected => {
                        self.changed();
                        return ResponseDisposition::Rejected;
                    }
                    ResponseDisposition::Applied => {}
                }
                self.submit_live();
                self.changed();
            }
            Response::Finished { job, outcome } => {
                if let Some(task) = self.audio.take_finished(job) {
                    rejected = task.protocol_failed || matches!(outcome, JobOutcome::File(_));
                    self.finish_audio(
                        task,
                        match outcome {
                            JobOutcome::Audio(result) => Ok(result.map_err(Error::from)),
                            JobOutcome::Failed(error) => Err(error),
                            JobOutcome::File(_) => Err(WorkerFailure::InvalidOutcome),
                        },
                    );
                } else if let Some(task) = self.files.take_finished(job) {
                    rejected = match &outcome {
                        JobOutcome::File(result) => !task.kind.accepts(result),
                        JobOutcome::Audio(_) => true,
                        JobOutcome::Failed(_) => false,
                    };
                    self.finish_file(
                        task,
                        match outcome {
                            JobOutcome::File(result) => Ok(result),
                            JobOutcome::Failed(error) => Err(error),
                            JobOutcome::Audio(_) => Err(WorkerFailure::InvalidOutcome),
                        },
                    );
                } else {
                    return ResponseDisposition::Ignored;
                }
            }
        }
        self.poll();
        if rejected {
            ResponseDisposition::Rejected
        } else {
            ResponseDisposition::Applied
        }
    }

    fn respond_prompt(&mut self, prompt: PromptId, response: PromptAnswer) -> bool {
        let Some(state) = self.prompts.take(prompt) else {
            return false;
        };
        if matches!(response, PromptAnswer::Path(Some(_)))
            && (self.files.busy() || self.pending.is_some())
        {
            self.files
                .record_status(FileStatus::Refused(Refusal::FileBusy));
            self.deny(matches!(state, PromptState::Unsaved(DocumentAction::Close)));
            self.changed();
            return true;
        }
        match (state, response) {
            (PromptState::Purge { key, expect }, PromptAnswer::Confirm) => {
                let result = self.purge_confirmed(key, expect);
                if let Err(refusal) = result {
                    self.files
                        .record_status(FileStatus::Refused(refusal.clone()));
                }
            }

            (PromptState::Unsaved(action), PromptAnswer::Save) => {
                let closes = matches!(action, DocumentAction::Close);
                match self.prepare_save() {
                    Ok(content) => {
                        if let Err(refusal) = self.editor.commit() {
                            self.files.record_status(FileStatus::Refused(refusal));
                            self.deny(closes);
                            return true;
                        }
                        self.submit_live();
                        let document = self.editor.session().document();
                        match self.start_managed_save(document, content) {
                            Ok(task) => self.schedule(Action::AfterSave {
                                action,
                                task,
                                document,
                            }),
                            Err(refusal) => {
                                self.files
                                    .record_status(FileStatus::Refused(refusal.clone()));
                                self.deny(closes);
                            }
                        }
                    }
                    Err(refusal) => {
                        self.files
                            .record_status(FileStatus::Refused(refusal.clone()));
                        self.deny(closes);
                    }
                }
            }
            (PromptState::Unsaved(action), PromptAnswer::Discard) => {
                self.schedule(Action::Document(action));
            }
            (PromptState::ExportRecipe(text), PromptAnswer::Path(Some(path))) => {
                self.start_file(FileKind::ExportRecipe { text, path });
            }
            (PromptState::Import, PromptAnswer::Path(Some(path))) => {
                self.start_file(FileKind::Import(path));
            }
            (PromptState::Export(sound), PromptAnswer::Path(Some(path))) => {
                if self.audio.busy() {
                    self.files
                        .record_status(FileStatus::Refused(Refusal::AudioPlaying));
                } else {
                    let frames = sound.signal().frames();
                    let title = sound.snapshot().title();
                    self.start_file(FileKind::Export {
                        title,
                        path,
                        frames,
                        cancel: Cancel::default(),
                        progress: Progress::default(),
                        sound,
                    });
                }
            }
            (state, response) => {
                if let PromptAnswer::Failed(message) = response {
                    self.files
                        .record_status(FileStatus::DialogFailed { cause: message });
                }
                let closes = matches!(state, PromptState::Unsaved(DocumentAction::Close));
                self.deny(closes);
            }
        }
        self.changed();
        true
    }

    /// Queue an action behind the tasks it depends on, asking them to finish:
    /// closing stops audio and cancels an export; replacing audio stops the
    /// current audio. Saves and imports always run to completion.
    fn schedule(&mut self, action: Action) {
        let (audio, file) = match &action {
            Action::Document(DocumentAction::Close) | Action::Retry(DocumentAction::Close) => {
                (true, true)
            }
            Action::StartAudio(_) => (true, false),
            Action::AfterSave {
                action: DocumentAction::Close,
                ..
            } => (true, true),
            _ => (false, true),
        };
        if audio {
            let _ = self.audio.stop();
        }
        if file {
            let _ = self.files.cancel_export();
        }
        let pending = Pending {
            audio: if audio {
                self.audio.task().map(|t| t.id)
            } else {
                None
            },
            file: if file {
                self.files.task().map(|t| t.id)
            } else {
                None
            },
            action,
            failed: false,
            saved: None,
        };
        if pending.audio.is_none() && pending.file.is_none() {
            self.run(pending);
        } else {
            self.pending = Some(pending);
        }
    }

    fn run(&mut self, pending: Pending) {
        if pending.failed {
            self.deny(pending.action.closes());
            return;
        }
        match pending.action {
            Action::Document(DocumentAction::Close) => {
                self.closing = false;
                self.effects.push(Effect::CloseResolved { approved: true });
            }
            Action::Document(DocumentAction::New) => match Draft::new() {
                Ok(draft) => {
                    self.replace_document(draft);
                }
                Err(e) => self.files.record_status(FileStatus::Failed(e.into())),
            },
            Action::Document(DocumentAction::Copy(snapshot)) => {
                if let Err(refusal) = Self::copy_content_admission(&snapshot) {
                    self.files
                        .record_status(FileStatus::Refused(refusal.clone()));
                } else {
                    match Draft::from_snapshot(&snapshot) {
                        Ok(draft) => {
                            self.replace_document(draft);
                        }
                        Err(e) => self.files.record_status(FileStatus::Failed(e.into())),
                    }
                }
            }
            Action::Document(DocumentAction::TrashSaved { key, expect }) => {
                self.discard_then_trash(key, expect);
            }
            Action::Document(DocumentAction::EditExternal(snapshot)) => {
                if self.replace_document(Draft::from_recipe(snapshot.recipe().clone()))
                    && self
                        .editor
                        .record_saved(self.editor.session().document(), snapshot.recipe())
                {
                    self.editor.locate(
                        snapshot
                            .external_file()
                            .cloned()
                            .map_or(DocumentLocation::Unsaved, DocumentLocation::External),
                    );
                }
            }
            Action::Document(DocumentAction::EditManaged(record)) => {
                if self.replace_document(Draft::from_recipe(record.snapshot.recipe().clone()))
                    && self
                        .editor
                        .record_saved(self.editor.session().document(), record.snapshot.recipe())
                {
                    self.editor.locate(DocumentLocation::Managed {
                        key: record.key,
                        token: record.version_token,
                    });
                }
            }
            Action::Retry(action) => {
                let _ = self.request(action);
            }
            Action::StartAudio(request) => self.start_audio(request),
            Action::AfterSave {
                action, document, ..
            } => {
                if pending.saved == Some(true)
                    && self.editor.session().document() == document
                    && !self.editor.session().dirty()
                {
                    let action = if let DocumentAction::TrashSaved { key, expect } = action {
                        let expect = self.editor.location().saved_token(&key).unwrap_or(expect);
                        DocumentAction::TrashSaved { key, expect }
                    } else {
                        action
                    };
                    self.schedule(Action::Document(action));
                } else {
                    if pending.saved == Some(true) {
                        self.files
                            .record_status(FileStatus::Notice(FileNotice::SavedSnapshotStillDirty));
                    }
                    self.deny(matches!(action, DocumentAction::Close));
                }
            }
        }
    }

    fn replace_document(&mut self, draft: Draft) -> bool {
        match EditorSession::new(draft) {
            Ok(editor) => {
                self.install_document(editor);
                true
            }
            Err(error) => {
                self.files.record_status(FileStatus::Failed(error.into()));
                false
            }
        }
    }

    fn install_document(&mut self, editor: EditorSession) {
        self.editor.install(editor);
        self.view = View::Workbench;
        self.effects.push(Effect::ShowView(View::Workbench));
        self.files.record_status(FileStatus::Idle);
    }

    // ---- tasks ----

    fn start_allowed(&self) -> Result<(), Refusal> {
        self.prompts.admit_start(self.pending.is_some())
    }

    fn start_file(&mut self, kind: FileKind) -> TaskId {
        let id = TaskId(self.next_id());
        let job = self.files.start(id, kind);
        self.effects.push(Effect::Run(job));
        id
    }

    fn start_audio(&mut self, request: AudioRequest) {
        let id = TaskId(self.next_id());
        let job = self.audio.start(id, request);
        self.effects.push(Effect::Run(job));
    }

    // ---- polling ----

    /// Refresh pure views/models and schedule captured library jobs.
    /// External completions arrive through respond; the Runner owns IO and joins.
    /// Returns whether anything a view shows may have changed.
    pub fn poll(&mut self) -> bool {
        let before = self.revision;
        self.resolve_pending();
        self.continue_library();
        let shown = self.selected().cloned();
        let export_rate = self.editor.export_rate();
        if self
            .audio
            .refresh(&mut self.editor, shown.as_ref(), self.view, export_rate)
        {
            self.changed();
        }
        let live_document = self.live_rate().is_some();
        let rate = match self.view {
            View::Workbench => self.diagnostic_rate(),
            View::Library => self.editor.export_rate(),
        };
        if self.audio.refresh_model(
            &mut self.editor,
            shown.as_ref(),
            self.view,
            live_document,
            rate,
        ) {
            self.changed();
        }
        before != self.revision
    }

    fn finish_audio(
        &mut self,
        task: AudioTask,
        result: Result<Result<crate::audio::session::PlaybackReport, Error>, WorkerFailure>,
    ) {
        let conclusion = self.audio.finish(&task, result);
        self.conclude(task.id, true, conclusion, None);
        self.changed();
    }

    fn finish_file(&mut self, task: FileTask, result: Result<FileOutcome, WorkerFailure>) {
        let outcome = match result {
            Ok(outcome) if task.kind.accepts(&outcome) => outcome,
            result => {
                let status = FileStatus::WorkerFailed {
                    activity: FileActivity::from_kind(&task.kind),
                    error: result.err().unwrap_or(WorkerFailure::InvalidOutcome),
                };
                if let FileKind::Managed(job) = &task.kind
                    && job.background_read()
                {
                    self.library.record_read_status(job, &status);
                } else {
                    self.files.record_status(status);
                }
                self.conclude(task.id, false, Conclusion::Failed, Some(false));
                self.changed();
                return;
            }
        };
        let mut conclusion = outcome.conclusion();
        let mut saved = None;
        let status = match (&task.kind, outcome) {
            (FileKind::Managed(job), FileOutcome::Managed(result)) => {
                let (status, confirmed) = self.finish_managed(job, result);
                saved = confirmed;
                status
            }
            (_, FileOutcome::Imported(Ok(snapshot))) => match self.add_import(snapshot.clone()) {
                Ok(()) => FileStatus::Imported {
                    snapshot,
                    hidden: self.selected().is_none(),
                },
                Err(error) => {
                    conclusion = Conclusion::Failed;
                    FileStatus::Failed(error)
                }
            },
            (FileKind::ExportRecipe { path, .. }, FileOutcome::RecipeExported(Ok(outcome))) => {
                FileStatus::RecipeExported {
                    path: path.clone(),
                    warnings: outcome.warnings,
                }
            }
            (FileKind::Export { path, .. }, FileOutcome::Exported(Ok(exported))) => {
                FileStatus::Exported {
                    path: path.clone(),
                    exported,
                }
            }
            (_, FileOutcome::Exported(Err(error))) if error.outcome() == Outcome::Cancelled => {
                FileStatus::Notice(FileNotice::ExportCancelled)
            }
            (_, FileOutcome::Imported(Err(error))) => FileStatus::Failed(error.into()),
            (_, FileOutcome::RecipeExported(Err(error))) => FileStatus::Failed(error.into()),
            (_, FileOutcome::Exported(Err(error))) => FileStatus::Failed(error.into()),
            _ => FileStatus::WorkerFailed {
                activity: FileActivity::from_kind(&task.kind),
                error: WorkerFailure::InvalidOutcome,
            },
        };
        let conclusion = status.conclusion(conclusion);
        if let FileKind::Managed(job) = &task.kind
            && job.background_read()
        {
            self.library.record_read_status(job, &status);
        } else {
            self.files.record_status(status);
        }
        self.conclude(task.id, false, conclusion, saved);
        self.changed();
    }

    fn add_import(&mut self, snapshot: RecipeSnapshot) -> Result<(), Error> {
        self.library.import(snapshot)
    }

    fn conclude(&mut self, task: TaskId, audio: bool, conclusion: Conclusion, saved: Option<bool>) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let waited = if audio {
            &mut pending.audio
        } else {
            &mut pending.file
        };
        if *waited == Some(task) {
            *waited = None;
            if conclusion == Conclusion::Failed {
                pending.failed = true;
            }
            if let Action::AfterSave { task: save, .. } = &pending.action
                && *save == task
            {
                pending.saved = saved;
            }
        }
    }

    fn resolve_pending(&mut self) {
        if self.exiting {
            return;
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.audio.is_none() && p.file.is_none())
            && let Some(pending) = self.pending.take()
        {
            self.run(pending);
            self.changed();
        }
    }

    /// End all work after the event loop exits: request Stop, cancel an
    /// export, wait for saves and imports, and join both workers. Expected
    /// Stop and cancellation are not failures.
    ///
    /// # Errors
    /// Returns every actual failure, one per line.
    fn prepare_shutdown(&mut self) {
        self.exiting = true;
        self.pending = None;
        self.prompts.clear();
        self.closing = false;
        let _ = self.audio.stop();
        let _ = self.files.cancel_export();
    }
    fn job_context(&self, job: TaskId) -> JobContext {
        if let Some(task) = self.files.task().filter(|task| task.id == job) {
            return match &task.kind {
                FileKind::Managed(job) => JobContext::Library(job.context()),
                FileKind::Import(path)
                | FileKind::ExportRecipe { path, .. }
                | FileKind::Export { path, .. } => JobContext::File(path.clone()),
            };
        }
        self.audio
            .task()
            .filter(|task| task.id == job)
            .map_or(JobContext::Unknown, |task| {
                JobContext::Audio(task.title.clone())
            })
    }

    // ---- display policy ----
}

#[cfg(test)]
mod tests;

pub mod monitor;

/// Editing gestures and display defaults.
pub mod controls;

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

impl std::error::Error for Refusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(cause) => Some(cause),
            Self::Edit(cause) => Some(cause),
            Self::Invalid {
                error: Some(cause), ..
            } => Some(cause),
            Self::Input(problems) => problems
                .first()
                .map(|(_, cause)| cause as &(dyn std::error::Error + 'static)),
            Self::LibraryUnavailable(problem) => match &problem.detail {
                crate::services::library::DiagnosticDetail::Failure(cause) => Some(cause.as_ref()),
                _ => None,
            },
            _ => None,
        }
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    Refusal {
        codes {
            Self::DevicePlaybackUnavailable => "workspace.device_playback_unavailable",
            Self::PromptOpen => "workspace.prompt_open",
            Self::ActionPending => "workspace.action_pending",
            Self::FileBusy => "workspace.file_busy",
            Self::AudioPlaying => "workspace.audio_playing",
            Self::Exporting => "workspace.exporting",
            Self::NoAudio => "workspace.no_audio",
            Self::NoExport => "workspace.no_export",
            Self::Input(..) => "workspace.input",
            Self::NotFinite => "workspace.not_finite",
            Self::SelectionChanged => "workspace.selection_changed",
            Self::TemporaryRequired => "workspace.temporary_required",
            Self::PurgeSelectionChanged => "workspace.purge_selection_changed",
            Self::DetachedDraft(..) => "workspace.detached_draft",
            Self::ExistingSavedIdentity(..) => "workspace.existing_saved_identity",
            Self::SavedEntryUnreadable => "workspace.saved_entry_unreadable",
            Self::RestoreBeforePlayback => "workspace.restore_before_playback",
            Self::Invalid { error: None, .. } => "workspace.invalid",
            Self::NoSelection => "workspace.no_selection",
        }
        delegates {
            Self::LibraryUnavailable(problem) => problem,
            Self::Document(error) => error,
            Self::Edit(error) => error,
            Self::Invalid {
                error: Some(error), ..
            } => error,
        }
        params(this) {
            match this {
                Self::Input(problems) => {
                    crate::core::diagnostic_params! {"problem_count"=>problems.len()}
                }
                Self::DetachedDraft(key) => {
                    crate::core::diagnostic_params! {"selector"=>key.to_string()}
                }
                Self::ExistingSavedIdentity(id) => {
                    crate::core::diagnostic_params! {"document_id"=>id.to_string()}
                }
                Self::Invalid { title, rate, .. } => {
                    crate::core::diagnostic_params! {"title"=>title.clone(),"diagnostic_rate_hz"=>rate.hz()}
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
}
