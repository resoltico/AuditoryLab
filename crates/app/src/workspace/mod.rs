// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

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
mod library;
mod operations;
mod output;
mod status;
mod tasks;
mod views;

pub use library::{LibraryBinding, LibraryOutcome};
pub use output::{MotionPreference, OutputView};
pub use tasks::{FileOutcome, Job, JobKind, JobOutcome, Runner, TaskId, WorkerFailure};
pub use views::{
    ActionView, ComponentView, EditorView, LibraryFacetView, LibraryRowView, LibraryView, MetadataView, ParameterView, TransportView,
};

use crate::controls;
use crate::editing::{
    CommitRefusal, EditRefusal, EditingField, FieldHandle, InputProblem, RowKey, SaveRefusal,
};
use crate::editing::{DocumentId, Draft, EditorSession};
use crate::error::{Error, ErrorKind};
use crate::recipe::file_name;
use crate::recipe::{RecipeSnapshot, Rendition};
use crate::session::{PlaybackControl, PlaybackPhase, Session, StopRequest};
use crate::waveform_view::WaveformScale;
use crate::workflow::{Cancel, Progress};
use auditorylab_core::{CarrierKind, Ears, ModulationKind, Recipe, SampleRate, Sound};
use output::OutputState;
use status::{
    AudioStatus, EditStatus, FileActivity, FileNotice, FileStatus, InventoryStatus, LibraryStatus,
    SaveFeedback,
};
use std::path::PathBuf;
use std::sync::Arc;
use tasks::{AudioKind, AudioTask, Conclusion, FileKind, FileTask};

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

/// An external response correlated by prompt or admitted job identity.
pub enum Response {
    /// A native dialog or file panel answer.
    Prompt {
        /// Prompt being answered.
        prompt: PromptId,
        /// Typed dialog answer.
        answer: PromptAnswer,
    },
    /// Audio preparation finished at the actual discovered device rate.
    Prepared {
        /// Admitted audio job.
        job: TaskId,
        /// Unique monitoring reader.
        reader: Box<crate::session::MonitorReader>,
        /// Actual device rate.
        rate: SampleRate,
        /// Live-update control, when playing an editable document.
        live: Option<crate::session::LiveControl>,
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
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The configured persistent library cannot be used.
    LibraryUnavailable(crate::library::LibraryDiagnostic),
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
    /// The current authored content has a nonfinite number.
    NonfiniteContent {
        /// Zero-based source component, or a recipe-wide field.
        component: Option<usize>,
        /// The source quantity requiring correction.
        parameter: auditorylab_core::ParameterId,
    },
    /// The document does not serialize.
    Document(String),
    /// The source is not valid at the export rate.
    Invalid {
        /// The captured sound's title.
        title: String,
        /// The rate used for this check.
        rate: SampleRate,
        /// The acoustic failure, when analysis produced one.
        error: Option<auditorylab_core::StimulusError>,
    },
    /// No recipe is selected: the search shows none.
    NoSelection,
    /// An editing command addressed something that no longer exists.
    Edit(EditRefusal),
    /// The recipe has more components than a document can hold.
    TooManyComponents(usize),
}

#[derive(Debug, Clone)]
enum DocumentAction {
    New,
    Copy(RecipeSnapshot),
    EditManaged(crate::library::LoadedRecord),
    EditExternal(RecipeSnapshot),
    TrashSaved {
        key: crate::library::LibraryKey,
        expect: crate::library::VersionToken,
    },
    Close,
}

#[derive(Debug, Clone)]
enum AudioRequest {
    Finite {
        snapshot: Box<RecipeSnapshot>,
        store: Option<crate::library::LibraryStore>,
    },
    Live {
        document: DocumentId,
        candidate: Arc<crate::editing::Candidate>,
    },
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

#[derive(Debug)]
enum PromptState {
    Unsaved(DocumentAction),
    Purge {
        key: crate::library::LibraryKey,
        expect: crate::library::VersionToken,
    },
    Import,
    Export(Rendition),
    ExportRecipe(Arc<str>),
}

#[derive(Debug)]
struct Prompt {
    id: PromptId,
    state: PromptState,
}

#[derive(Debug, Clone, Default)]
enum DocumentLocation {
    #[default]
    Unsaved,
    Managed {
        key: crate::library::LibraryKey,
        token: crate::library::VersionToken,
    },
    External(crate::external::ObservedFile),
    Detached(crate::library::LibraryKey),
}
impl DocumentLocation {
    fn managed(&self) -> Option<(&crate::library::LibraryKey, crate::library::VersionToken)> {
        match self {
            Self::Managed { key, token } => Some((key, *token)),
            _ => None,
        }
    }
    fn detached(&self) -> Option<&crate::library::LibraryKey> {
        match self {
            Self::Detached(key) => Some(key),
            _ => None,
        }
    }
    fn external(&self) -> Option<&crate::external::ObservedFile> {
        match self {
            Self::External(file) => Some(file),
            _ => None,
        }
    }
    fn saved_token(
        &self,
        key: &crate::library::LibraryKey,
    ) -> Option<crate::library::VersionToken> {
        self.managed()
            .filter(|(saved, _)| *saved == key)
            .map(|(_, token)| token)
    }
}
struct EditorState {
    session: EditorSession,
    export_rate: SampleRate,
    status: EditStatus,
    location: DocumentLocation,
}
struct AudioState {
    slot: Option<AudioTask>,
    status: AudioStatus,
    output: OutputState,
    capabilities: Capabilities,
}
struct FilesState {
    slot: Option<FileTask>,
    status: FileStatus,
}

/// Interactive application state for one window of work.
pub struct Workspace {
    library: library::LibraryState,
    view: View,
    editor: EditorState,
    audio: AudioState,
    files: FilesState,
    prompt: Option<Prompt>,
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
            .field("document", &self.editor.session.document())
            .field("audio", &self.audio.slot.as_ref().map(|t| t.id))
            .field("file", &self.files.slot.as_ref().map(|t| t.id))
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
            editor: EditorState {
                session: EditorSession::new(Draft::new()?)?,
                export_rate: SampleRate::DEFAULT,
                status: EditStatus::Idle,
                location: DocumentLocation::Unsaved,
            },
            audio: AudioState {
                slot: None,
                status: AudioStatus::Idle,
                output: OutputState::default(),
                capabilities,
            },
            files: FilesState {
                slot: None,
                status: FileStatus::Idle,
            },
            prompt: None,
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

