// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! One operation-scoped worker owns all native capabilities until they settle.
use auditory_app::{
    self as app, AppError, Cancellation, DeclaredDescription, ExportReport, ExportView, Facts,
    LoadedPreset, PlaybackReport, PlaybackView, SampleRate, SignalObservation,
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone)]
pub(crate) enum Input {
    Catalog(String),
    Imported(LoadedPreset),
}
impl Input {
    fn load(self) -> Result<LoadedPreset, AppError> {
        match self {
            Self::Catalog(id) => app::load_catalog(id),
            Self::Imported(preset) => Ok(preset),
        }
    }
}
pub(crate) enum Job {
    Catalog,
    Import(PathBuf),
    Inspect(Input, SampleRate),
    Play(Input),
    Export(Input, SampleRate, PathBuf),
}
impl Job {
    pub(crate) fn is_playback(&self) -> bool {
        matches!(self, Self::Play(_))
    }
    pub(crate) fn cancellable(&self) -> bool {
        matches!(self, Self::Play(_) | Self::Export(..))
    }
    pub(crate) fn destination(&self) -> Option<PathBuf> {
        if let Self::Export(_, _, path) = self {
            Some(path.clone())
        } else {
            None
        }
    }
}
pub(crate) enum Preparation {
    Playback(PlaybackView, Box<SignalObservation>),
    Export(ExportView),
}
pub(crate) enum Outcome {
    Catalog(Vec<DeclaredDescription>),
    Imported(LoadedPreset),
    Inspected(Facts),
    Played(PlaybackReport),
    Exported(ExportReport),
}
pub(crate) enum Event {
    Prepared(Preparation),
    Finished(Result<Outcome, AppError>),
}
pub(crate) struct Message {
    pub(crate) id: u64,
    pub(crate) event: Event,
}
pub(crate) struct Worker {
    pub(crate) messages: Receiver<Message>,
    pub(crate) permit: Option<SyncSender<()>>,
    pub(crate) cancellation: Cancellation,
    pub(crate) handle: Option<JoinHandle<()>>,
}
impl Worker {
    pub(crate) fn spawn(id: u64, job: Job) -> std::io::Result<Self> {
        let (send, messages) = mpsc::sync_channel(2);
        let (permit, receive) = mpsc::sync_channel(1);
        let cancellation = Cancellation::default();
        let cancel = cancellation.clone();
        let handle = thread::Builder::new()
            .name(format!("auditory-operation-{id}"))
            .spawn(move || {
                let result = execute(id, job, &send, &receive, &cancel);
                let _ = send.send(Message {
                    id,
                    event: Event::Finished(result),
                });
            })?;
        Ok(Self {
            messages,
            permit: Some(permit),
            cancellation,
            handle: Some(handle),
        })
    }
    pub(crate) fn stop(&mut self) {
        self.cancellation.request();
        self.permit.take();
    }
    pub(crate) fn finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub(crate) fn join(&mut self) -> Result<(), String> {
        match self.handle.take(){Some(handle)=>handle.join().map_err(|_|"The operation worker panicked; check the requested destination for any completed file.".into()),None=>Ok(())}
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Normal close joins only after is_finished. This fallback retains
        // ownership on event-loop failure or unwinding instead of detaching work.
        self.stop();
        let _ = self.join();
    }
}

