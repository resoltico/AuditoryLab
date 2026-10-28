//! Immutable job captures and the application-owned thread runner.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{Response, Workspace};
use crate::audio::session::{LiveControl, PlaybackControl, PlaybackReport, Session};
use crate::document::editing::{DocumentId, Draft};
use crate::document::recipe::{RecipeSnapshot, Rendition};
use crate::job::{Cancel, Progress, Replace};
use crate::storage::wav::Exported;
use auditorylab_core::SampleRate;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};
use {
    crate::audio::PlaybackError, crate::services::ExportError, crate::storage::error::SaveError,
    crate::storage::load::LoadError,
};
use {crate::outcome::Outcome, crate::services::OperationError as Error};

/// Process-local identity of an admitted job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub(crate) u64);
/// Which execution slot a captured job uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// Filesystem work.
    File,
    /// Audio preparation and playback.
    Audio,
}
/// Dependency result of one completed job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conclusion {
    Succeeded,
    ExpectedCleanup,
    Failed,
}
/// What an audio task plays.
#[derive(Debug, Clone)]
pub(crate) enum AudioKind {
    /// A finite snapshot, validated at the device rate during preparation.
    Finite(RecipeSnapshot),
    /// Revalidate a managed selection on the worker before device preparation.
    Managed {
        snapshot: RecipeSnapshot,
        store: crate::services::library::Library,
    },
    /// A document's committed components, live until stopped.
    Live(DocumentId),
}

/// Metadata of the occupied audio slot; no thread is owned by the coordinator.
pub(crate) struct AudioTask {
    pub(crate) id: TaskId,
    pub(crate) kind: AudioKind,
    pub(crate) title: String,
    pub(crate) control: PlaybackControl,
    pub(crate) live: Option<LiveControl>,
    pub(crate) rate: Option<SampleRate>,
    pub(crate) protocol_failed: bool,
}
/// What a file task does.
#[derive(Debug, Clone)]
pub(crate) enum FileKind {
    Import(PathBuf),
    Managed(super::library::LibraryJob),
    ExportRecipe {
        text: std::sync::Arc<str>,
        path: PathBuf,
    },
    Export {
        title: String,
        path: PathBuf,
        frames: u64,
        cancel: Cancel,
        progress: Progress,
        sound: Rendition,
    },
}

/// A file task's typed result.
pub enum FileOutcome {
    /// Captured external recipe load.
    Imported(Result<RecipeSnapshot, LoadError>),
    /// Managed library operation.
    Managed(Result<super::library::LibraryOutcome, Error>),
    /// Published portable bytes.
    RecipeExported(Result<crate::services::SaveOutcome, SaveError>),
    /// Published audio export.
    Exported(Result<Exported, ExportError>),
}

impl FileOutcome {
    pub(crate) fn conclusion(&self) -> Conclusion {
        let outcome = match self {
            Self::Managed(result) => result.as_ref().err().map(|error| error.outcome()),
            Self::Imported(result) => result.as_ref().err().map(|error| error.outcome()),
            Self::RecipeExported(result) => result.as_ref().err().map(|error| error.outcome()),
            Self::Exported(result) => result.as_ref().err().map(|error| error.outcome()),
        };
        match outcome {
            None => Conclusion::Succeeded,
            Some(Outcome::Cancelled) => Conclusion::ExpectedCleanup,
            Some(_) => Conclusion::Failed,
        }
    }
}