    /// The capabilities in effect.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.audio.capabilities
    }

    /// The open document's editor.
    #[must_use]
    pub const fn editor(&self) -> &EditorSession {
        &self.editor.session
    }

    /// The current view.
    #[must_use]
    pub const fn view(&self) -> View {
        self.view
    }

    /// The selected inspection and export rate; device rates are separate.
    #[must_use]
    pub const fn export_rate(&self) -> SampleRate {
        self.editor.export_rate
    }

    /// Loaded in-memory Built-in and temporary snapshots. Managed User summaries
    /// appear in `library_view`; only selected User content is loaded in full.
    #[must_use]
    pub fn catalog(&self) -> &[RecipeSnapshot] {
        &self.library.catalog
    }

    /// The selected recipe, when the search shows it. A search that hides
    /// every recipe leaves nothing selected.
    #[must_use]
    pub fn selected(&self) -> Option<&RecipeSnapshot> {
        self.library.selected()
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.library.query.search
    }

    /// Whether a prompt is open.
    #[must_use]
    pub const fn prompt_open(&self) -> bool {
        self.prompt.is_some()
    }

    /// Whether an action is waiting for tasks.
    #[must_use]
    pub const fn action_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Whether an audio task occupies its slot.
    #[must_use]
    pub const fn audio_busy(&self) -> bool {
        self.audio.slot.is_some()
    }

    /// Whether a file task occupies its slot.
    #[must_use]
    pub const fn file_busy(&self) -> bool {
        self.files.slot.is_some()
    }

    /// Whether the running file task is an export.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.files
            .slot
            .as_ref()
            .is_some_and(|task| matches!(task.kind, FileKind::Export { .. }))
    }

    // ---- library ----

    fn refilter(&mut self) {
        self.library.refilter();
    }

    /// Search titles, IDs, research text and component meaning. When the
    /// selection stops matching, the first match is selected.
    pub fn set_query(&mut self, query: &str) {
        self.library.query.search = query.into();
        self.refilter();
        self.load_library_selection();
        self.changed();
    }

    /// Change nominal facets without rebinding any playback or editor snapshot.
    pub fn set_rhythm_query(&mut self, query: crate::query::RhythmQuery) {
        self.library.query.rhythm = query;
        self.refilter();
        self.load_library_selection();
        self.changed();
    }

    /// Current shared nominal restrictions, retained when text search changes.
    #[must_use]
    pub const fn rhythm_query(&self) -> &crate::query::RhythmQuery {
        &self.library.query.rhythm
    }