fn await_permit(permit: &Receiver<()>, cancel: &Cancellation) {
    loop {
        if cancel.is_requested() {
            return;
        }
        match permit.recv_timeout(Duration::from_millis(10)) {
            Ok(()) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                cancel.request();
                return;
            }
        }
    }
}
fn deliver_preparation(
    id: u64,
    view: Preparation,
    send: &SyncSender<Message>,
    permit: &Receiver<()>,
    cancel: &Cancellation,
) {
    if send
        .send(Message {
            id,
            event: Event::Prepared(view),
        })
        .is_err()
    {
        cancel.request();
        return;
    }
    await_permit(permit, cancel);
}
fn execute(
    id: u64,
    job: Job,
    send: &SyncSender<Message>,
    permit: &Receiver<()>,
    cancel: &Cancellation,
) -> Result<Outcome, AppError> {
    match job {
        Job::Catalog => app::catalog().map(Outcome::Catalog),
        Job::Import(path) => app::load_file(path).map(Outcome::Imported),
        Job::Inspect(input, rate) => input.load()?.inspect(rate).map(Outcome::Inspected),
        Job::Play(input) => {
            let (prepared, observation) = input
                .load()?
                .prepare_observed_playback(crate::signal_geometry::reserve_bytes())?;
            deliver_preparation(
                id,
                Preparation::Playback(prepared.view().clone(), Box::new(observation)),
                send,
                permit,
                cancel,
            );
            prepared.execute(cancel).map(Outcome::Played)
        }
        Job::Export(input, rate, path) => {
            let prepared = input.load()?.prepare_export(rate, &path)?;
            deliver_preparation(
                id,
                Preparation::Export(prepared.view().clone()),
                send,
                permit,
                cancel,
            );
            prepared.execute(cancel).map(Outcome::Exported)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn silence() -> Input {
        Input::Imported(
            app::load_bytes(
                b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
                app::Origin::Reader("worker test".into()),
            )
            .unwrap(),
        )
    }
    #[test]
    fn export_waits_for_permit_and_cancellation_before_permit_never_publishes() {
        for stop in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("new.wav");
            let mut worker =
                Worker::spawn(1, Job::Export(silence(), SampleRate::DEFAULT, path.clone()))
                    .unwrap();
            let message = worker
                .messages
                .recv_timeout(Duration::from_secs(20))
                .unwrap();
            assert!(matches!(
                message.event,
                Event::Prepared(Preparation::Export(_))
            ));
            assert!(!path.exists());
            if stop {
                worker.stop();
            } else {
                worker.permit.as_ref().unwrap().send(()).unwrap();
            }
            let message = worker
                .messages
                .recv_timeout(Duration::from_secs(20))
                .unwrap();
            match message.event {
                Event::Finished(result) => {
                    if stop {
                        assert_eq!(result.err().unwrap().class(), app::ErrorClass::Cancelled);
                        assert!(!path.exists());
                    } else {
                        assert!(matches!(result, Ok(Outcome::Exported(_))));
                        assert!(path.exists());
                    }
                }
                _ => panic!("missing terminal"),
            }
            worker.join().unwrap();
        }
    }
    #[test]
    fn disconnected_ui_does_not_strand_permit_wait_or_publish() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.wav");
        let mut worker =
            Worker::spawn(2, Job::Export(silence(), SampleRate::DEFAULT, path.clone())).unwrap();
        let _ = worker
            .messages
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        worker.permit.take();
        worker.join().unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn missing_export_folder_is_a_bound_worker_failure_after_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-created/new.wav");
        let mut worker =
            Worker::spawn(3, Job::Export(silence(), SampleRate::DEFAULT, path.clone())).unwrap();
        let preparation = worker
            .messages
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        match preparation.event {
            Event::Prepared(Preparation::Export(view)) => assert_eq!(view.destination(), path),
            _ => panic!("expected bound preparation before filesystem execution"),
        }
        worker.permit.as_ref().unwrap().send(()).unwrap();
        let finished = worker
            .messages
            .recv_timeout(Duration::from_secs(20))
            .unwrap();
        let Event::Finished(Err(error)) = finished.event else {
            panic!("missing real filesystem failure")
        };
        assert_eq!(error.destination(), Some(path.as_path()));
        assert!(
            matches!(error.diagnostic(), app::Diagnostic::Filesystem(details) if details.phase() == app::FilesystemPhase::CreateStage)
        );
        worker.join().unwrap();
        assert!(!path.exists());
    }
}
