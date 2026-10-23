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
mod output;
mod tasks;
mod views;

pub use output::{MotionPreference, OutputView};
pub use tasks::TaskId;
pub use views::{
    ActionView, ComponentView, EditorView, LibraryView, MetadataView, ParameterView, TransportView,
};

use crate::editing::{
    CommitRefusal, EditRefusal, EditingField, FieldHandle, InputProblem, SaveRefusal,
};
use crate::session::{PlaybackControl, PlaybackPhase, Session, StopRequest};
use crate::{
    Cancel, DocumentId, Draft, EditorSession, Error, ErrorKind, Progress, RecipeSnapshot, Sound,
    WaveformScale, controls, presentation,
};
use auditorylab_core::{
    CarrierKind, ComponentId, Ears, ModulationKind, Recipe, SampleRate, Stimulus,
};
use output::OutputState;
use std::path::PathBuf;
use std::sync::Arc;
use tasks::{
    AudioEvent, AudioKind, AudioTask, Conclusion, FileKind, FileOutcome, FileTask, Polled,
};

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
    /// Whether default-device playback was built in and is offered.
    pub device_playback: bool,
}

impl Capabilities {
    /// Everything this build supports.
    #[must_use]
    pub const fn available() -> Self {
        Self {
            device_playback: cfg!(feature = "device-playback"),
        }
    }

    /// No device playback, for portable consumers.
    #[must_use]
    pub const fn portable() -> Self {
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
    /// A destination for the open document; replacement follows the panel's
    /// own overwrite confirmation.
    SaveRecipe,
    /// A destination for exported audio; replacement follows the panel's
    /// own overwrite confirmation.
    ExportWav,
}

/// Something the frontend must do on the workspace's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Ask whether to save, discard or keep unsaved changes.
    AskUnsaved(PromptId),
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
pub enum Response {
    /// Save, then continue.
    Save,
    /// Continue without saving.
    Discard,
    /// Keep everything as it is.
    Cancel,
    /// The chosen path, or `None` when the panel was cancelled.
    Path(Option<PathBuf>),
    /// The prompt could not be shown.
    Failed(String),
}

/// Why a command was not carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// This build or workspace does not offer device playback.
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
    Document(String),
    /// The source is not valid at the export rate.
    Invalid(String),
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
    Close,
}

#[derive(Debug, Clone)]
enum AudioRequest {
    Finite(RecipeSnapshot),
    Live { document: DocumentId, draft: Draft },
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
    SaveBeforeAction {
        action: DocumentAction,
        document: DocumentId,
        content: Recipe,
    },
    Save {
        document: DocumentId,
        content: Recipe,
    },
    Import,
    Export(Sound),
}

#[derive(Debug)]
struct Prompt {
    id: PromptId,
    state: PromptState,
}

/// Interactive application state for one window of work.
pub struct Workspace {
    capabilities: Capabilities,
    catalog: Vec<RecipeSnapshot>,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
    view: View,
    editor: EditorSession,
    export_rate: SampleRate,
    audio: Option<AudioTask>,
    file: Option<FileTask>,
    prompt: Option<Prompt>,
    pending: Option<Pending>,
    closing: bool,
    effects: Vec<Effect>,
    next_id: u64,
    file_status: String,
    edit_status: String,
    audio_status: String,
    output: OutputState,
    revision: u64,
    #[cfg(test)]
    audio_spawner: Option<Box<tests::AudioSpawner>>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("view", &self.view)
            .field("document", &self.editor.document())
            .field("audio", &self.audio.as_ref().map(|t| t.id))
            .field("file", &self.file.as_ref().map(|t| t.id))
            .finish_non_exhaustive()
    }
}