    /// Switch between the library and the open document.
    pub fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.changed();
        }
    }

    /// Select the inspection and export rate. A device's actual rate is
    /// unaffected.
    pub fn set_export_rate(&mut self, rate: SampleRate) {
        if self.editor.export_rate != rate {
            self.editor.export_rate = rate;
            self.changed();
        }
    }

    /// Show the recipe the open document was copied from, by its retained
    /// snapshot rather than by its path.
    pub fn show_based_on_source(&mut self) {
        if let Some(source) = self.editor.session.draft().based_on().cloned() {
            self.reveal_retained_source(source);
        }
    }

    // ---- editing ----

    fn editing_allowed(&self) -> Result<(), Refusal> {
        if self.prompt.is_some() {
            return Err(Refusal::PromptOpen);
        }
        if self.pending.is_some() {
            return Err(Refusal::ActionPending);
        }
        if self
            .files
            .slot
            .as_ref()
            .is_some_and(|t| matches!(t.kind, FileKind::Export { .. } | FileKind::Import(_)))
        {
            return Err(Refusal::FileBusy);
        }
        Ok(())
    }

    /// Whether editing commands are accepted now.
    #[must_use]
    pub fn can_edit(&self) -> bool {
        self.editing_allowed().is_ok()
    }

    fn after_edit(&mut self, result: Result<(), Refusal>) -> Result<(), Refusal> {
        // Field diagnostics already explain input problems; other refusals
        // are reported until the next accepted edit.
        self.editor.status = match &result {
            Ok(()) => {
                self.submit_live();
                EditStatus::Idle
            }
            Err(Refusal::Input(_)) => EditStatus::Idle,
            Err(refusal) => EditStatus::Refused(refusal.clone()),
        };
        self.changed();
        result
    }

    /// Stage typed text. Staging changes diagnostics and readiness only.
    ///
    /// # Errors
    /// Returns the refusal for stale handles or while editing is blocked.
    pub fn stage(&mut self, handle: FieldHandle, text: &str) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .stage(handle, text)
            .map_err(Refusal::Edit);
        self.changed();
        result
    }

    /// Discard a field's staged text; returns its committed text.
    ///
    /// # Errors
    /// Returns the refusal for stale handles.
    pub fn restore(&mut self, handle: FieldHandle) -> Result<String, Refusal> {
        let result = self.editor.session.restore(handle).map_err(Refusal::Edit);
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
        let result = match self.editor.session.commit_field(handle, text) {
            Err(refusal) => Err(Refusal::Edit(refusal)),
            Ok(Err(CommitRefusal { problems })) => Err(Refusal::Input(problems)),
            Ok(Ok(_)) => Ok(()),
        };
        self.after_edit(result)
    }

    /// Commit the whole candidate.
    ///
    /// # Errors
    /// Returns every field that kept the candidate staged.
    pub fn commit(&mut self) -> Result<(), Refusal> {
        let result = self
            .editor
            .session
            .commit()
            .map(|_| ())
            .map_err(|CommitRefusal { problems }| Refusal::Input(problems));
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
        let result = self
            .editor
            .session
            .set_ears(row, ears)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Change a component's carrier kind.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_carrier(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .set_carrier(row, kind)
            .map_err(Refusal::Edit);
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
        let result = self
            .editor
            .session
            .set_modulation(row, kind, rate)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Add a default component.
    ///
    /// # Errors
    /// Returns the refusal at the component limit or while editing is blocked.
    pub fn add_component(&mut self, row: RowKey, kind: CarrierKind) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .add_component(row, kind)
            .map(|_| ())
            .map_err(Refusal::Edit);
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
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .add_pair(row, kind)
            .map(|_| ())
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Remove a component.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn remove_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .remove_component(row)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Put the most recently removed component back where it was.
    ///
    /// # Errors
    /// Returns the refusal when nothing was removed, at the component limit
    /// or while editing is blocked.
    pub fn restore_component(&mut self, row: RowKey) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .session
            .restore_removed(row)
            .map(|_| ())
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// The live session's actual rate, when a live task is bound to the open
    /// document; otherwise the selected export rate. Field diagnostics use it.
    #[must_use]
    pub fn diagnostic_rate(&self) -> SampleRate {
        self.live_rate().unwrap_or(self.editor.export_rate)
    }

    /// The bound live session's actual device rate, if any.
    #[must_use]
    pub fn live_rate(&self) -> Option<SampleRate> {
        self.audio.slot
            .as_ref()
            .filter(|t| matches!(t.kind, AudioKind::Live(document) if document == self.editor.session.document()))
            .and_then(|t| t.rate)
    }

    /// Send the committed document to a bound live session, compiled at the
    /// session's actual rate. Invalid committed values keep the last valid
    /// accepted target; staged text is never sent.
    fn submit_live(&mut self) {
        let Some(task) = self.audio.slot.as_ref() else {
            return;
        };
        let (AudioKind::Live(document), Some(live), Some(rate)) =
            (&task.kind, &task.live, task.rate)
        else {
            return;
        };
        if *document != self.editor.session.document() {
            return;
        }
        let live = live.clone();
        let analysis = self.editor.session.committed_analysis(rate);
        if let Some(target) = analysis.target()
            && let Err(error) = live.update(target)
            && error != crate::session::LiveUpdateError::SessionClosed
        {
            self.audio.status = AudioStatus::LiveUpdate(error);
        }
    }

    // ---- prompts and document actions ----

    fn prompt(&mut self, state: PromptState) -> PromptId {
        let id = PromptId(self.next_id());
        self.prompt = Some(Prompt { id, state });
        id
    }

    fn deny(&mut self, closes: bool) {
        if closes && std::mem::take(&mut self.closing) {
            self.effects.push(Effect::CloseResolved { approved: false });
        }
    }

    /// Start a new blank document, asking about unsaved changes first.
    pub fn new_document(&mut self) {
        self.request(DocumentAction::New);
    }

    /// Open an editable copy of the selected recipe.
    pub fn copy_selected(&mut self) {
        if let Some(snapshot) = self.selected().cloned() {
            if let Err(refusal) = Self::copy_content_admission(&snapshot) {
                self.files.status = FileStatus::Refused(refusal.clone());
                self.changed();
            } else {
                self.request(DocumentAction::Copy(snapshot));
            }
        }
    }

    /// Request closing the application. Every close entry point uses this
    /// same decision; repeated requests while one is pending coalesce. The
    /// answer arrives as [`Effect::CloseResolved`] after the tasks that
    /// closing depends on have been joined.
    pub fn close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.request(DocumentAction::Close);
    }

    fn request(&mut self, action: DocumentAction) {
        let closes = matches!(action, DocumentAction::Close);
        if self.prompt.is_some() || self.pending.is_some() {
            self.files.status = FileStatus::Refused(
                (if self.prompt.is_some() {
                    &Refusal::PromptOpen
                } else {
                    &Refusal::ActionPending
                })
                .clone(),
            );
            self.deny(closes);
            self.changed();
            return;
        }
        if self.files.slot.is_some() {
            self.schedule(Action::Retry(action));
            self.changed();
            return;
        }
        // Commit staged text when it can be; anything left staged is unsaved.
        let _ = self.editor.session.commit();
        self.submit_live();
        if self.editor.session.dirty() {
            let id = self.prompt(PromptState::Unsaved(action));
            self.effects.push(Effect::AskUnsaved(id));
        } else {
            self.schedule(Action::Document(action));
        }
        self.changed();
    }

    /// Answer a prompt. Answers to prompts that are no longer open change nothing.
    pub fn respond(&mut self, response: Response) -> ResponseDisposition {
        let mut rejected = false;
        match response {
            Response::Prompt { prompt, answer } => {
                if self
                    .prompt
                    .as_ref()
                    .is_none_or(|current| current.id != prompt)
                {
                    return ResponseDisposition::Ignored;
                }
                self.respond_prompt(prompt, answer);
            }
            Response::Prepared {
                job,
                reader,
                rate,
                live,
            } => {
                if self.exiting {
                    return ResponseDisposition::Ignored;
                }
                let Some(task) = self.audio.slot.as_mut().filter(|task| task.id == job) else {
                    return ResponseDisposition::Ignored;
                };
                if task.rate.is_some()
                    || task.protocol_failed
                    || reader.descriptor().id != task.control.session()
                    || reader.descriptor().output.sample_rate != rate
                    || matches!(task.kind, AudioKind::Live(_)) != live.is_some()
                    || live.as_ref().is_some_and(|control| {
                        control.session() != task.control.session() || control.sample_rate() != rate
                    })
                {
                    task.protocol_failed = true;
                    task.control.request_stop();
                    self.audio.status = AudioStatus::WorkerFailed(WorkerFailure::InvalidOutcome);
                    self.changed();
                    return ResponseDisposition::Rejected;
                }
                task.rate = Some(rate);
                task.live = live;
                self.audio.output.bind(*reader);
                self.submit_live();
                self.changed();
            }
            Response::Finished { job, outcome } => {
                if self.audio.slot.as_ref().is_some_and(|task| task.id == job) {
                    let task = self
                        .audio
                        .slot
                        .take()
                        .unwrap_or_else(|| unreachable!("matched audio slot"));
                    rejected = task.protocol_failed || matches!(outcome, JobOutcome::File(_));
                    self.finish_audio(
                        task,
                        match outcome {
                            JobOutcome::Audio(result) => Ok(result),
                            JobOutcome::Failed(error) => Err(error),
                            JobOutcome::File(_) => Err(WorkerFailure::InvalidOutcome),
                        },
                    );
                } else if self.files.slot.as_ref().is_some_and(|task| task.id == job) {
                    let task = self
                        .files
                        .slot
                        .take()
                        .unwrap_or_else(|| unreachable!("matched file slot"));
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

    fn respond_prompt(&mut self, prompt: PromptId, response: PromptAnswer) {
        if self.prompt.as_ref().is_none_or(|p| p.id != prompt) {
            return;
        }
        let Some(Prompt { state, .. }) = self.prompt.take() else {
            return;
        };
        if matches!(response, PromptAnswer::Path(Some(_)))
            && (self.files.slot.is_some() || self.pending.is_some())
        {
            self.files.status = FileStatus::Refused(Refusal::FileBusy);
            self.deny(matches!(state, PromptState::Unsaved(DocumentAction::Close)));
            self.changed();
            return;
        }
        match (state, response) {
            (PromptState::Purge { key, expect }, PromptAnswer::Confirm) => {
                let result =
                    self.mutate_library(&key.to_string(), &expect.to_string(), "purge", "", true);
                if let Err(refusal) = result {
                    self.files.status = FileStatus::Refused(refusal.clone());
                }
            }

            (PromptState::Unsaved(action), PromptAnswer::Save) => {
                let closes = matches!(action, DocumentAction::Close);
                match self.prepare_save() {
                    Ok(content) => {
                        if let Err(CommitRefusal { problems }) = self.editor.session.commit() {
                            self.files.status = FileStatus::Refused(Refusal::Input(problems));
                            self.deny(closes);
                            return;
                        }
                        self.submit_live();
                        let document = self.editor.session.document();
                        match self.start_managed_save(document, content) {
                            Ok(task) => self.schedule(Action::AfterSave {
                                action,
                                task,
                                document,
                            }),
                            Err(refusal) => {
                                self.files.status = FileStatus::Refused(refusal.clone());
                                self.deny(closes);
                            }
                        }
                    }
                    Err(refusal) => {
                        self.files.status = FileStatus::Refused(refusal.clone());
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
                if self.audio.slot.is_some() {
                    self.files.status = FileStatus::Refused(Refusal::AudioPlaying);
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
                    self.files.status = FileStatus::DialogFailed { cause: message };
                }
                let closes = matches!(state, PromptState::Unsaved(DocumentAction::Close));
                self.deny(closes);
            }
        }
        self.changed();
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
        if audio && let Some(task) = &self.audio.slot {
            task.control.request_stop();
        }
        if file
            && let Some(FileTask {
                kind: FileKind::Export { cancel, .. },
                ..
            }) = &self.files.slot
        {
            cancel.request();
        }
        let pending = Pending {
            audio: if audio {
                self.audio.slot.as_ref().map(|t| t.id)
            } else {
                None
            },
            file: if file {
                self.files.slot.as_ref().map(|t| t.id)
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
                Err(e) => self.files.status = FileStatus::Failed(e),
            },
            Action::Document(DocumentAction::Copy(snapshot)) => {
                if let Err(refusal) = Self::copy_content_admission(&snapshot) {
                    self.files.status = FileStatus::Refused(refusal.clone());
                } else {
                    match Draft::from_snapshot(&snapshot) {
                        Ok(draft) => {
                            self.replace_document(draft);
                        }
                        Err(e) => self.files.status = FileStatus::Failed(e),
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
                        .session
                        .record_saved(self.editor.session.document(), snapshot.recipe())
                {
                    self.editor.location = snapshot
                        .external_file()
                        .cloned()
                        .map_or(DocumentLocation::Unsaved, DocumentLocation::External);
                }
            }
            Action::Document(DocumentAction::EditManaged(record)) => {
                if self.replace_document(Draft::from_recipe(record.snapshot.recipe().clone()))
                    && self
                        .editor
                        .session
                        .record_saved(self.editor.session.document(), record.snapshot.recipe())
                {
                    self.editor.location = DocumentLocation::Managed {
                        key: record.key,
                        token: record.version_token,
                    };
                }
            }
            Action::Retry(action) => self.request(action),
            Action::StartAudio(request) => self.start_audio(request),
            Action::AfterSave {
                action, document, ..
            } => {
                if pending.saved == Some(true)
                    && self.editor.session.document() == document
                    && !self.editor.session.dirty()
                {
                    let action = if let DocumentAction::TrashSaved { key, expect } = action {
                        let expect = self.editor.location.saved_token(&key).unwrap_or(expect);
                        DocumentAction::TrashSaved { key, expect }
                    } else {
                        action
                    };
                    self.schedule(Action::Document(action));
                } else {
                    if pending.saved == Some(true) {
                        self.files.status = FileStatus::Notice(FileNotice::SavedSnapshotStillDirty);
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
                self.files.status = FileStatus::Failed(error);
                false
            }
        }
    }

    fn install_document(&mut self, editor: EditorSession) {
        self.editor.session = editor;
        self.editor.location = DocumentLocation::Unsaved;
        self.editor.status = EditStatus::Idle;
        self.view = View::Workbench;
        self.effects.push(Effect::ShowView(View::Workbench));
        self.files.status = FileStatus::Idle;
    }

    // ---- tasks ----

    fn start_allowed(&self) -> Result<(), Refusal> {
        if self.prompt.is_some() {
            return Err(Refusal::PromptOpen);
        }
        if self.pending.is_some() {
            return Err(Refusal::ActionPending);
        }
        Ok(())
    }

    fn start_file(&mut self, kind: FileKind) -> TaskId {
        let id = TaskId(self.next_id());
        if !kind.background_read() {
            self.files.status = FileStatus::Running(FileActivity::from_kind(&kind));
        }
        self.effects.push(Effect::Run(Job::file(id, kind.clone())));
        self.files.slot = Some(FileTask { id, kind });
        id
    }

    /// Choose a recipe file to open. Opening may run beside audio.
    ///
    /// # Errors
    /// Returns the admission refusal.
    pub fn import(&mut self) -> Result<(), Refusal> {
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

    /// Commit and capture the open document, then choose where to save it.
    /// The captured content is what is written, whatever is edited later.
    ///
    /// # Errors
    /// Returns the admission or readiness refusal.
    pub fn save(&mut self) -> Result<(), Refusal> {
        let result = self.save_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }

    fn save_inner(&mut self) -> Result<(), Refusal> {
        let content = self.prepare_save()?;
        self.editor
            .session
            .commit()
            .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
        self.submit_live();
        self.start_managed_save(self.editor.session.document(), content)?;
        Ok(())
    }

    /// Export captured portable content without saving or forking the document.
    ///
    /// # Errors
    /// Refuses occupied slots or an unserializable draft; no playback is started.
    pub fn export_recipe(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let (text, title): (Arc<str>, String) = match self.view {
            View::Workbench => {
                let recipe = self.prepare_recipe()?;
                (
                    Arc::from(
                        recipe
                            .to_toml()
                            .map_err(|e| Refusal::Document(e.to_string()))?,
                    ),
                    self.editor.session.draft().title().into(),
                )
            }
            View::Library => {
                let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
                let text = match snapshot.origin() {
                    crate::recipe::Origin::BuiltIn(id) => Arc::from(
                        crate::recipe::builtin_source(id)
                            .map_err(|e| Refusal::Document(e.to_string()))?,
                    ),
                    crate::recipe::Origin::User { .. } => {
                        self.library.selected_text().ok_or(Refusal::NoSelection)?
                    }
                    _ => Arc::from(
                        snapshot
                            .recipe()
                            .to_toml()
                            .map_err(|e| Refusal::Document(e.to_string()))?,
                    ),
                };
                (text, snapshot.title())
            }
        };
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
    pub fn export(&mut self) -> Result<(), Refusal> {
        let result = self.export_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }

    fn export_inner(&mut self) -> Result<(), Refusal> {
        let prepared = self.prepare_export()?;
        if self.view == View::Workbench {
            self.editor
                .session
                .commit()
                .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
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

    /// Cancel the running export. Audio is unaffected.
    ///
    /// # Errors
    /// Returns [`Refusal::NoExport`] when no export runs.
    pub fn cancel_export(&mut self) -> Result<(), Refusal> {
        match &self.files.slot {
            Some(FileTask {
                kind: FileKind::Export { cancel, .. },
                ..
            }) => {
                cancel.request();
                Ok(())
            }
            _ => Err(Refusal::NoExport),
        }
    }

    /// Request Stop of the current audio session. Files are unaffected, and
    /// Stop stays available while prompts are open.
    ///
    /// # Errors
    /// Returns [`Refusal::NoAudio`] when nothing plays.
    pub fn stop(&mut self) -> Result<StopRequest, Refusal> {
        let result = self
            .audio
            .slot
            .as_ref()
            .map(|task| task.control.request_stop())
            .ok_or(Refusal::NoAudio);
        self.changed();
        result
    }

    /// Play what the current view shows: the selected recipe to its end, or
    /// the open document live. Playing replaces current audio after it
    /// stops. Preparation is requested from fully parsed finite values; the
    /// definitive check happens at the device's actual rate before the
    /// stream starts, independently of the export rate.
    ///
    /// # Errors
    /// Returns the admission or input refusal.
    pub fn play(&mut self) -> Result<(), Refusal> {
        let result = self.play_inner();
        self.audio.status = match &result {
            Ok(()) => AudioStatus::Idle,
            Err(refusal) => AudioStatus::Refused(refusal.clone()),
        };
        self.changed();
        result
    }

    /// Invoke the persistent transport action for the current workspace.
    ///
    /// # Errors
    /// Returns the same admission refusal as Stop or Play.
    pub fn play_stop(&mut self) -> Result<(), Refusal> {
        if self.audio_busy() {
            self.stop().map(|_| ())
        } else {
            self.play()
        }
    }

    fn play_inner(&mut self) -> Result<(), Refusal> {
        let request = self.audio_request()?;
        if self.audio.slot.is_some() {
            self.schedule(Action::StartAudio(request));
        } else {
            self.start_audio(request);
        }
        Ok(())
    }

    fn audio_request(&mut self) -> Result<AudioRequest, Refusal> {
        let request = self.prepare_audio()?;
        if matches!(request, AudioRequest::Live { .. }) {
            self.editor
                .session
                .commit()
                .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
        }
        Ok(request)
    }

    /// Whether Play has a complete candidate and admission, before device discovery.
    pub(crate) fn play_readiness(&mut self) -> Result<(), Refusal> {
        self.prepare_audio().map(|_| ())
    }

    fn start_audio(&mut self, request: AudioRequest) {
        let session = Session::new();
        let control = session.control();
        let (kind, title, draft) = match request {
            AudioRequest::Finite { snapshot, store } => {
                let title = snapshot.title();
                let kind = match store {
                    Some(store) => AudioKind::Managed {
                        snapshot: *snapshot,
                        store,
                    },
                    None => AudioKind::Finite(*snapshot),
                };
                (kind, title, None)
            }
            AudioRequest::Live {
                document,
                candidate,
            } => (
                AudioKind::Live(document),
                candidate.draft().title().to_owned(),
                Some(candidate.draft().clone()),
            ),
        };
        let id = TaskId(self.next_id());
        self.audio.output.unbind();
        self.audio.status = AudioStatus::Idle;
        self.effects
            .push(Effect::Run(Job::audio(id, session, kind.clone(), draft)));
        self.audio.slot = Some(AudioTask {
            id,
            kind,
            title,
            control,
            live: None,
            rate: None,
            protocol_failed: false,
        });
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
        if self.audio.output.refresh(
            &mut self.editor.session,
            shown.as_ref(),
            self.view,
            self.editor.export_rate,
            self.audio.slot.is_some(),
        ) {
            self.changed();
        }
        let live_document = self.live_rate().is_some();
        let rate = match self.view {
            View::Workbench => self.diagnostic_rate(),
            View::Library => self.editor.export_rate,
        };
        if self.audio.output.refresh_model(
            &mut self.editor.session,
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
        result: Result<Result<crate::session::PlaybackReport, Error>, WorkerFailure>,
    ) {
        let failed = task.protocol_failed || !matches!(&result, Ok(Ok(_)));
        self.audio.status = match result {
            Ok(Ok(report)) if !task.protocol_failed => AudioStatus::Ended {
                report,
                monitored: self.audio.output.reader().is_some(),
            },
            Ok(Err(error)) => AudioStatus::Failed(error),
            Err(error) => AudioStatus::WorkerFailed(error),
            Ok(Ok(_)) => AudioStatus::WorkerFailed(WorkerFailure::InvalidOutcome),
        };
        if failed {
            self.audio.output.mark_failed(&task.title);
        }
        self.conclude(
            task.id,
            true,
            if failed {
                Conclusion::Failed
            } else {
                Conclusion::Succeeded
            },
            None,
        );
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
                    self.files.status = status;
                }
                self.conclude(task.id, false, Conclusion::Failed, Some(false));
                self.changed();
                return;
            }
        };
        let conclusion = outcome.conclusion();
        let mut saved = None;
        let status = match (&task.kind, outcome) {
            (FileKind::Managed(job), FileOutcome::Managed(result)) => {
                let (status, confirmed) = self.finish_managed(job, result);
                saved = confirmed;
                status
            }
            (_, FileOutcome::Imported(Ok(snapshot))) => {
                self.add_import(snapshot.clone());
                FileStatus::Imported {
                    snapshot,
                    hidden: self.selected().is_none(),
                }
            }
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
            (_, FileOutcome::Exported(Err(error))) if error.kind() == ErrorKind::Cancelled => {
                FileStatus::Notice(FileNotice::ExportCancelled)
            }
            (
                _,
                FileOutcome::Imported(Err(error))
                | FileOutcome::RecipeExported(Err(error))
                | FileOutcome::Exported(Err(error)),
            ) => FileStatus::Failed(error),
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
            self.files.status = status;
        }
        self.conclude(task.id, false, conclusion, saved);
        self.changed();
    }

    fn add_import(&mut self, snapshot: RecipeSnapshot) {
        self.register_external(&snapshot);
        if let Some(index) = self
            .library
            .catalog
            .iter()
            .position(|existing| existing.origin() == snapshot.origin())
        {
            self.library.catalog[index] = snapshot;
        } else {
            self.library.catalog.push(snapshot);
        }
        self.library.query.search.clear();
        self.refilter();
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
        self.prompt = None;
        self.closing = false;
        if let Some(task) = &self.audio.slot {
            task.control.request_stop();
        }
        if let Some(FileTask {
            kind: FileKind::Export { cancel, .. },
            ..
        }) = &self.files.slot
        {
            cancel.request();
        }
    }
    fn job_context(&self, job: TaskId) -> String {
        if let Some(task) = self.files.slot.as_ref().filter(|task| task.id == job) {
            return match &task.kind {
                FileKind::Managed(job) => format!(" · {}", job.context().description()),
                FileKind::Import(path)
                | FileKind::ExportRecipe { path, .. }
                | FileKind::Export { path, .. } => format!(" · {}", path.display()),
            };
        }
        self.audio
            .slot
            .as_ref()
            .filter(|task| task.id == job)
            .map_or(String::new(), |task| format!(" · {}", task.title))
    }

    // ---- display policy ----

    /// Record the platform's reduced-motion preference.
    pub fn set_motion_preference(&mut self, preference: MotionPreference) {
        if self.audio.output.set_motion(preference) {
            self.changed();
        }
    }

    /// Hold or release the displayed capture, overriding the platform
    /// preference until [`Workspace::follow_motion`].
    pub fn set_hold(&mut self, held: bool) {
        self.audio.output.set_override(Some(held));
        self.changed();
    }

    /// Follow the platform's motion preference again.
    pub fn follow_motion(&mut self) {
        self.audio.output.set_override(None);
        self.changed();
    }

    /// Choose the waveform span in milliseconds and its display scale.
    pub fn set_waveform(&mut self, span_ms: u32, scale: WaveformScale) {
        if self.audio.output.set_waveform(span_ms, scale) {
            self.changed();
        }
    }

    /// Stop showing the last session's output and preview the current source.
    pub fn preview_current(&mut self) {
        if self.audio.slot.is_none() {
            self.audio.output.unbind();
            self.changed();
        }
    }

    /// The timing model of what the current view explains: the open
    /// document's accepted live revision, its committed content, or the
    /// selected recipe. `None` when that source is not valid.
    #[must_use]
    pub fn model_view(&self) -> Option<Arc<crate::waveform_view::ModelView>> {
        self.audio.output.model().cloned()
    }

    /// Stop control of the current audio session, for a native control that
    /// must stop exactly that session.
    #[must_use]
    pub fn audio_control(&self) -> Option<PlaybackControl> {
        self.audio.slot.as_ref().map(|task| task.control.clone())
    }

    /// Phase of the current audio session, if one is bound.
    #[must_use]
    pub fn audio_phase(&self) -> Option<PlaybackPhase> {
        self.audio.slot.as_ref().map(|t| t.control.phase())
    }
}

#[cfg(test)]
mod tests;
