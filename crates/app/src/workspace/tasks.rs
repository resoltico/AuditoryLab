// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Immutable job captures and the application-owned thread runner.
use super::{Response, Workspace};
use crate::audio::session::{LiveControl, PlaybackControl, PlaybackReport, Session};
use crate::document::editing::{DocumentId, Draft};
use crate::document::recipe::{RecipeSnapshot, Rendition};
use crate::error::{Error, ErrorKind};
use crate::services::{Cancel, Progress, Replace};
use crate::storage::wav::Exported;
use auditorylab_core::SampleRate;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
};

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
        store: crate::storage::library::LibraryStore,
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
    Imported(Result<RecipeSnapshot, Error>),
    /// Managed library operation.
    Managed(Result<super::library::LibraryOutcome, Error>),
    /// Published portable bytes.
    RecipeExported(Result<crate::services::SaveOutcome, Error>),
    /// Published audio export.
    Exported(Result<Exported, Error>),
}

impl FileOutcome {
    pub(crate) fn conclusion(&self) -> Conclusion {
        let result = match self {
            Self::Managed(r) => r.as_ref().err(),
            Self::Imported(r) => r.as_ref().err(),
            Self::RecipeExported(r) => r.as_ref().err(),
            Self::Exported(r) => r.as_ref().err(),
        };
        match result {
            None => Conclusion::Succeeded,
            Some(e) if e.kind() == ErrorKind::Cancelled => Conclusion::ExpectedCleanup,
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
            Self::Import(path) => FileOutcome::Imported(crate::storage::load::load_file(&path)),
            Self::Export {
                path,
                cancel,
                progress,
                sound,
                ..
            } => FileOutcome::Exported(crate::services::export(
                &sound,
                &path,
                Replace::Allowed,
                &cancel,
                &progress,
            )),
        }
    }
}