impl Workspace {
    /// A workspace over a library, with a new blank document.
    ///
    /// # Errors
    /// Returns an error only when document identities are exhausted.
    pub fn new(catalog: Vec<RecipeSnapshot>, capabilities: Capabilities) -> Result<Self, Error> {
        let filtered = (0..catalog.len()).collect();
        Ok(Self {
            capabilities: Capabilities {
                device_playback: capabilities.device_playback
                    && Capabilities::available().device_playback,
            },
            catalog,
            query: String::new(),
            filtered,
            selected: 0,
            view: View::Library,
            editor: EditorSession::new(Draft::new())?,
            export_rate: SampleRate::DEFAULT,
            audio: None,
            file: None,
            prompt: None,
            pending: None,
            closing: false,
            effects: Vec::new(),
            next_id: 1,
            file_status: String::new(),
            edit_status: String::new(),
            audio_status: String::new(),
            output: OutputState::default(),
            revision: 0,
            #[cfg(test)]
            audio_spawner: None,
        })
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
        self.capabilities
    }

    /// The open document's editor.
    #[must_use]
    pub const fn editor(&self) -> &EditorSession {
        &self.editor
    }

    /// The current view.
    #[must_use]
    pub const fn view(&self) -> View {
        self.view
    }

    /// The selected inspection and export rate; device rates are separate.
    #[must_use]
    pub const fn export_rate(&self) -> SampleRate {
        self.export_rate
    }

    /// The library in natural order.
    #[must_use]
    pub fn catalog(&self) -> &[RecipeSnapshot] {
        &self.catalog
    }

    /// The selected recipe, when the search shows it. A search that hides
    /// every recipe leaves nothing selected.
    #[must_use]
    pub fn selected(&self) -> Option<&RecipeSnapshot> {
        self.selected_index().map(|index| &self.catalog[index])
    }

    fn selected_index(&self) -> Option<usize> {
        self.filtered
            .contains(&self.selected)
            .then_some(self.selected)
            .filter(|&index| index < self.catalog.len())
    }

    /// Positions of the library entries matching the query, in order.
    #[must_use]
    pub fn filtered(&self) -> &[usize] {
        &self.filtered
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
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
        self.audio.is_some()
    }

    /// Whether a file task occupies its slot.
    #[must_use]
    pub const fn file_busy(&self) -> bool {
        self.file.is_some()
    }