/// Metadata of the occupied file slot; completion is a joined Runner response.
pub(crate) struct FileTask {
    pub(crate) id: TaskId,
    pub(crate) kind: FileKind,
}
impl FileKind {
    pub(crate) fn background_read(&self) -> bool {
        matches!(self,Self::Managed(job) if job.background_read())
    }
    pub(crate) fn accepts(&self, outcome: &FileOutcome) -> bool {
        match (self, outcome) {
            (Self::Import(_), FileOutcome::Imported(_))
            | (Self::ExportRecipe { .. }, FileOutcome::RecipeExported(_))
            | (Self::Export { .. }, FileOutcome::Exported(_)) => true,
            (Self::Managed(job), FileOutcome::Managed(Ok(outcome))) => job.accepts(outcome),
            (Self::Managed(_), FileOutcome::Managed(Err(_))) => true,
            _ => false,
        }
    }
    pub(crate) fn execute(self) -> FileOutcome {
        match self {
            Self::ExportRecipe { text, path } => FileOutcome::RecipeExported(
                crate::services::save_recipe_text(&text, &path, Replace::Allowed),
            ),
            Self::Managed(job) => FileOutcome::Managed(job.execute()),
            Self::Import(path) => FileOutcome::Imported(crate::services::open_file(&path)),
            Self::Export {
                path,
                cancel,
                progress,
                sound,
                ..
            } => FileOutcome::Exported(crate::services::export(
                sound.snapshot(),
                sound.signal().sample_rate(),
                &path,
                Replace::Allowed,
                &cancel,
                &progress,
            )),
        }
    }
}

#[cfg(test)]
use crate::services::playback::revalidate_managed;

/// An immutable admitted operation. Audio captures own their unique Session.
pub struct Job {
    id: TaskId,
    work: Box<Work>,
}
enum Work {
    File(FileKind),
    Audio {
        session: Session,
        kind: AudioKind,
        draft: Option<Box<Draft>>,
    },
}
impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("kind", &self.kind())
            .finish_non_exhaustive()
    }
}
/// A worker failure, distinct from an operation's own error.
#[derive(Debug, Clone)]
pub enum WorkerFailure {
    /// The operating system refused thread creation.
    Launch(crate::services::IoCause),
    /// The worker panicked; storage may have changed before the panic.
    Panicked,
    /// A completion did not match its admitted operation.
    InvalidOutcome,
}

/// Typed final result of a joined operation.
pub enum JobOutcome {
    /// Completed filesystem operation.
    File(FileOutcome),
    /// Completed playback or preparation failure.
    Audio(Result<PlaybackReport, PlaybackError>),
    /// No operation result can be trusted.
    Failed(WorkerFailure),
}
impl JobOutcome {
    pub(crate) fn failure(&self) -> Option<TaskFailure> {
        let error = match self {
            Self::Failed(error) => return Some(TaskFailure::Worker(error.clone())),
            Self::Audio(Err(error)) => crate::services::OperationError::from(error.clone()),
            Self::File(outcome) => match outcome {
                FileOutcome::Imported(Err(error)) => error.clone().into(),
                FileOutcome::Managed(Err(error)) => error.clone(),
                FileOutcome::RecipeExported(Err(error)) => error.clone().into(),
                FileOutcome::Exported(Err(error)) => error.clone().into(),
                _ => return None,
            },
            _ => return None,
        };
        (error.outcome() != Outcome::Cancelled).then_some(TaskFailure::Operation(error))
    }
}

