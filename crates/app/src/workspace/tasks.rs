// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Worker threads for the two task slots and their typed results.
//!
//! A worker's final event does not prove that its thread has exited. The
//! slot keeps its join handle after the result arrives and is released only
//! after a nonblocking check sees the thread finished and the join consumes
//! the handle. A panic or a missing result is a failure.
use crate::editing::DocumentId;
use crate::error::{Error, ErrorKind};
use crate::recipe::{RecipeSnapshot, Rendition};
use crate::session::{LiveControl, MonitorReader, PlaybackControl, PlaybackReport};
use crate::wav::Exported;
use crate::workflow::{Cancel, Progress, Replace};
use auditorylab_core::{Recipe, SampleRate};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

/// The process-local identity of one task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub(crate) u64);

/// What a finished task means for actions that depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conclusion {
    /// The task did what it was asked.
    Succeeded,
    /// The task ended as requested: audio stopped, export cancelled.
    ExpectedCleanup,
    /// The task, its worker or its cleanup failed.
    Failed,
}

/// A worker thread, its event stream and, once received, its result.
pub(crate) struct Worker<E, R> {
    events: Receiver<E>,
    handle: Option<JoinHandle<()>>,
    result: Option<R>,
    launch_error: Option<String>,
}

/// One poll of a worker.
pub(crate) enum Polled<R> {
    /// Still running, or finishing until its thread exits.
    Busy,
    /// Joined. The result is `None` when the thread ended without one.
    Joined { result: Option<R>, panicked: bool },
}

impl<E: Send + 'static, R> Worker<E, R> {
    pub(crate) fn spawn(work: impl FnOnce(mpsc::Sender<E>) + Send + 'static) -> Self {
        let (tx, events) = mpsc::channel();
        let started = thread::Builder::new()
            .name("auditorylab-worker".into())
            .spawn(move || work(tx));
        Self::started(events, started)
    }

    fn started(events: Receiver<E>, started: std::io::Result<JoinHandle<()>>) -> Self {
        let (handle, launch_error) = match started {
            Ok(handle) => (Some(handle), None),
            Err(error) => (None, Some(format!("Cannot start worker: {error}"))),
        };
        Self {
            events,
            handle,
            result: None,
            launch_error,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn failed() -> Self {
        let (_, events) = mpsc::channel();
        Self::started(
            events,
            Err(std::io::Error::other("injected launch failure")),
        )
    }

    pub(crate) fn launch_error(&self) -> Option<&str> {
        self.launch_error.as_deref()
    }

    /// Drain events, keeping a final result; join only after the thread has
    /// exited. `final_event` turns an event into the result or passes it on.
    pub(crate) fn poll(&mut self, mut on_event: impl FnMut(E) -> Option<R>) -> Polled<R> {
        while let Ok(event) = self.events.try_recv() {
            if let Some(result) = on_event(event) {
                self.result = Some(result);
            }
        }
        if self.launch_error.is_some() || self.handle.as_ref().is_some_and(JoinHandle::is_finished)
        {
            // The thread has exited, so this join does not block.
            let panicked = self.handle.take().is_some_and(|h| h.join().is_err());
            // Events sent just before exit are still in the channel.
            while let Ok(event) = self.events.try_recv() {
                if let Some(result) = on_event(event) {
                    self.result = Some(result);
                }
            }
            return Polled::Joined {
                result: self.result.take(),
                panicked,
            };
        }
        Polled::Busy
    }

    /// Whether the final result has arrived while the thread still runs.
    pub(crate) const fn finishing(&self) -> bool {
        self.result.is_some()
    }

    /// Block until the thread exits; used only at shutdown, off the event loop.
    pub(crate) fn wait(mut self, mut on_event: impl FnMut(E) -> Option<R>) -> (Option<R>, bool) {
        let panicked = self.handle.take().is_some_and(|h| h.join().is_err());
        while let Ok(event) = self.events.try_recv() {
            if let Some(result) = on_event(event) {
                self.result = Some(result);
            }
        }
        (self.result, panicked)
    }
}

#[cfg_attr(
    not(any(feature = "device-playback", test)),
    expect(dead_code, reason = "Audio tasks start only with device playback")
)]
/// What an audio task plays.
#[derive(Debug, Clone)]
pub(crate) enum AudioKind {
    /// A finite snapshot, validated at the device rate during preparation.
    Finite(RecipeSnapshot),
    /// Revalidate a managed selection on the worker before device preparation.
    Managed {
        snapshot: RecipeSnapshot,
        store: crate::library::LibraryStore,
    },
    /// A document's committed components, live until stopped.
    Live(DocumentId),
}