pub(super) fn revalidate_managed(
    snapshot: &RecipeSnapshot,
    store: &crate::storage::library::LibraryStore,
) -> Result<RecipeSnapshot, Error> {
    let crate::document::recipe::Origin::User { id, version } = snapshot.origin() else {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "expected a captured User selection",
        ));
    };
    let record = store.load(&crate::storage::library::LibraryKey::User(*id))?;
    if record.trashed || &record.version_token != version {
        return Err(Error::new(
            ErrorKind::Conflict,
            "Saved selection changed or moved to Trash. Refresh before starting playback; no audio was started.",
        ));
    }
    Ok(record.snapshot)
}

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
#[derive(Debug)]
pub enum WorkerFailure {
    /// The operating system refused thread creation.
    Launch(std::io::Error),
    /// The worker panicked; storage may have changed before the panic.
    Panicked,
    /// A completion did not match its admitted operation.
    InvalidOutcome,
}
impl WorkerFailure {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Launch(error) => format!("Cannot start worker: {error}"),
            Self::Panicked => crate::presentation::WORKER_FAILED.into(),
            Self::InvalidOutcome => "The worker returned a result for another operation.".into(),
        }
    }
}
/// Typed final result of a joined operation.
pub enum JobOutcome {
    /// Completed filesystem operation.
    File(FileOutcome),
    /// Completed playback or preparation failure.
    Audio(Result<PlaybackReport, Error>),
    /// No operation result can be trusted.
    Failed(WorkerFailure),
}
impl JobOutcome {
    pub(crate) fn failure(&self) -> Option<String> {
        match self {
            Self::Failed(error) => Some(error.description()),
            Self::Audio(Err(error)) => Some(error.to_string()),
            Self::File(outcome) => match outcome {
                FileOutcome::Imported(Err(e))
                | FileOutcome::Managed(Err(e))
                | FileOutcome::RecipeExported(Err(e))
                | FileOutcome::Exported(Err(e))
                    if e.kind() != ErrorKind::Cancelled =>
                {
                    Some(e.to_string())
                }
                _ => None,
            },
            _ => None,
        }
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
) -> Result<PlaybackReport, Error> {
    use crate::{
        audio::device,
        audio::session::{MonitorDetail, Outcome, Preparation},
    };
    if session.state().outcome() == Outcome::StopRequested {
        session.state().finish_drained(Outcome::Completed);
        return session.state().report().ok_or_else(|| {
            Error::new(
                ErrorKind::Device,
                "stopped preparation has no terminal outcome",
            )
        });
    }
    let kind = match kind {
        AudioKind::Managed { snapshot, store } => {
            AudioKind::Finite(revalidate_managed(&snapshot, &store)?)
        }
        kind => kind,
    };
    match (kind, draft) {
        (AudioKind::Live(_), Some(draft)) => match device::prepare_live(session, &draft)? {
            Preparation::Ready(mut output) => {
                let reader = output
                    .monitor(MonitorDetail::Waveform)
                    .reader()
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::Device,
                            crate::presentation::vocabulary::Caption::MonitorAlreadyInUse.label(),
                        )
                    })?;
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
        (AudioKind::Finite(snapshot), _) => match device::prepare_playback(session, &snapshot)? {
            Preparation::Ready(mut output) => {
                let reader = output
                    .monitor(MonitorDetail::Waveform)
                    .reader()
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::Device,
                            crate::presentation::vocabulary::Caption::MonitorAlreadyInUse.label(),
                        )
                    })?;
                prepared(Response::Prepared {
                    job: id,
                    reader: Box::new(reader),
                    rate: output.output().sample_rate,
                    live: None,
                });
                output.play()
            }
            Preparation::Stopped(report) => Ok(report),
        },
        (AudioKind::Managed { .. }, _) => {
            unreachable!("managed content resolved before device preparation")
        }
        (AudioKind::Live(_), None) => Err(Error::new(
            ErrorKind::InvalidSound,
            "a live session needs a committed document",
        )),
    }
}
struct Running {
    id: TaskId,
    handle: JoinHandle<Response>,
    prepared: Receiver<Response>,
}
/// Executes a single workspace's admitted jobs and returns joined completions.
#[derive(Default)]
pub struct Runner {
    running: Vec<Running>,
    ready: Vec<Response>,
}
impl Runner {
    /// Create an idle executor; it discovers no device and starts no thread.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            running: Vec::new(),
            ready: Vec::new(),
        }
    }
    /// Start one admitted operation. A launch error becomes a Finished response.
    pub fn start(&mut self, job: Job) {
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
                handle,
                prepared,
            }),
            Err(error) => self.ready.push(Response::Finished {
                job: id,
                outcome: JobOutcome::Failed(WorkerFailure::Launch(error)),
            }),
        }
    }
    /// Nonblocking: preparations may arrive early; final results require a join.
    pub fn poll(&mut self) -> Vec<Response> {
        let mut responses = std::mem::take(&mut self.ready);
        let mut index = 0;
        while index < self.running.len() {
            responses.extend(self.running[index].prepared.try_iter());
            if self.running[index].handle.is_finished() {
                let running = self.running.swap_remove(index);
                let response = running
                    .handle
                    .join()
                    .unwrap_or_else(|_| Response::Finished {
                        job: running.id,
                        outcome: JobOutcome::Failed(WorkerFailure::Panicked),
                    });
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
    pub fn shutdown(mut self, mut workspace: Workspace) -> Result<(), String> {
        workspace.prepare_shutdown();
        for effect in workspace.take_effects() {
            if let super::Effect::Run(job) = effect {
                self.start(job);
            }
        }
        let mut responses = std::mem::take(&mut self.ready);
        for running in self.running.drain(..) {
            let response = running
                .handle
                .join()
                .unwrap_or_else(|_| Response::Finished {
                    job: running.id,
                    outcome: JobOutcome::Failed(WorkerFailure::Panicked),
                });
            responses.extend(running.prepared.try_iter());
            responses.push(response);
        }
        let mut errors = Vec::new();
        for response in responses {
            let failure = if let Response::Finished { job, outcome } = &response {
                Some((
                    outcome.failure(),
                    workspace.job_context(*job),
                    workspace
                        .files
                        .slot
                        .as_ref()
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
                    errors.push(
                        if active_file {
                            workspace.files.status.library_failure_notice()
                        } else {
                            None
                        }
                        .unwrap_or_else(|| format!("{error}{context}")),
                    );
                }
                if active_file && let Some(error) = workspace.files.status.editor_reset_error() {
                    errors.push(format!(
                        "Trash confirmed; editor reset failed: {error}{context}"
                    ));
                }
                if disposition == super::ResponseDisposition::Rejected {
                    errors.push(format!(
                        "{}{context}",
                        WorkerFailure::InvalidOutcome.description()
                    ));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests;
