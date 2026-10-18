// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Display-independent operation lifetime and source-selection controller.
use crate::{
    paths::PathForm,
    presentation,
    signal::SignalDisplay,
    worker::{Event, Input, Job, Message, Outcome, Preparation, Worker},
};
use auditorylab::{DeclaredDescription, ErrorClass, LoadedPreset};
use std::{path::PathBuf, sync::mpsc::TryRecvError};
#[derive(Clone)]
pub(crate) struct Selection {
    pub(crate) input: Input,
    pub(crate) description: DeclaredDescription,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum State {
    Idle,
    Preparing,
    Running,
    Stopping,
    Completed,
    Cancelled,
    Failed,
}
struct Active {
    id: u64,
    worker: Worker,
    cancellable: bool,
    destination: Option<PathBuf>,
    selection_revision: u64,
    terminal: bool,
    prepared: bool,
    playback: bool,
}
pub(crate) struct Controller {
    pub(crate) catalog: Vec<DeclaredDescription>,
    pub(crate) imported: Option<LoadedPreset>,
    pub(crate) selection: Option<Selection>,
    pub(crate) selected_index: usize,
    pub(crate) input_path: PathForm,
    pub(crate) folder: PathForm,
    pub(crate) state: State,
    pub(crate) activity: String,
    pub(crate) dialog: bool,
    pub(crate) signal: SignalDisplay,
    active: Option<Active>,
    next_id: u64,
    selection_revision: u64,
    pending_close: bool,
    last_result: String,
    presentation_notification: Option<String>,
}
impl Default for Controller {
    fn default() -> Self {
        Self{catalog:Vec::new(),imported:None,selection:None,selected_index:0,input_path:PathForm::default(),folder:PathForm::default(),state:State::Idle,activity:"Choose a built-in sound or explicitly open a recipe. No audio starts automatically.".into(),dialog:false,signal:SignalDisplay::default(),active:None,next_id:1,selection_revision:0,pending_close:false,last_result:String::new(),presentation_notification:None}
    }
}
impl Controller {
    pub(crate) fn busy(&self) -> bool {
        self.active.is_some() || self.dialog || self.pending_close
    }
    pub(crate) fn polling_needed(&self) -> bool {
        self.active.is_some()
    }
    pub(crate) fn selection_locked(&self) -> bool {
        self.active.as_ref().is_some_and(|active| active.playback)
    }
    pub(crate) fn state_label(&self) -> String {
        if self.state == State::Running && self.selection_locked() {
            "Playing".into()
        } else {
            format!("{:?}", self.state)
        }
    }
    pub(crate) fn stoppable(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|a| a.cancellable && !a.terminal && !a.worker.cancellation.is_requested())
            && !self.pending_close
    }
    pub(crate) fn select(&mut self, index: usize) {
        if self.selection_locked() {
            return;
        }
        let selected = if let Some(description) = self.catalog.get(index) {
            if let auditorylab::Origin::BuiltIn(id) = description.origin() {
                Some(Selection {
                    input: Input::Catalog(id.clone()),
                    description: description.clone(),
                })
            } else {
                None
            }
        } else if index == self.catalog.len() {
            self.imported.as_ref().map(|p| Selection {
                input: Input::Imported(p.clone()),
                description: p.description().clone(),
            })
        } else {
            None
        };
        if let Some(selection) = selected {
            self.signal.clear();
            self.selected_index = index;
            self.selection = Some(selection);
            self.selection_revision = self.selection_revision.saturating_add(1);
        }
    }
    pub(crate) fn start(&mut self, job: Job) -> bool {
        if self.busy() {
            return false;
        }
        let id = self.next_id;
        let Some(next) = id.checked_add(1) else {
            self.fail("Operation identifiers exhausted; restart the application.".into());
            return false;
        };
        self.next_id = next;
        let cancellable = job.cancellable();
        let playback = job.is_playback();
        let destination = job.destination();
        if playback {
            // Release the previous observation before preparing its successor.
            self.signal.clear();
        }
        match Worker::spawn(id, job) {
            Ok(worker) => {
                self.active = Some(Active {
                    id,
                    worker,
                    cancellable,
                    destination,
                    selection_revision: self.selection_revision,
                    terminal: false,
                    prepared: false,
                    playback,
                });
                self.state = State::Preparing;
                self.activity="Preparing operation. Native queries can delay cancellation; close waits for the actual worker result.".into();
                true
            }
            Err(error) => {
                self.fail(format!("Cannot start operation worker: {error}"));
                false
            }
        }
    }
    pub(crate) fn stop(&mut self) {
        if let Some(active) = &mut self.active
            && active.cancellable
            && !active.terminal
            && !active.worker.cancellation.is_requested()
        {
            active.worker.stop();
            self.state = State::Stopping;
            self.activity.push_str(
                "\nStopping: waiting for the actual result; queued audio cannot be recalled.",
            );
        }
    }
    pub(crate) fn request_close(&mut self) -> bool {
        self.pending_close = true;
        if let Some(active) = &mut self.active {
            active.worker.stop();
            if !active.terminal {
                self.state = State::Stopping;
            }
        }
        self.close_ready()
    }
    pub(crate) fn close_ready(&self) -> bool {
        self.pending_close && self.active.is_none() && !self.dialog
    }
    /// A window presentation failure does not change an operation or its result.
    pub(crate) fn notify_presentation_error(&mut self, message: String) {
        self.presentation_notification = Some(message);
    }
    pub(crate) fn fail(&mut self, message: String) {
        self.state = State::Failed;
        self.activity = message;
    }
    pub(crate) fn dialog_finished(&mut self, selection: Option<PathBuf>, folder: bool) {
        self.dialog = false;
        let selected = selection.is_some();
        if folder {
            self.folder.choose(selection);
        } else {
            self.input_path.choose(selection);
        }
        if !selected {
            self.activity="No selection. The previous path is unchanged; manual path entry remains available.".into();
        }
    }
    pub(crate) fn receive(&mut self, message: Message) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        if active.id != message.id {
            return;
        }
        match message.event {
            Event::Prepared(view) => {
                if let Some(active) = &mut self.active {
                    active.prepared = true;
                }
                self.activity = match view {
                    Preparation::Playback(view, observation) => {
                        self.signal.attach(*observation);
                        presentation::playback(&view)
                    }
                    Preparation::Export(view) => presentation::export(&view),
                };
                // Update UI-thread controller state before the caller applies the
                // state to widgets and explicitly permits this same operation.
            }
            Event::Finished(result) => {
                let revision = active.selection_revision;
                if let Some(active) = &mut self.active {
                    active.terminal = true;
                }
                match result {
                    Ok(outcome) => {
                        self.state = State::Completed;
                        self.activity = match outcome {
                            Outcome::Catalog(catalog) => {
                                self.state = State::Idle;
                                self.catalog = catalog;
                                if self.selection.is_none() {
                                    self.select(0);
                                }
                                "Catalog loaded. Selecting a sound does not play it.".into()
                            }
                            Outcome::Imported(preset) => {
                                let text = format!(
                                    "Recipe loaded as an immutable snapshot: {}",
                                    presentation::origin(preset.description().origin())
                                );
                                self.imported = Some(preset);
                                if self.selection_revision == revision {
                                    self.select(self.catalog.len());
                                }
                                text
                            }
                            Outcome::Inspected(facts) => presentation::facts(&facts),
                            Outcome::Exported(report) => format!(
                                "Export completed\nSource: {}\nDestination: {:?}\n{} frames at {} Hz",
                                presentation::origin(report.origin()),
                                report.destination(),
                                report.exported().frames,
                                report.exported().sample_rate.hz()
                            ),
                            Outcome::Played(report) => format!(
                                "Playback completed\nSource: {}\n{} frames at {} Hz\n{}",
                                presentation::origin(report.origin()),
                                report.played().frames,
                                report.played().sample_rate.hz(),
                                report
                                    .notices()
                                    .iter()
                                    .map(presentation::notice)
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            ),
                        };
                        self.last_result = self.activity.clone();
                    }
                    Err(error) => {
                        self.state = if error.class() == ErrorClass::Cancelled {
                            State::Cancelled
                        } else {
                            State::Failed
                        };
                        self.activity = presentation::error(&error);
                    }
                }
            }
        }
    }
    pub(crate) fn grant_start_after_ui_apply(&mut self) -> bool {
        if matches!(self.state, State::Preparing)
            && !self.pending_close
            && let Some(active) = &mut self.active
            && !active.terminal
            && active.prepared
            && !active.worker.cancellation.is_requested()
            && let Some(sender) = &active.worker.permit
        {
            match sender.try_send(()) {
                Ok(()) => self.state = State::Running,
                Err(error) => self.fail(format!("Cannot permit the prepared operation: {error}")),
            }
            return true;
        }
        false
    }
    fn drain(&mut self) -> bool {
        let mut changed = false;
        loop {
            let message = match self.active.as_ref().map(|a| a.worker.messages.try_recv()) {
                Some(Ok(message)) => message,
                Some(Err(TryRecvError::Empty | TryRecvError::Disconnected)) | None => break,
            };
            self.receive(message);
            changed = true;
        }
        changed
    }
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = self.drain();
        if self.active.as_ref().is_some_and(|a| a.worker.finished()) {
            self.drain();
            changed = true;
            if self.active.as_ref().is_some_and(|active| active.playback) {
                self.signal.finish();
            }
            if let Some(mut active) = self.active.take() {
                let joined = active.worker.join();
                if let Err(error) = joined {
                    self.fail(format!(
                        "{error}\nRequested destination: {:?}",
                        active.destination
                    ));
                } else if !active.terminal {
                    self.fail(format!("Worker ended without a terminal result. Check destination {:?}; no cancellation or file-cleanup outcome can be inferred.",active.destination));
                }
            }
        }
        changed
    }
    pub(crate) fn shutdown_after_event_loop(&mut self) {
        if let Some(active) = &mut self.active {
            active.worker.stop();
            let joined = active.worker.join();
            self.drain();
            if let Some(active) = self.active.take() {
                if let Err(error) = joined {
                    self.fail(format!(
                        "{error}\nRequested destination: {:?}",
                        active.destination
                    ));
                } else if !active.terminal {
                    self.fail(format!("Worker ended without a terminal result. Check destination {:?}; no cleanup outcome can be inferred.", active.destination));
                }
            }
        }
    }
    pub(crate) fn visible_activity(&self) -> String {
        let execution = if !self.last_result.is_empty() && self.activity != self.last_result {
            format!(
                "{}\n\nPrevious completed result:\n{}",
                self.activity, self.last_result
            )
        } else {
            self.activity.clone()
        };
        match &self.presentation_notification {
            Some(notification) => {
                format!("Presentation notification:\n{notification}\n\n{execution}")
            }
            None => execution,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab::{self as app, Cancellation, Origin, SampleRate};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    struct Fake {
        send: mpsc::SyncSender<Message>,
        release: Option<mpsc::Sender<()>>,
        permit: mpsc::Receiver<()>,
    }
    fn install(
        state: &mut Controller,
        id: u64,
        cancellable: bool,
        destination: Option<PathBuf>,
    ) -> Fake {
        let (send, messages) = mpsc::sync_channel(2);
        let (permit, permit_rx) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _ = waiting.recv();
        });
        state.active = Some(Active {
            id,
            worker: Worker {
                messages,
                permit: Some(permit),
                cancellation: Cancellation::default(),
                handle: Some(handle),
            },
            cancellable,
            destination,
            selection_revision: state.selection_revision,
            terminal: false,
            prepared: false,
            playback: false,
        });
        state.state = State::Preparing;
        Fake {
            send,
            release: Some(release),
            permit: permit_rx,
        }
    }
    fn settle(state: &mut Controller, fake: &mut Fake) {
        fake.release.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        while state.active.is_some() {
            state.poll();
            assert!(Instant::now() < deadline, "worker not joined");
            std::thread::yield_now();
        }
    }
    fn preset() -> LoadedPreset {
        app::load_bytes(
            b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
            Origin::Reader("test snapshot".into()),
        )
        .unwrap()
    }
    fn wait_until(state: &mut Controller, ready: impl Fn(&Controller) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready(state) {
            state.poll();
            assert!(Instant::now() < deadline, "real operation failed to settle");
            std::thread::yield_now();
        }
    }
    #[test]
    fn about_failure_cannot_strand_real_export_at_any_execution_phase() {
        for phase in 0..3 {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("silent.wav");
            let mut state = Controller::default();
            assert!(state.start(Job::Export(
                Input::Imported(preset()),
                SampleRate::DEFAULT,
                path.clone()
            )));
            if phase != 0 {
                wait_until(&mut state, |s| s.active.as_ref().unwrap().prepared);
            }
            if phase == 2 {
                assert!(state.grant_start_after_ui_apply());
            }
            let execution_phase = state.state;
            state.notify_presentation_error("Cannot open About: injected window failure".into());
            assert_eq!(state.state, execution_phase);
            if phase == 0 {
                wait_until(&mut state, |s| s.active.as_ref().unwrap().prepared);
            }
            if phase != 2 {
                assert_eq!(state.state, State::Preparing);
                assert!(!path.exists(), "export bypassed the prepared permit");
                assert!(state.grant_start_after_ui_apply());
            }
            wait_until(&mut state, |s| s.active.is_none());
            assert_eq!(state.state, State::Completed);
            assert!(path.is_file());
            assert!(state.visible_activity().contains("Cannot open About"));
            assert!(state.visible_activity().contains("Export completed"));
            assert!(state.activity.contains("480 frames"));
            assert!(state.request_close());
        }
    }
    #[test]
    fn about_failure_preserves_real_export_stop_close_and_join() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cancelled.wav");
        let mut state = Controller::default();
        assert!(state.start(Job::Export(
            Input::Imported(preset()),
            SampleRate::DEFAULT,
            path.clone()
        )));
        wait_until(&mut state, |s| s.active.as_ref().unwrap().prepared);
        state.notify_presentation_error("Cannot open About: injected window failure".into());
        state.stop();
        assert_eq!(state.state, State::Stopping);
        assert!(!state.request_close());
        assert!(!state.grant_start_after_ui_apply());
        wait_until(&mut state, |s| s.active.is_none());
        assert_eq!(state.state, State::Cancelled);
        assert!(state.close_ready());
        assert!(!path.exists());
        assert!(state.visible_activity().contains("Cannot open About"));
        assert!(state.visible_activity().contains("cancel"));
    }
    #[cfg(unix)]
    #[test]
    fn fifo_import_close_joins_after_typed_rejection() {
        const CHILD_PATH: &str = "AUDITORY_CONTROLLER_FIFO_CHILD";
        if let Some(path) = std::env::var_os(CHILD_PATH) {
            let mut state = Controller::default();
            assert!(state.start(Job::Import(path.into())));
            assert!(!state.request_close());
            wait_until(&mut state, |s| s.active.is_none());
            assert_eq!(state.state, State::Failed);
            assert!(state.activity.contains("regular file"));
            assert!(state.close_ready());
            return;
        }
        use std::{
            io::Read,
            process::{Command, Stdio},
        };
        let directory = tempfile::tempdir().unwrap();
        let fifo = directory.path().join("recipe.pipe");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Child(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "controller::tests::fifo_import_close_joins_after_typed_rejection",
                    "--nocapture",
                ])
                .env(CHILD_PATH, fifo)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "FIFO import stranded close; child is killed and reaped"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut output = String::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut output)
            .unwrap();
        assert!(
            status.success(),
            "real FIFO import controller failed: {output}"
        );
    }
    #[test]
    fn preparation_requires_ui_application_then_matching_permit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.wav");
        let prepared = preset().prepare_export(SampleRate::DEFAULT, &path).unwrap();
        let mut state = Controller::default();
        let mut fake = install(&mut state, 7, true, Some(path.clone()));
        assert!(!state.start(Job::Catalog));
        state.grant_start_after_ui_apply();
        assert!(fake.permit.try_recv().is_err());
        state.receive(Message {
            id: 6,
            event: Event::Prepared(Preparation::Export(prepared.view().clone())),
        });
        state.grant_start_after_ui_apply();
        assert!(fake.permit.try_recv().is_err());
        state.receive(Message {
            id: 7,
            event: Event::Prepared(Preparation::Export(prepared.view().clone())),
        });
        assert!(state.activity.contains("new.wav"));
        assert!(fake.permit.try_recv().is_err());
        // The binding applies controller values to widgets before calling this.
        state.grant_start_after_ui_apply();
        assert!(fake.permit.try_recv().is_ok());
        assert_eq!(state.state, State::Running);
        let cancel = Cancellation::default();
        cancel.request();
        let result = prepared.execute(&cancel).map(Outcome::Exported);
        fake.send
            .send(Message {
                id: 7,
                event: Event::Finished(result),
            })
            .unwrap();
        settle(&mut state, &mut fake);
        assert_eq!(state.state, State::Cancelled);
        assert!(!path.exists());
    }
    #[test]
    fn close_during_discovery_permit_or_running_never_joins_early() {
        for phase in [State::Preparing, State::Running, State::Stopping] {
            let mut state = Controller::default();
            let mut fake = install(&mut state, 4, true, Some("bound.wav".into()));
            state.state = phase;
            assert!(!state.request_close());
            assert!(
                state
                    .active
                    .as_ref()
                    .unwrap()
                    .worker
                    .cancellation
                    .is_requested()
            );
            assert!(!state.close_ready());
            state.poll();
            assert!(state.active.is_some());
            assert!(!state.close_ready());
            assert!(fake.permit.try_recv().is_err());
            let cancel = Cancellation::default();
            cancel.request();
            let dir = tempfile::tempdir().unwrap();
            let result = preset()
                .prepare_export(SampleRate::DEFAULT, &dir.path().join("none.wav"))
                .unwrap()
                .execute(&cancel)
                .map(Outcome::Exported);
            fake.send
                .send(Message {
                    id: 4,
                    event: Event::Finished(result),
                })
                .unwrap();
            state.poll();
            assert_eq!(state.state, State::Cancelled);
            assert!(!state.close_ready());
            settle(&mut state, &mut fake);
            assert!(state.close_ready());
        }
    }
    #[test]
    fn committed_success_survives_stop_selection_change_and_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("complete.wav");
        let report = preset()
            .prepare_export(SampleRate::DEFAULT, &path)
            .unwrap()
            .execute(&Cancellation::default())
            .unwrap();
        let mut state = Controller {
            catalog: app::catalog().unwrap(),
            ..Controller::default()
        };
        state.select(0);
        let original = state
            .selection
            .as_ref()
            .unwrap()
            .description
            .origin()
            .clone();
        let mut fake = install(&mut state, 9, true, Some(path.clone()));
        state.state = State::Running;
        state.select(1);
        assert_ne!(
            state.selection.as_ref().unwrap().description.origin(),
            &original
        );
        state.stop();
        fake.send
            .send(Message {
                id: 9,
                event: Event::Finished(Ok(Outcome::Exported(report))),
            })
            .unwrap();
        state.poll();
        assert_eq!(state.state, State::Completed);
        assert!(state.activity.contains("complete.wav"));
        let selected = state
            .selection
            .as_ref()
            .unwrap()
            .description
            .origin()
            .clone();
        state.receive(Message {
            id: 8,
            event: Event::Finished(Ok(Outcome::Imported(preset()))),
        });
        assert_eq!(
            state.selection.as_ref().unwrap().description.origin(),
            &selected
        );
        assert!(!state.request_close());
        settle(&mut state, &mut fake);
        assert!(state.close_ready());
        assert!(path.exists());
        assert!(state.visible_activity().contains("complete.wav"));
    }
    #[test]
    fn failed_import_and_delayed_import_keep_current_selection_and_separate_context() {
        let mut state = Controller {
            catalog: app::catalog().unwrap(),
            ..Controller::default()
        };
        state.select(0);
        let before = state
            .selection
            .as_ref()
            .unwrap()
            .description
            .origin()
            .clone();
        let mut fake = install(&mut state, 11, false, None);
        let error = app::load_file("missing-source.toml").unwrap_err();
        fake.send
            .send(Message {
                id: 11,
                event: Event::Finished(Err(error)),
            })
            .unwrap();
        settle(&mut state, &mut fake);
        assert_eq!(
            state.selection.as_ref().unwrap().description.origin(),
            &before
        );
        assert!(state.activity.contains("missing-source.toml"));
        let mut fake = install(&mut state, 12, false, None);
        state.select(1);
        let chosen = state
            .selection
            .as_ref()
            .unwrap()
            .description
            .origin()
            .clone();
        fake.send
            .send(Message {
                id: 12,
                event: Event::Finished(Ok(Outcome::Imported(preset()))),
            })
            .unwrap();
        settle(&mut state, &mut fake);
        assert_eq!(
            state.selection.as_ref().unwrap().description.origin(),
            &chosen
        );
        assert!(state.imported.is_some());
        assert!(state.activity.contains("test snapshot"));
    }
    #[test]
    fn idle_and_native_dialog_waits_need_no_polling_but_join_wait_does() {
        let mut state = Controller::default();
        assert!(!state.polling_needed());
        state.dialog = true;
        assert!(!state.polling_needed());
        state.dialog = false;
        let mut fake = install(&mut state, 18, false, None);
        assert!(state.polling_needed());
        fake.send
            .send(Message {
                id: 18,
                event: Event::Finished(Ok(Outcome::Imported(preset()))),
            })
            .unwrap();
        state.poll();
        assert!(state.polling_needed());
        settle(&mut state, &mut fake);
        assert!(!state.polling_needed());
    }
    #[test]
    fn playback_locks_source_until_worker_join_and_stopped_selection_clears_display_mode() {
        let mut state = Controller {
            catalog: app::catalog().unwrap(),
            ..Controller::default()
        };
        state.select(0);
        let source = state
            .selection
            .as_ref()
            .unwrap()
            .description
            .origin()
            .clone();
        let mut fake = install(&mut state, 24, true, None);
        state.active.as_mut().unwrap().playback = true;
        state.state = State::Running;
        assert_eq!(state.state_label(), "Playing");
        state.select(1);
        assert_eq!(
            state.selection.as_ref().unwrap().description.origin(),
            &source
        );
        state.stop();
        assert!(state.selection_locked());
        fake.send
            .send(Message {
                id: 24,
                event: Event::Finished(Ok(Outcome::Inspected(
                    preset().inspect(SampleRate::DEFAULT).unwrap(),
                ))),
            })
            .unwrap();
        state.poll();
        assert!(
            state.selection_locked(),
            "terminal receipt alone is not worker settlement"
        );
        settle(&mut state, &mut fake);
        assert!(!state.selection_locked());
        state.signal.frozen = true;
        state.signal.reduced_motion = true;
        state.select(1);
        assert!(!state.signal.frozen);
        assert!(state.signal.reduced_motion);
        assert!(state.signal.snapshot.is_none());
        assert!(state.signal.progress.is_none());
        assert_ne!(
            state.selection.as_ref().unwrap().description.origin(),
            &source
        );
    }
    #[test]
    fn ended_worker_without_result_and_native_dialog_close_are_not_clean_cancellation() {
        let mut state = Controller::default();
        let mut fake = install(&mut state, 3, true, Some("check-me.wav".into()));
        settle(&mut state, &mut fake);
        assert_eq!(state.state, State::Failed);
        assert!(
            state.activity.contains("check-me.wav")
                && state.activity.contains("without a terminal result")
        );
        state.dialog = true;
        assert!(!state.request_close());
        state.dialog_finished(None, false);
        assert!(state.close_ready());
        assert!(state.activity.contains("No selection"));
    }
    #[test]
    fn panic_is_an_explicit_failure_and_event_loop_shutdown_joins() {
        let mut state = Controller::default();
        let mut fake = install(&mut state, 5, true, Some("check-me.wav".into()));
        let original = state.active.as_mut().unwrap().worker.handle.take().unwrap();
        fake.release.take();
        original.join().unwrap();
        state.active.as_mut().unwrap().worker.handle =
            Some(std::thread::spawn(|| panic!("deliberate worker failure")));
        let deadline = Instant::now() + Duration::from_secs(10);
        while state.active.is_some() {
            state.poll();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(state.state, State::Failed);
        assert!(state.activity.contains("panicked"));
        let mut fake = install(&mut state, 6, true, None);
        fake.release.take();
        state.shutdown_after_event_loop();
        assert!(state.active.is_none());
    }
}