#[cfg_attr(
    not(any(feature = "device-playback", test)),
    expect(dead_code, reason = "Audio tasks start only with device playback")
)]
/// Events from an audio worker. The prepared event carries the monitor
/// reader by value: it is sent once per session and moved into the view.
#[expect(
    clippy::large_enum_variant,
    reason = "One Prepared event per session moves its preallocated reader; boxing adds an allocation for no benefit"
)]
pub(crate) enum AudioEvent {
    /// Preparation succeeded and the stream is about to start.
    Prepared {
        reader: MonitorReader,
        rate: SampleRate,
        live: Option<LiveControl>,
    },
    /// Playback ended, or preparation stopped or failed.
    Finished(Result<PlaybackReport, Error>),
}

/// The audio slot's task.
pub(crate) struct AudioTask {
    pub(crate) id: TaskId,
    pub(crate) kind: AudioKind,
    pub(crate) title: String,
    pub(crate) control: PlaybackControl,
    pub(crate) live: Option<LiveControl>,
    pub(crate) rate: Option<SampleRate>,
    pub(crate) worker: Worker<AudioEvent, Result<PlaybackReport, Error>>,
}

/// What a file task does.
#[derive(Debug, Clone)]
pub(crate) enum FileKind {
    Import(PathBuf),
    Managed(super::persistent::Job),
    ExportRecipe {
        text: std::sync::Arc<str>,
        path: PathBuf,
    },
    Save {
        document: DocumentId,
        content: std::sync::Arc<Recipe>,
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
pub(crate) enum FileOutcome {
    Imported(Result<RecipeSnapshot, Error>),
    Managed(Result<super::persistent::ResultValue, Error>),
    Saved(Result<crate::workflow::SaveOutcome, Error>),
    RecipeExported(Result<crate::workflow::SaveOutcome, Error>),
    Exported(Result<Exported, Error>),
}

impl FileOutcome {
    pub(crate) fn conclusion(&self) -> Conclusion {
        let result = match self {
            Self::Managed(r) => r.as_ref().err(),
            Self::Imported(r) => r.as_ref().err(),
            Self::RecipeExported(r) => r.as_ref().err(),
            Self::Saved(r) => r.as_ref().err(),
            Self::Exported(r) => r.as_ref().err(),
        };
        match result {
            None => Conclusion::Succeeded,
            Some(e) if e.kind() == ErrorKind::Cancelled => Conclusion::ExpectedCleanup,
            Some(_) => Conclusion::Failed,
        }
    }
}

/// The file slot's task.
pub(crate) struct FileTask {
    pub(crate) id: TaskId,
    pub(crate) kind: FileKind,
    pub(crate) worker: Worker<FileOutcome, FileOutcome>,
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) type FileSpawner = dyn FnMut(FileKind) -> Worker<FileOutcome, FileOutcome>;
#[cfg(any(test, feature = "test-support"))]
pub(crate) type AudioSpawner = dyn FnMut(
    crate::session::Session,
    AudioKind,
    Option<crate::editing::Draft>,
) -> Worker<AudioEvent, Result<PlaybackReport, Error>>;

impl FileKind {
    pub(crate) fn execute(self) -> FileOutcome {
        match self {
            Self::ExportRecipe { text, path } => FileOutcome::RecipeExported(
                crate::workflow::save_recipe_text(&text, &path, Replace::Allowed),
            ),
            Self::Managed(job) => FileOutcome::Managed(job.execute()),
            Self::Import(path) => FileOutcome::Imported(crate::recipe::load_file(&path)),
            Self::Save { content, path, .. } => FileOutcome::Saved(crate::editing::save_content(
                &content,
                &path,
                Replace::Allowed,
            )),
            Self::Export {
                path,
                cancel,
                progress,
                sound,
                ..
            } => FileOutcome::Exported(crate::workflow::export(
                &sound,
                &path,
                Replace::Allowed,
                &cancel,
                &progress,
            )),
        }
    }
}

pub(crate) fn file_worker(kind: FileKind) -> Worker<FileOutcome, FileOutcome> {
    Worker::spawn(move |tx| {
        let _ = tx.send(kind.execute());
    })
}

#[cfg(any(feature = "device-playback", test))]
pub(super) fn revalidate_managed(
    snapshot: &RecipeSnapshot,
    store: &crate::library::LibraryStore,
) -> Result<RecipeSnapshot, Error> {
    let crate::recipe::Origin::User { id, version } = snapshot.origin() else {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "expected a captured User selection",
        ));
    };
    let record = store.load(&crate::library::LibraryKey::User(*id))?;
    if record.trashed || &record.version_token != version {
        return Err(Error::new(
            ErrorKind::Conflict,
            "Saved selection changed or moved to Trash. Refresh before starting playback; no audio was started.",
        ));
    }
    Ok(record.snapshot)
}

