//! A bounded audio or file task with independent cancellation, progress and completion.
use auditorylab::{Cancel, Draft, LiveControl, Preset, Progress, Sound};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
};
pub(crate) enum Message {
    Ready(auditorylab::MonitorReader),
    LiveReady(LiveControl, auditorylab::MonitorReader, std::sync::Arc<()>),
    Saved(
        std::sync::Arc<()>,
        Result<(Draft, PathBuf), auditorylab::Error>,
    ),
    Imported(Result<Preset, auditorylab::Error>),
    Done(Result<String, auditorylab::Error>),
}
pub(crate) enum Work {
    Import(PathBuf),
    Play(Preset),
    Live(Draft, std::sync::Arc<()>),
    Save(Draft, PathBuf, std::sync::Arc<()>),
    Export(Sound, PathBuf),
}
pub(crate) struct Worker {
    pub(crate) cancel: Cancel,
    pub(crate) progress: Progress,
    source_owner: Option<std::sync::Arc<()>>,
    messages: Receiver<Message>,
    handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub(crate) fn start(work: Work) -> Self {
        let source_owner = match &work {
            Work::Live(_, owner) => Some(owner.clone()),
            _ => None,
        };
        let cancel = Cancel::default();
        let progress = Progress::default();
        let c = cancel.clone();
        let p = progress.clone();
        let (tx, messages) = mpsc::channel();
        let handle = Some(thread::spawn(move || match work {
            Work::Import(path) => {
                let _ = tx.send(Message::Imported(auditorylab::load_file(&path)));
            }
            Work::Play(preset) => {
                let result = auditorylab::prepare_playback(&preset)
                    .and_then(|mut playback| {
                        let reader = playback
                            .monitor(auditorylab::MonitorDetail::Waveform)
                            .reader();
                        let _ = tx.send(Message::Ready(reader));
                        playback.play(&c, &p)
                    })
                    .map(|played| {
                        if played.underruns > 0 {
                            auditorylab::Notice::Underruns(played.underruns).to_string()
                        } else {
                            "Playback complete.".into()
                        }
                    });
                let _ = tx.send(Message::Done(result));
            }
            Work::Live(draft, owner) => {
                let result = auditorylab::prepare_live(&draft)
                    .and_then(|mut live| {
                        let reader = live.monitor(auditorylab::MonitorDetail::Waveform).reader();
                        let _ = tx.send(Message::LiveReady(live.control(), reader, owner));
                        live.play(&c, &p)
                    })
                    .map(|played| {
                        if played.underruns > 0 {
                            auditorylab::Notice::Underruns(played.underruns).to_string()
                        } else {
                            "Stopped.".into()
                        }
                    });
                let _ = tx.send(Message::Done(result));
            }
            Work::Save(draft, path, owner) => {
                let result = draft
                    .save(&path, auditorylab::Replace::Allowed)
                    .map(|()| (draft, path));
                let _ = tx.send(Message::Saved(owner, result));
            }
            Work::Export(sound, path) => {
                let result =
                    auditorylab::export(&sound, &path, auditorylab::Replace::Allowed, &c, &p)
                        .map(|_| format!("Exported WAV to {path:?}."));
                let _ = tx.send(Message::Done(result));
            }
        }));
        Self {
            cancel,
            progress,
            source_owner,
            messages,
            handle,
        }
    }
    pub(crate) fn owns_draft(&self, owner: &std::sync::Arc<()>) -> bool {
        self.source_owner
            .as_ref()
            .is_some_and(|source| std::sync::Arc::ptr_eq(source, owner))
    }
    pub(crate) fn receive(&self) -> Result<Option<Message>, String> {
        match self.messages.try_recv() {
            Ok(message) => Ok(Some(message)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err("The operation thread ended without a result.".into())
            }
        }
    }
    /// Drain both termination and failure evidence after an unexpected event-loop exit.
    pub(crate) fn shutdown(mut self) -> Result<(), String> {
        self.cancel.request();
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| "The operation thread failed.".to_owned())?;
        }
        let mut errors = Vec::new();
        for message in self.messages.try_iter() {
            let error = match message {
                Message::Saved(_, Err(e)) | Message::Imported(Err(e)) | Message::Done(Err(e)) => {
                    Some(e)
                }
                _ => None,
            };
            if let Some(error) = error
                && error.kind() != auditorylab::ErrorKind::Cancelled
            {
                errors.push(error.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
    pub(crate) fn join(mut self) -> Result<(), String> {
        self.handle.take().map_or(Ok(()), |h| {
            h.join().map_err(|_| "The operation thread failed.".into())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_panicked_worker_reports_disconnection_and_can_be_joined() {
        let (tx, messages) = mpsc::channel();
        let handle = thread::spawn(move || {
            let _sender = tx;
            panic!("injected operation failure");
        });
        let worker = Worker {
            cancel: Cancel::default(),
            progress: Progress::default(),
            source_owner: None,
            messages,
            handle: Some(handle),
        };
        assert!(matches!(
            worker
                .messages
                .recv_timeout(std::time::Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        assert_eq!(
            worker.receive().err().as_deref(),
            Some("The operation thread ended without a result.")
        );
        assert_eq!(worker.join(), Err("The operation thread failed.".into()));
    }
    #[test]
    fn cancelled_export_joins_and_removes_the_partial_file() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base).unwrap();
        let dir = tempfile::tempdir_in(base).unwrap();
        let path = dir.path().join("cancel.wav");
        let preset = auditorylab::load_reader(
            b"schema_version=2\nduration_seconds=1000000\nfade_seconds=0\n".as_slice(),
            "silence",
        )
        .unwrap();
        assert!(preset.stimulus().components.is_empty());
        let worker = Worker::start(Work::Export(
            preset.at(auditorylab::SampleRate::DEFAULT).unwrap(),
            path.clone(),
        ));
        worker.cancel.request();
        let message = worker
            .messages
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(
            matches!(message, Message::Done(Err(ref e)) if e.kind() == auditorylab::ErrorKind::Cancelled)
        );
        worker.join().unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