    /// Whether the running file task is an export.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.file
            .as_ref()
            .is_some_and(|task| matches!(task.kind, FileKind::Export { .. }))
    }

    // ---- library ----

    fn refilter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.filtered = (0..self.catalog.len())
            .filter(|&i| matches(&self.catalog[i], &query))
            .collect();
    }

    /// Search titles, IDs, research text and component meaning. When the
    /// selection stops matching, the first match is selected.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        self.refilter();
        if !self.filtered.contains(&self.selected)
            && let Some(&first) = self.filtered.first()
        {
            self.selected = first;
        }
        self.changed();
    }

    /// Select the entry at a position among the filtered entries.
    pub fn select(&mut self, position: usize) {
        if let Some(&index) = self.filtered.get(position) {
            self.selected = index;
            self.changed();
        }
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
        if self.export_rate != rate {
            self.export_rate = rate;
            self.changed();
        }
    }

    /// Show the recipe the open document was copied from, by its retained
    /// snapshot rather than by its path.
    pub fn show_based_on_source(&mut self) {
        let Some(source) = self.editor.draft().based_on().cloned() else {
            return;
        };
        let index = self
            .catalog
            .iter()
            .position(|s| s.id() == source.id())
            .unwrap_or_else(|| {
                self.catalog.push(source);
                self.catalog.len() - 1
            });
        self.query.clear();
        self.refilter();
        self.selected = index;
        self.view = View::Library;
        self.effects.push(Effect::ShowView(View::Library));
        self.changed();
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
            .file
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
        self.edit_status = match &result {
            Ok(()) => {
                self.submit_live();
                String::new()
            }
            Err(Refusal::Input(_)) => String::new(),
            Err(refusal) => views::refusal_text(refusal),
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
        let result = self.editor.stage(handle, text).map_err(Refusal::Edit);
        self.changed();
        result
    }

    /// Discard a field's staged text; returns its committed text.
    ///
    /// # Errors
    /// Returns the refusal for stale handles.
    pub fn restore(&mut self, handle: FieldHandle) -> Result<String, Refusal> {
        let result = self.editor.restore(handle).map_err(Refusal::Edit);
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
        let result = match self.editor.commit_field(handle, text) {
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
    pub fn set_ears(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        ears: Ears,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .set_ears(document, id, ears)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Change a component's carrier kind.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_carrier(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        kind: CarrierKind,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .set_carrier(document, id, kind)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Change a component's modulation kind, or remove its modulation.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn set_modulation(
        &mut self,
        document: DocumentId,
        id: ComponentId,
        kind: Option<ModulationKind>,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let rate = self.diagnostic_rate();
        let result = self
            .editor
            .set_modulation(document, id, kind, rate)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Add a default component.
    ///
    /// # Errors
    /// Returns the refusal at the component limit or while editing is blocked.
    pub fn add_component(
        &mut self,
        document: DocumentId,
        kind: CarrierKind,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .add_component(document, kind)
            .map(|_| ())
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Remove a component.
    ///
    /// # Errors
    /// Returns the refusal for stale identities or while editing is blocked.
    pub fn remove_component(
        &mut self,
        document: DocumentId,
        id: ComponentId,
    ) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .remove_component(document, id)
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// Put the most recently removed component back where it was.
    ///
    /// # Errors
    /// Returns the refusal when nothing was removed, at the component limit
    /// or while editing is blocked.
    pub fn restore_component(&mut self, document: DocumentId) -> Result<(), Refusal> {
        self.editing_allowed()?;
        let result = self
            .editor
            .restore_removed(document)
            .map(|_| ())
            .map_err(Refusal::Edit);
        self.after_edit(result)
    }

    /// The live session's actual rate, when a live task is bound to the open
    /// document; otherwise the selected export rate. Field diagnostics use it.
    #[must_use]
    pub fn diagnostic_rate(&self) -> SampleRate {
        self.live_rate().unwrap_or(self.export_rate)
    }

    /// The bound live session's actual device rate, if any.
    #[must_use]
    pub fn live_rate(&self) -> Option<SampleRate> {
        self.audio
            .as_ref()
            .filter(|t| matches!(t.kind, AudioKind::Live(document) if document == self.editor.document()))
            .and_then(|t| t.rate)
    }

    /// Send the committed document to a bound live session, compiled at the
    /// session's actual rate. Invalid committed values keep the last valid
    /// accepted target; staged text is never sent.
    fn submit_live(&mut self) {
        let Some(task) = self.audio.as_ref() else {
            return;
        };
        let (AudioKind::Live(document), Some(live), Some(rate)) =
            (&task.kind, &task.live, task.rate)
        else {
            return;
        };
        if *document != self.editor.document() {
            return;
        }
        let live = live.clone();
        let analysis = self.editor.committed_analysis(rate);
        if let Some(target) = analysis.target()
            && let Err(error) = live.update(target)
            && error != crate::LiveUpdateError::SessionClosed
        {
            self.audio_status = error.to_string();
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
            self.request(DocumentAction::Copy(snapshot));
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
            self.file_status = views::refusal_text(if self.prompt.is_some() {
                &Refusal::PromptOpen
            } else {
                &Refusal::ActionPending
            });
            self.deny(closes);
            self.changed();
            return;
        }
        if self.file.is_some() {
            self.schedule(Action::Retry(action));
            self.changed();
            return;
        }
        // Commit staged text when it can be; anything left staged is unsaved.
        let _ = self.editor.commit();
        self.submit_live();
        if self.editor.dirty() {
            let id = self.prompt(PromptState::Unsaved(action));
            self.effects.push(Effect::AskUnsaved(id));
        } else {
            self.schedule(Action::Document(action));
        }
        self.changed();
    }

    /// Answer a prompt. Answers to prompts that are no longer open change nothing.
    pub fn respond(&mut self, prompt: PromptId, response: Response) {
        if self.prompt.as_ref().is_none_or(|p| p.id != prompt) {
            return;
        }
        let Some(Prompt { state, .. }) = self.prompt.take() else {
            return;
        };
        match (state, response) {
            (PromptState::Unsaved(action), Response::Save) => {
                let closes = matches!(action, DocumentAction::Close);
                match self.editor.save_readiness() {
                    Ok(()) => {
                        let document = self.editor.document();
                        let content = self.editor.draft().recipe();
                        let id = self.prompt(PromptState::SaveBeforeAction {
                            action,
                            document,
                            content,
                        });
                        self.effects.push(Effect::ChoosePath {
                            prompt: id,
                            purpose: PathPurpose::SaveRecipe,
                            suggested_name: Some(file_name(self.editor.draft().title(), "toml")),
                        });
                    }
                    Err(refusal) => {
                        self.file_status = views::save_refusal_text(&refusal);
                        self.deny(closes);
                    }
                }
            }
            (PromptState::Unsaved(action), Response::Discard) => {
                self.schedule(Action::Document(action));
            }
            (
                PromptState::SaveBeforeAction {
                    action,
                    document,
                    content,
                },
                Response::Path(Some(path)),
            ) => {
                let task = self.start_file(
                    FileKind::Save {
                        document,
                        content,
                        path,
                    },
                    None,
                );
                self.schedule(Action::AfterSave {
                    action,
                    task,
                    document,
                });
            }
            (PromptState::Save { document, content }, Response::Path(Some(path))) => {
                self.start_file(
                    FileKind::Save {
                        document,
                        content,
                        path,
                    },
                    None,
                );
            }
            (PromptState::Import, Response::Path(Some(path))) => {
                self.start_file(FileKind::Import(path), None);
            }
            (PromptState::Export(sound), Response::Path(Some(path))) => {
                if self.audio.is_some() {
                    self.file_status = views::refusal_text(&Refusal::AudioPlaying);
                } else {
                    let frames = sound.signal().frames();
                    let title = sound.snapshot().title();
                    self.start_file(
                        FileKind::Export {
                            title,
                            path,
                            frames,
                            cancel: Cancel::default(),
                            progress: Progress::default(),
                        },
                        Some(sound),
                    );
                }
            }
            (state, response) => {
                if let Response::Failed(message) = response {
                    self.file_status = message;
                }
                let closes = matches!(
                    state,
                    PromptState::Unsaved(DocumentAction::Close)
                        | PromptState::SaveBeforeAction {
                            action: DocumentAction::Close,
                            ..
                        }
                );
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
        if audio && let Some(task) = &self.audio {
            task.control.request_stop();
        }
        if file
            && let Some(FileTask {
                kind: FileKind::Export { cancel, .. },
                ..
            }) = &self.file
        {
            cancel.request();
        }
        let pending = Pending {
            audio: if audio {
                self.audio.as_ref().map(|t| t.id)
            } else {
                None
            },
            file: if file {
                self.file.as_ref().map(|t| t.id)
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
            Action::Document(DocumentAction::New) => self.replace_document(Draft::new()),
            Action::Document(DocumentAction::Copy(snapshot)) => {
                let count = snapshot.stimulus().components.len();
                if count > Stimulus::MAX_COMPONENTS {
                    self.file_status = views::refusal_text(&Refusal::TooManyComponents(count));
                } else {
                    self.replace_document(Draft::from_snapshot(&snapshot));
                }
            }
            Action::Retry(action) => self.request(action),
            Action::StartAudio(request) => self.start_audio(request),
            Action::AfterSave {
                action, document, ..
            } => {
                if pending.saved == Some(true)
                    && self.editor.document() == document
                    && !self.editor.dirty()
                {
                    self.schedule(Action::Document(action));
                } else {
                    if pending.saved == Some(true) {
                        self.file_status = presentation::SAVED_SNAPSHOT_STILL_DIRTY.into();
                    }
                    self.deny(matches!(action, DocumentAction::Close));
                }
            }
        }
    }

    fn replace_document(&mut self, draft: Draft) {
        match EditorSession::new(draft) {
            Ok(editor) => {
                self.editor = editor;
                self.view = View::Workbench;
                self.effects.push(Effect::ShowView(View::Workbench));
                self.file_status.clear();
            }
            Err(error) => self.file_status = error.to_string(),
        }
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

    fn start_file(&mut self, kind: FileKind, export: Option<Sound>) -> TaskId {
        let id = TaskId(self.next_id());
        self.file_status = views::file_started_text(&kind);
        self.file = Some(FileTask::start(id, kind, export));
        id
    }

    /// Choose a recipe file to open. Opening may run beside audio.
    ///
    /// # Errors
    /// Returns the admission refusal.
    pub fn import(&mut self) -> Result<(), Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
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
            self.file_status = views::refusal_text(refusal);
        }
        self.changed();
        result
    }

    fn save_inner(&mut self) -> Result<(), Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        self.editor
            .commit()
            .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
        self.submit_live();
        self.editor
            .save_readiness()
            .map_err(|refusal| match refusal {
                SaveRefusal::Input(problems) => Refusal::Input(problems),
                SaveRefusal::Document(message) => Refusal::Document(message),
            })?;
        let document = self.editor.document();
        let content = self.editor.draft().recipe();
        let id = self.prompt(PromptState::Save { document, content });
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::SaveRecipe,
            suggested_name: Some(file_name(self.editor.draft().title(), "toml")),
        });
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
            self.file_status = views::refusal_text(refusal);
        }
        self.changed();
        result
    }

    fn export_inner(&mut self) -> Result<(), Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        if self.audio.is_some() {
            return Err(Refusal::AudioPlaying);
        }
        let rate = self.export_rate;
        let (sound, name) = match self.view {
            View::Library => {
                let snapshot = self.selected().cloned().ok_or(Refusal::NoSelection)?;
                let name = match snapshot.origin() {
                    crate::Origin::BuiltIn(id) => format!("{id}.wav"),
                    _ => file_name(&snapshot.title(), "wav"),
                };
                (
                    snapshot
                        .at(rate)
                        .map_err(|e| Refusal::Invalid(e.to_string()))?,
                    name,
                )
            }
            View::Workbench => {
                self.editor
                    .commit()
                    .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
                self.submit_live();
                let analysis = self.editor.committed_analysis(rate);
                let signal = analysis.signal().cloned().ok_or_else(|| {
                    Refusal::Invalid(analysis.finite_error().map_or_else(String::new, |e| {
                        presentation::invalid_at_rate(self.editor.draft().title(), rate, e)
                    }))
                })?;
                let snapshot = self.editor.draft().snapshot();
                let name = file_name(self.editor.draft().title(), "wav");
                (Sound::from_parts(snapshot, signal), name)
            }
        };
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
        match &self.file {
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
        self.audio_status = match &result {
            Ok(()) => String::new(),
            Err(refusal) => views::refusal_text(refusal),
        };
        self.changed();
        result
    }

    fn play_inner(&mut self) -> Result<(), Refusal> {
        let request = self.audio_request()?;
        if self.audio.is_some() {
            self.schedule(Action::StartAudio(request));
        } else {
            self.start_audio(request);
        }
        Ok(())
    }

    fn audio_request(&mut self) -> Result<AudioRequest, Refusal> {
        if !self.capabilities.device_playback {
            return Err(Refusal::DevicePlaybackUnavailable);
        }
        self.start_allowed()?;
        if self.exporting() {
            return Err(Refusal::Exporting);
        }
        match self.view {
            View::Library => {
                let snapshot = self.selected().cloned().ok_or(Refusal::NoSelection)?;
                if snapshot.stimulus().nonfinite().next().is_some() {
                    return Err(Refusal::NotFinite);
                }
                Ok(AudioRequest::Finite(snapshot))
            }
            View::Workbench => {
                self.editor
                    .commit()
                    .map_err(|CommitRefusal { problems }| Refusal::Input(problems))?;
                Ok(AudioRequest::Live {
                    document: self.editor.document(),
                    draft: self.editor.draft().clone(),
                })
            }
        }
    }

    /// Whether Play would be admitted now, before any device rate is known.
    pub(crate) fn play_readiness(&mut self) -> Result<(), Refusal> {
        if !self.capabilities.device_playback {
            return Err(Refusal::DevicePlaybackUnavailable);
        }
        self.start_allowed()?;
        if self.exporting() {
            return Err(Refusal::Exporting);
        }
        match self.view {
            View::Library => match self.selected() {
                None => Err(Refusal::NoSelection),
                Some(s) if s.stimulus().nonfinite().next().is_some() => Err(Refusal::NotFinite),
                Some(_) => Ok(()),
            },
            View::Workbench => {
                let candidate = self.editor.candidate();
                if candidate.is_complete() {
                    Ok(())
                } else {
                    Err(Refusal::Input(candidate.problems().to_vec()))
                }
            }
        }
    }

    fn start_audio(&mut self, request: AudioRequest) {
        let session = Session::new();
        let control = session.control();
        let (kind, title, draft) = match request {
            AudioRequest::Finite(snapshot) => {
                let title = snapshot.title();
                (AudioKind::Finite(snapshot), title, None)
            }
            AudioRequest::Live { document, draft } => (
                AudioKind::Live(document),
                draft.title().to_owned(),
                Some(draft),
            ),
        };
        let Some(worker) = self.spawn_audio(session, kind.clone(), draft) else {
            self.audio_status = views::refusal_text(&Refusal::DevicePlaybackUnavailable);
            return;
        };
        let id = TaskId(self.next_id());
        self.output.unbind();
        self.audio_status.clear();
        self.audio = Some(AudioTask {
            id,
            kind,
            title,
            control,
            live: None,
            rate: None,
            worker,
        });
    }

    #[cfg_attr(
        not(any(feature = "device-playback", test)),
        expect(
            clippy::unused_self,
            reason = "Without device playback no worker can start"
        )
    )]
    fn spawn_audio(
        &mut self,
        session: Session,
        kind: AudioKind,
        draft: Option<Draft>,
    ) -> Option<tasks::Worker<AudioEvent, Result<crate::session::PlaybackReport, Error>>> {
        #[cfg(test)]
        if let Some(spawn) = self.audio_spawner.as_mut() {
            return Some(spawn(session, kind, draft));
        }
        #[cfg(feature = "device-playback")]
        {
            Some(tasks::audio_worker(session, kind, draft))
        }
        #[cfg(not(feature = "device-playback"))]
        {
            drop((session, kind, draft));
            None
        }
    }

    // ---- polling ----

    /// Receive task events, join finished workers, run queued actions and
    /// refresh the output view. Nonblocking. Returns whether anything a view
    /// shows may have changed.
    pub fn poll(&mut self) -> bool {
        let before = self.revision;
        self.poll_audio();
        self.poll_file();
        self.resolve_pending();
        let shown = self.selected_index();
        if self.output.refresh(
            &mut self.editor,
            &self.catalog,
            shown,
            self.view,
            self.export_rate,
            self.audio.is_some(),
        ) {
            self.changed();
        }
        let live_document = self.live_rate().is_some();
        let rate = match self.view {
            View::Workbench => self.diagnostic_rate(),
            View::Library => self.export_rate,
        };
        if self.output.refresh_model(
            &mut self.editor,
            shown.map(|index| &self.catalog[index]),
            self.view,
            live_document,
            rate,
        ) {
            self.changed();
        }
        before != self.revision
    }

    fn poll_audio(&mut self) {
        let Some(task) = self.audio.as_mut() else {
            return;
        };
        let mut prepared = None;
        let polled = task.worker.poll(|event| match event {
            AudioEvent::Prepared { reader, rate, live } => {
                prepared = Some((reader, rate, live));
                None
            }
            AudioEvent::Finished(result) => Some(result),
        });
        if let Some((reader, rate, live)) = prepared {
            task.rate = Some(rate);
            task.live = live;
            self.output.bind(reader);
            self.submit_live();
            self.changed();
        }
        if let Polled::Joined { result, panicked } = polled {
            let Some(task) = self.audio.take() else {
                return;
            };
            let conclusion = match (&result, panicked) {
                (Some(Ok(_)), false) => Conclusion::Succeeded,
                _ => Conclusion::Failed,
            };
            self.audio_status = match (result, panicked) {
                (_, true) => presentation::WORKER_FAILED.into(),
                (None, false) => presentation::WORKER_ENDED_WITHOUT_RESULT.into(),
                // A bound monitor already shows how the session ended.
                (Some(Ok(report)), false) => {
                    views::playback_text(&report, self.output.reader().is_some())
                }
                (Some(Err(error)), false) => error.to_string(),
            };
            if conclusion == Conclusion::Failed {
                self.output.mark_failed(&task.title);
            }
            self.conclude(task.id, true, conclusion, None);
            self.changed();
        }
    }

    fn poll_file(&mut self) {
        let Some(task) = self.file.as_mut() else {
            return;
        };
        let polled = task.worker.poll(Some);
        if task.worker.finishing() {
            // Publication truth is recorded as soon as it is known.
            self.changed();
        }
        let Polled::Joined { result, panicked } = polled else {
            return;
        };
        let Some(task) = self.file.take() else {
            return;
        };
        let mut saved = None;
        let conclusion = match result {
            None => {
                self.file_status = presentation::WORKER_ENDED_WITHOUT_RESULT.into();
                Conclusion::Failed
            }
            Some(outcome) => {
                let conclusion = outcome.conclusion();
                self.file_status = match (&task.kind, outcome) {
                    (_, FileOutcome::Imported(Ok(snapshot))) => {
                        let text = views::imported_text(&snapshot);
                        self.add_import(snapshot);
                        text
                    }
                    (
                        FileKind::Save {
                            document,
                            content,
                            path,
                        },
                        FileOutcome::Saved(Ok(())),
                    ) => {
                        let same = self.editor.record_saved(*document, content);
                        saved = Some(true);
                        views::saved_text(path, content, same, self.export_rate)
                    }
                    (FileKind::Export { path, .. }, FileOutcome::Exported(Ok(exported))) => {
                        views::exported_text(path, &exported)
                    }
                    (_, FileOutcome::Exported(Err(e))) if e.kind() == ErrorKind::Cancelled => {
                        presentation::EXPORT_CANCELLED.into()
                    }
                    (
                        _,
                        FileOutcome::Saved(Err(e))
                        | FileOutcome::Imported(Err(e))
                        | FileOutcome::Exported(Err(e)),
                    ) => {
                        if matches!(task.kind, FileKind::Save { .. }) {
                            saved = Some(false);
                        }
                        e.to_string()
                    }
                    (_, _) => String::new(),
                };
                conclusion
            }
        };
        let conclusion = if panicked {
            self.file_status = format!("{}\n{}", self.file_status, presentation::WORKER_FAILED)
                .trim()
                .to_owned();
            saved = saved.map(|_| false);
            Conclusion::Failed
        } else {
            conclusion
        };
        self.conclude(task.id, false, conclusion, saved);
        self.changed();
    }

    fn add_import(&mut self, snapshot: RecipeSnapshot) {
        let index = if let Some(index) = self
            .catalog
            .iter()
            .position(|p| p.origin() == snapshot.origin())
        {
            // Same path, new content and identity.
            self.catalog[index] = snapshot;
            index
        } else {
            self.catalog.push(snapshot);
            self.catalog.len() - 1
        };
        self.query.clear();
        self.refilter();
        self.selected = index;
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
    pub fn shutdown(mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Some(task) = self.audio.take() {
            task.control.request_stop();
            let (result, panicked) = task.worker.wait(|event| match event {
                AudioEvent::Finished(result) => Some(result),
                AudioEvent::Prepared { .. } => None,
            });
            match (result, panicked) {
                (_, true) => errors.push(presentation::WORKER_FAILED.to_owned()),
                (Some(Err(error)), _) => errors.push(error.to_string()),
                (None, _) => errors.push(presentation::WORKER_ENDED_WITHOUT_RESULT.to_owned()),
                (Some(Ok(_)), _) => {}
            }
        }
        if let Some(task) = self.file.take() {
            if let FileKind::Export { cancel, .. } = &task.kind {
                cancel.request();
            }
            let (result, panicked) = task.worker.wait(Some);
            if panicked {
                errors.push(presentation::WORKER_FAILED.to_owned());
            }
            match result {
                Some(outcome) if outcome.conclusion() == Conclusion::Failed => {
                    if let FileOutcome::Imported(Err(e))
                    | FileOutcome::Saved(Err(e))
                    | FileOutcome::Exported(Err(e)) = outcome
                    {
                        errors.push(e.to_string());
                    }
                }
                None => errors.push(presentation::WORKER_ENDED_WITHOUT_RESULT.to_owned()),
                Some(_) => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    // ---- display policy ----

    /// Record the platform's reduced-motion preference.
    pub fn set_motion_preference(&mut self, preference: MotionPreference) {
        if self.output.set_motion(preference) {
            self.changed();
        }
    }

    /// Hold or release the displayed capture, overriding the platform
    /// preference until [`Workspace::follow_motion`].
    pub fn set_hold(&mut self, held: bool) {
        self.output.set_override(Some(held));
        self.changed();
    }

    /// Follow the platform's motion preference again.
    pub fn follow_motion(&mut self) {
        self.output.set_override(None);
        self.changed();
    }

    /// Choose the waveform span in milliseconds and its display scale.
    pub fn set_waveform(&mut self, span_ms: u32, scale: WaveformScale) {
        if self.output.set_waveform(span_ms, scale) {
            self.changed();
        }
    }

    /// Stop showing the last session's output and preview the current source.
    pub fn preview_current(&mut self) {
        if self.audio.is_none() {
            self.output.unbind();
            self.changed();
        }
    }

    /// The timing model of what the current view explains: the open
    /// document's accepted live revision, its committed content, or the
    /// selected recipe. `None` when that source is not valid.
    #[must_use]
    pub fn model_view(&self) -> Option<Arc<crate::ModelView>> {
        self.output.model().cloned()
    }

    /// Stop control of the current audio session, for a native control that
    /// must stop exactly that session.
    #[must_use]
    pub fn audio_control(&self) -> Option<PlaybackControl> {
        self.audio.as_ref().map(|task| task.control.clone())
    }

    /// Phase of the current audio session, if one is bound.
    #[must_use]
    pub fn audio_phase(&self) -> Option<PlaybackPhase> {
        self.audio.as_ref().map(|t| t.control.phase())
    }
}

/// A file name suggested from a title: characters that file systems reject
/// become spaces, and the name is kept short.
fn file_name(title: &str, extension: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut stem: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some((cut, _)) = stem.char_indices().nth(80) {
        stem.truncate(cut);
    }
    let stem = stem.trim_matches(|c: char| c == '.' || c.is_whitespace());
    format!(
        "{}.{extension}",
        if stem.is_empty() { "sound" } else { stem }
    )
}

/// Search identity, title, research text and component meaning together.
fn matches(snapshot: &RecipeSnapshot, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let text = format!(
        "{} {} {} {}",
        snapshot.origin(),
        snapshot.title(),
        presentation::research(snapshot)
            .iter()
            .map(|(_, v)| *v)
            .collect::<Vec<_>>()
            .join(" "),
        snapshot
            .stimulus()
            .components
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    text.to_lowercase().contains(query)
}

#[cfg(test)]
mod tests;