#[cfg(feature = "device-playback")]
pub(crate) fn audio_worker(
    session: crate::session::Session,
    kind: AudioKind,
    draft: Option<crate::editing::Draft>,
) -> Worker<AudioEvent, Result<PlaybackReport, Error>> {
    use crate::device;
    use crate::session::{MonitorDetail, Preparation};
    Worker::spawn(move |tx| {
        let kind = match kind {
            AudioKind::Managed { snapshot, store } => match revalidate_managed(&snapshot, &store) {
                Ok(snapshot) => AudioKind::Finite(snapshot),
                Err(error) => {
                    let _ = tx.send(AudioEvent::Finished(Err(error)));
                    return;
                }
            },
            kind => kind,
        };
        let result = match (kind, draft) {
            (AudioKind::Live(_), Some(draft)) => match device::prepare_live(session, &draft) {
                Ok(Preparation::Ready(mut prepared)) => {
                    let Some(reader) = prepared.monitor(MonitorDetail::Waveform).reader() else {
                        let _ = tx.send(AudioEvent::Finished(Err(Error::new(
                            crate::error::ErrorKind::Device,
                            crate::vocabulary::Caption::MonitorAlreadyInUse.label(),
                        ))));
                        return;
                    };
                    let _ = tx.send(AudioEvent::Prepared {
                        reader,
                        rate: prepared.sample_rate(),
                        live: Some(prepared.live_control()),
                    });
                    prepared.play()
                }
                Ok(Preparation::Stopped(report)) => Ok(report),
                Err(error) => Err(error),
            },
            (AudioKind::Finite(snapshot), _) => {
                match device::prepare_playback(session, &snapshot) {
                    Ok(Preparation::Ready(mut prepared)) => {
                        let Some(reader) = prepared.monitor(MonitorDetail::Waveform).reader()
                        else {
                            let _ = tx.send(AudioEvent::Finished(Err(Error::new(
                                crate::error::ErrorKind::Device,
                                crate::vocabulary::Caption::MonitorAlreadyInUse.label(),
                            ))));
                            return;
                        };
                        let _ = tx.send(AudioEvent::Prepared {
                            reader,
                            rate: prepared.output().sample_rate,
                            live: None,
                        });
                        prepared.play()
                    }
                    Ok(Preparation::Stopped(report)) => Ok(report),
                    Err(error) => Err(error),
                }
            }
            (AudioKind::Managed { .. }, _) => {
                unreachable!("managed source is resolved before device preparation")
            }
            (AudioKind::Live(_), None) => Err(Error::new(
                ErrorKind::InvalidSound,
                "a live session needs a committed document",
            )),
        };
        let _ = tx.send(AudioEvent::Finished(result));
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn a_result_before_exit_keeps_the_slot_finishing_until_the_join() {
        let release = Arc::new(Barrier::new(2));
        let worker_release = Arc::clone(&release);
        let (sent, received) = mpsc::channel();
        let mut worker: Worker<u8, u8> = Worker::spawn(move |tx| {
            tx.send(7).unwrap();
            sent.send(()).unwrap();
            worker_release.wait();
        });
        // Prove the event is queued; a fixed number of yields does not prove
        // that the operating system has scheduled the worker.
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(worker.poll(Some), Polled::Busy));
        assert!(worker.finishing(), "the result arrived before worker exit");
        assert!(matches!(worker.poll(Some), Polled::Busy), "still owned");
        release.wait();
        loop {
            if let Polled::Joined { result, panicked } = worker.poll(Some) {
                assert_eq!(result, Some(7));
                assert!(!panicked);
                break;
            }
            thread::yield_now();
        }
    }

    #[test]
    fn a_panic_after_a_result_is_still_a_visible_failure() {
        let mut worker: Worker<u8, u8> = Worker::spawn(|tx| {
            tx.send(1).unwrap();
            panic!("injected worker failure after its result");
        });
        loop {
            if let Polled::Joined { result, panicked } = worker.poll(Some) {
                assert_eq!(result, Some(1), "publication truth is kept");
                assert!(panicked, "the panic is not hidden by the earlier result");
                break;
            }
            thread::yield_now();
        }
        let mut silent: Worker<u8, u8> = Worker::spawn(|_| {});
        loop {
            if let Polled::Joined { result, .. } = silent.poll(Some) {
                assert_eq!(result, None, "a missing result is observable");
                break;
            }
            thread::yield_now();
        }
    }
}

#[cfg(test)]
mod launch_tests {
    use super::*;
    #[test]
    fn launch_failure_has_no_worker_and_is_a_visible_terminal_outcome() {
        let (_, events) = mpsc::channel::<u8>();
        let mut worker: Worker<u8, u8> = Worker::started(
            events,
            Err(std::io::Error::other("injected launch failure")),
        );
        assert!(worker.handle.is_none());
        assert!(
            worker
                .launch_error()
                .unwrap()
                .contains("injected launch failure")
        );
        assert!(matches!(
            worker.poll(Some),
            Polled::Joined {
                result: None,
                panicked: false
            }
        ));
    }
    #[test]
    fn panic_before_result_cannot_be_success() {
        let worker: Worker<u8, u8> = Worker::spawn(|_| panic!("before result"));
        let (result, panicked) = worker.wait(Some);
        assert!(result.is_none());
        assert!(panicked);
    }
}