impl Job {
    pub(crate) fn file(id: TaskId, kind: FileKind) -> Self {
        Self {
            id,
            work: Box::new(Work::File(kind)),
        }
    }
    pub(crate) fn audio(
        id: TaskId,
        session: Session,
        kind: AudioKind,
        draft: Option<Draft>,
    ) -> Self {
        Self {
            id,
            work: Box::new(Work::Audio {
                session,
                kind,
                draft: draft.map(Box::new),
            }),
        }
    }
    /// Identity used to correlate responses with the still-occupied slot.
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }
    /// Physical execution slot required by this job.
    #[must_use]
    pub fn kind(&self) -> JobKind {
        match *self.work {
            Work::File(_) => JobKind::File,
            Work::Audio { .. } => JobKind::Audio,
        }
    }
    /// Execute captured work on the caller thread and emit any audio preparation.
    /// The returned Finished response is delivered after the executor's thread joins.
    pub fn execute(self, mut prepared: impl FnMut(Response)) -> Response {
        let outcome = match *self.work {
            Work::File(kind) => JobOutcome::File(kind.execute()),
            Work::Audio {
                session,
                kind,
                draft,
            } => JobOutcome::Audio(execute_audio(
                self.id,
                session,
                kind,
                draft.map(|draft| *draft),
                &mut prepared,
            )),
        };
        Response::Finished {
            job: self.id,
            outcome,
        }
    }
}
fn execute_audio(
    id: TaskId,
    session: Session,
    kind: AudioKind,
    draft: Option<Draft>,
    prepared: &mut impl FnMut(Response),
) -> Result<PlaybackReport, PlaybackError> {
    use {
        crate::audio::session::MonitorDetail, crate::audio::session::Outcome as TransportOutcome,
        crate::audio::session::Preparation, crate::services::playback as device,
    };
    if session.state().outcome() == TransportOutcome::StopRequested {
        session.state().finish_drained(TransportOutcome::Completed);
        return session
            .state()
            .report()
            .ok_or(PlaybackError::MissingTerminalOutcome);
    }
    let (kind, library) = match kind {
        AudioKind::Managed { snapshot, store } => (AudioKind::Finite(snapshot), Some(store)),
        kind => (kind, None),
    };
    match (kind, draft) {
        (AudioKind::Live(_), Some(draft)) => match device::prepare_live(session, &draft)? {
            Preparation::Ready(mut output) => {
                let reader = output
                    .monitor(MonitorDetail::Waveform)
                    .reader()
                    .ok_or(PlaybackError::MonitorInUse)?;
                prepared(Response::Prepared {
                    job: id,
                    reader: Box::new(reader),
                    rate: output.sample_rate(),
                    live: Some(output.live_control()),
                });
                output.play()
            }
            Preparation::Stopped(report) => Ok(report),
        },
        (AudioKind::Finite(snapshot), _) => {
            match device::prepare_captured(session, &snapshot, library.as_ref())? {
                Preparation::Ready(mut output) => {
                    let reader = output
                        .monitor(MonitorDetail::Waveform)
                        .reader()
                        .ok_or(PlaybackError::MonitorInUse)?;
                    prepared(Response::Prepared {
                        job: id,
                        reader: Box::new(reader),
                        rate: output.output().sample_rate,
                        live: None,
                    });
                    output.play()
                }
                Preparation::Stopped(report) => Ok(report),
            }
        }
        (AudioKind::Managed { .. }, _) => {
            unreachable!("managed content resolved before device preparation")
        }
        (AudioKind::Live(_), None) => Err(PlaybackError::MissingDocument),
    }
}
struct Running {
    id: TaskId,
    handle: Worker,
    prepared: Receiver<Response>,
}
enum Worker {
    Direct(JoinHandle<Response>),
    /// The completion observer sends only after joining the actual worker.
    Notifying {
        observer: JoinHandle<()>,
        completed: Receiver<Response>,
    },
}
/// Execution strategy for admitted jobs, retaining each capture until completion.
/// Preparation responses may arrive before a worker exits; final responses require joining it.
pub trait Executor: std::any::Any {
    /// Install the event-loop notification before admitting work. Deterministic executors may ignore it.
    fn set_waker(&mut self, _wake: Arc<dyn Fn() + Send + Sync>) {}
    /// Accept one immutable admitted job; launch failure is returned through polling.
    fn start(&mut self, job: Job);
    /// Return preparation messages and joined terminal responses available now.
    fn poll(&mut self) -> Vec<Response>;
    /// End admitted work and join every owned worker before returning.
    ///
    /// # Errors
    /// Returns every observed operation, execution or editor-reset failure.
    fn shutdown(self: Box<Self>, workspace: Workspace) -> Result<(), ShutdownError>;
}
/// Executes a single workspace's admitted jobs and returns joined completions.
#[derive(Default)]
pub struct Runner {
    running: Vec<Running>,
    ready: Vec<Response>,
    retiring: Vec<JoinHandle<()>>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl Runner {
    /// Create an idle executor; it discovers no device and starts no thread.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            running: Vec::new(),
            ready: Vec::new(),
            retiring: Vec::new(),
            wake: None,
        }
    }
    /// Create an event-driven runner. Completion notifications follow the real worker join.
    #[must_use]
    pub fn with_waker(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            wake: Some(wake),
            ..Self::new()
        }
    }
    fn reap_observers(&mut self) {
        let mut index = 0;
        while index < self.retiring.len() {
            if self.retiring[index].is_finished() {
                let _ = self.retiring.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
    }
    fn failure(id: TaskId, error: std::io::Error) -> Response {
        Response::Finished {
            job: id,
            outcome: JobOutcome::Failed(WorkerFailure::Launch(crate::services::IoCause::new(
                error,
            ))),
        }
    }
    fn joined(id: TaskId, worker: JoinHandle<Response>) -> Response {
        worker.join().unwrap_or_else(|_| Response::Finished {
            job: id,
            outcome: JobOutcome::Failed(WorkerFailure::Panicked),
        })
    }
    fn post_joined(
        id: TaskId,
        started: std::io::Result<JoinHandle<Response>>,
        terminal: mpsc::Sender<Response>,
        wake: &dyn Fn(),
    ) {
        let response = match started {
            Ok(worker) => Self::joined(id, worker),
            Err(error) => Self::failure(id, error),
        };
        if terminal.send(response).is_ok() {
            wake();
        }
    }
    fn start_notifying(&mut self, mut job: Job, wake: Arc<dyn Fn() + Send + Sync>) {
        if let Work::File(FileKind::Export {
            progress, frames, ..
        }) = job.work.as_mut()
        {
            *progress = progress.with_waker(wake.clone(), *frames);
        }
        let id = job.id;
        let (tx, prepared) = mpsc::channel();
        let (terminal, completed) = mpsc::channel();
        let notify = wake.clone();
        // Start the observer first: failure cannot detach an already-launched worker.
        let observer = thread::Builder::new()
            .name("auditorylab-completion".into())
            .spawn(move || {
                let prepared_wake = notify.clone();
                let started = thread::Builder::new()
                    .name("auditorylab-worker".into())
                    .spawn(move || {
                        job.execute(|response| {
                            if tx.send(response).is_ok() {
                                prepared_wake();
                            }
                        })
                    });
                Self::post_joined(id, started, terminal, notify.as_ref());
            });
        match observer {
            Ok(observer) => self.running.push(Running {
                id,
                handle: Worker::Notifying {
                    observer,
                    completed,
                },
                prepared,
            }),
            Err(error) => {
                self.ready.push(Self::failure(id, error));
                wake();
            }
        }
    }
    /// Start one admitted operation. A launch error becomes a Finished response.
    pub fn start(&mut self, job: Job) {
        self.reap_observers();
        if let Some(wake) = self.wake.clone() {
            self.start_notifying(job, wake);
            return;
        }
        let id = job.id;
        let (tx, prepared) = mpsc::channel();
        let started = thread::Builder::new()
            .name("auditorylab-worker".into())
            .spawn(move || {
                job.execute(|response| {
                    let _ = tx.send(response);
                })
            });
        self.started(id, prepared, started);
    }
    fn started(
        &mut self,
        id: TaskId,
        prepared: Receiver<Response>,
        started: std::io::Result<JoinHandle<Response>>,
    ) {
        match started {
            Ok(handle) => self.running.push(Running {
                id,
                handle: Worker::Direct(handle),
                prepared,
            }),
            Err(error) => self.ready.push(Response::Finished {
                job: id,
                outcome: JobOutcome::Failed(WorkerFailure::Launch(crate::services::IoCause::new(
                    error,
                ))),
            }),
        }
    }
    /// Nonblocking: preparations may arrive early; final results require a join.
    pub fn poll(&mut self) -> Vec<Response> {
        self.reap_observers();
        let mut responses = std::mem::take(&mut self.ready);
        let mut index = 0;
        while index < self.running.len() {
            responses.extend(self.running[index].prepared.try_iter());
            let (finished, terminal) = match &self.running[index].handle {
                Worker::Direct(worker) => (worker.is_finished(), None),
                Worker::Notifying { completed, .. } => match completed.try_recv() {
                    Ok(response) => (true, Some(response)),
                    Err(mpsc::TryRecvError::Disconnected) => (
                        true,
                        Some(Response::Finished {
                            job: self.running[index].id,
                            outcome: JobOutcome::Failed(WorkerFailure::Panicked),
                        }),
                    ),
                    Err(mpsc::TryRecvError::Empty) => (false, None),
                },
            };
            if finished {
                let running = self.running.swap_remove(index);
                let response = match running.handle {
                    Worker::Direct(worker) => Self::joined(running.id, worker),
                    Worker::Notifying { observer, .. } => {
                        self.retiring.push(observer);
                        terminal.unwrap_or_else(|| {
                            unreachable!("notifying worker has joined terminal response")
                        })
                    }
                };
                responses.extend(running.prepared.try_iter());
                responses.push(response);
            } else {
                index += 1;
            }
        }
        responses
    }
    /// Whether any physical worker or undelivered launch failure remains.
    #[must_use]
    pub fn busy(&self) -> bool {
        !self.running.is_empty() || !self.ready.is_empty()
    }
    /// Stop audio, cancel export, finish admitted work and report actual failures.
    ///
    /// # Errors
    /// Returns the joined workers' operation and execution failures.
    pub fn shutdown(mut self, mut workspace: Workspace) -> Result<(), ShutdownError> {
        workspace.prepare_shutdown();
        for effect in workspace.take_effects() {
            if let super::Effect::Run(job) = effect {
                self.start(job);
            }
        }
        let mut responses = std::mem::take(&mut self.ready);
        for running in self.running.drain(..) {
            let response = match running.handle {
                Worker::Direct(worker) => Self::joined(running.id, worker),
                Worker::Notifying {
                    observer,
                    completed,
                } => {
                    let _ = observer.join();
                    completed.recv().unwrap_or_else(|_| Response::Finished {
                        job: running.id,
                        outcome: JobOutcome::Failed(WorkerFailure::Panicked),
                    })
                }
            };
            responses.extend(running.prepared.try_iter());
            responses.push(response);
        }
        for observer in self.retiring.drain(..) {
            let _ = observer.join();
        }
        let mut errors = Vec::new();
        for response in responses {
            let failure = if let Response::Finished { job, outcome } = &response {
                Some((
                    outcome.failure(),
                    workspace.job_context(*job),
                    workspace
                        .files.task()
                        .is_some_and(|task| {
                            task.id == *job
                                && !matches!(&task.kind, FileKind::Managed(job) if job.background_read())
                        }),
                ))
            } else {
                None
            };
            let disposition = workspace.respond(response);
            if disposition != super::ResponseDisposition::Ignored
                && let Some((operation_error, context, active_file)) = failure
            {
                if let Some(error) = operation_error {
                    let library = if active_file {
                        match &workspace.files.status() {
                            super::status::FileStatus::Library(
                                super::status::LibraryStatus::Failed {
                                    error,
                                    publishing,
                                    context,
                                },
                            ) => Some(LibraryFailure {
                                error: error.clone(),
                                publishing: *publishing,
                                context: context.clone(),
                            }),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    if let Some(failure) = library {
                        errors.push(ShutdownIssue::Library(failure));
                    } else {
                        errors.push(ShutdownIssue::Task {
                            error,
                            context: context.clone(),
                        });
                    }
                }
                if active_file && let Some(error) = workspace.files.status().editor_reset_error() {
                    errors.push(ShutdownIssue::EditorReset {
                        error: error.clone(),
                        context: context.clone(),
                    });
                }
                if disposition == super::ResponseDisposition::Rejected {
                    errors.push(ShutdownIssue::Task {
                        error: TaskFailure::Worker(WorkerFailure::InvalidOutcome),
                        context,
                    });
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(ShutdownError { issues: errors })
        }
    }
}

#[cfg(test)]
mod tests;

impl std::fmt::Display for WorkerFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}

/// Captured source of an admitted worker operation.
#[derive(Debug, Clone)]
pub enum JobContext {
    /// Managed root and captured selector.
    Library(super::status::LibraryContext),
    /// Portable input or export destination.
    File(std::path::PathBuf),
    /// Audio source title at admission.
    Audio(String),
    /// The operation no longer has a retained slot.
    Unknown,
}
/// Operation or worker execution failure, with the original cause.
#[derive(Debug, Clone)]
pub enum TaskFailure {
    /// The operation returned a failure.
    Operation(crate::services::OperationError),
    /// Execution failed independently of the operation.
    Worker(WorkerFailure),
}
/// Managed failure context for recovery wording.
#[derive(Debug, Clone)]
pub struct LibraryFailure {
    /// Retained operation cause.
    pub error: crate::services::OperationError,
    /// Whether the admitted operation could publish.
    pub publishing: bool,
    /// Exact root and selector used by the operation.
    pub context: super::status::LibraryContext,
}
/// A joined shutdown operation could not complete cleanly.
#[derive(Debug, Clone)]
pub enum ShutdownIssue {
    /// A managed operation failed with its publication context.
    Library(LibraryFailure),
    /// A joined operation failed or returned an inconsistent result.
    Task {
        /// Retained failure.
        error: TaskFailure,
        /// Source captured before applying the completion.
        context: JobContext,
    },
    /// Trash completed, but resetting the editor failed.
    EditorReset {
        /// Original allocation or document cause.
        error: crate::document::error::EditorError,
        /// Source captured before applying the completion.
        context: JobContext,
    },
}
/// Every observed failure after shutdown has joined its workers.
#[derive(Debug, Clone)]
pub struct ShutdownError {
    /// Failures in completion order.
    pub issues: Vec<ShutdownIssue>,
}
impl std::fmt::Display for ShutdownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}
impl std::error::Error for ShutdownError {}

impl Executor for Runner {
    fn set_waker(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.wake = Some(wake);
    }
    fn start(&mut self, job: Job) {
        Runner::start(self, job);
    }
    fn poll(&mut self) -> Vec<Response> {
        Runner::poll(self)
    }
    fn shutdown(self: Box<Self>, workspace: Workspace) -> Result<(), ShutdownError> {
        Runner::shutdown(*self, workspace)
    }
}

impl std::fmt::Display for TaskFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}
impl std::error::Error for TaskFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Operation(error) => error,
            Self::Worker(error) => error,
        })
    }
}
impl std::error::Error for WorkerFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Launch(cause) => Some(cause.error()),
            Self::Panicked | Self::InvalidOutcome => None,
        }
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    WorkerFailure {
        codes {
            Self::Launch(..) => "worker.launch",
            Self::Panicked => "worker.panicked",
            Self::InvalidOutcome => "worker.invalid_outcome",
        }
        delegates {}
        params(this) {
            match this {
                Self::Launch(cause) => {
                    crate::core::diagnostic_params! {"cause"=>cause.error().to_string()}
                }
                _ => crate::core::Params::new(),
            }
        }
    }
    ShutdownError {
        codes {
            Self { .. } => "shutdown.failed",
        }
        delegates {}
        params(this) {
            crate::core::diagnostic_params! {"issue_count"=>this.issues.len()}
        }
    }
    TaskFailure {
        codes {}
        delegates {
            Self::Operation(error) => error,
            Self::Worker(error) => error,
        }
    }
}
