// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Workspace orchestration through its public commands, with scripted audio
//! workers in place of a device and controllable file workers.
use super::tasks::Worker;
use super::*;
use crate::session::{
    LiveGenerator, MonitorDetail, OutputInfo, PlaybackMonitor, SampleFormat, SessionDescriptor,
};
use auditorylab_core::ParameterId;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub(super) type AudioSpawner =
    dyn FnMut(
        Session,
        AudioKind,
        Option<Draft>,
    ) -> Worker<AudioEvent, Result<crate::session::PlaybackReport, Error>>;

/// Steps a scripted audio worker performs, in order.
enum Step {
    /// Validate at this device rate and report Ready, or fail.
    Prepare(SampleRate),
    /// End playback: Stopped if Stop was accepted, otherwise Completed.
    Finish,
    /// Report a playback failure.
    Fail,
    /// Exit the thread.
    Exit,
}

#[derive(Clone)]
struct Fake {
    control: crate::PlaybackControl,
    steps: Sender<Step>,
}

#[derive(Clone, Default)]
struct Fakes(Arc<Mutex<Vec<Fake>>>);

impl Fakes {
    fn last(&self) -> Fake {
        self.0
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("an audio worker started")
    }
    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

fn scripted(fakes: &Fakes) -> Box<AudioSpawner> {
    let fakes = fakes.clone();
    Box::new(move |session, kind, draft| {
        let (steps, script): (Sender<Step>, Receiver<Step>) = mpsc::channel();
        fakes.0.lock().unwrap().push(Fake {
            control: session.control(),
            steps,
        });
        Worker::spawn(move |tx| {
            let state = Arc::clone(session.state());
            let mut generator = None;
            for step in script {
                match step {
                    Step::Prepare(rate) => {
                        if state.outcome() == crate::session::Outcome::StopRequested {
                            state.finish_drained(crate::session::Outcome::Completed);
                            let _ = tx.send(AudioEvent::Finished(Ok(state.report().unwrap())));
                            continue;
                        }
                        let target = match (&kind, &draft) {
                            (AudioKind::Live(_), Some(draft)) => {
                                draft.target_check(rate).into_result()
                            }
                            (AudioKind::Finite(snapshot), _) => snapshot.stimulus().target(rate),
                            _ => unreachable!(),
                        };
                        let target = match target {
                            Ok(target) => target,
                            Err(e) => {
                                state.finish(crate::session::Outcome::PreparationFailed);
                                let _ = tx.send(AudioEvent::Finished(Err(Error::new(
                                    ErrorKind::InvalidSound,
                                    e.to_string(),
                                ))));
                                continue;
                            }
                        };
                        let live = LiveGenerator::with_state(target, Arc::clone(&state));
                        let monitor = PlaybackMonitor::new(
                            SessionDescriptor {
                                id: state.id(),
                                title: "scripted".into(),
                                origin: crate::Origin::Stream("scripted".into()),
                                output: OutputInfo {
                                    name: None,
                                    channels: 2,
                                    sample_rate: rate,
                                    format: SampleFormat::F32,
                                },
                                frames: None,
                                target,
                            },
                            MonitorDetail::Quantities,
                            Arc::clone(&state),
                            Some(Arc::clone(live.mailbox())),
                        );
                        let control = matches!(kind, AudioKind::Live(_)).then(|| live.control());
                        let _ = tx.send(AudioEvent::Prepared {
                            reader: monitor.reader(),
                            rate,
                            live: control,
                        });
                        generator = Some(live);
                    }
                    Step::Finish => {
                        state.finish_drained(crate::session::Outcome::Completed);
                        let _ = tx.send(AudioEvent::Finished(Ok(state.report().unwrap())));
                    }
                    Step::Fail => {
                        state.finish(crate::session::Outcome::Backend);
                        let _ = tx.send(AudioEvent::Finished(Err(Error::new(
                            ErrorKind::Device,
                            "scripted failure",
                        ))));
                    }
                    Step::Exit => break,
                }
            }
            drop(generator);
            drop(session);
        })
    })
}

fn workspace() -> (Workspace, Fakes) {
    let mut ws = Workspace::new(crate::catalog().unwrap(), Capabilities::available()).unwrap();
    ws.capabilities.device_playback = true;
    let fakes = Fakes::default();
    ws.audio_spawner = Some(scripted(&fakes));
    (ws, fakes)
}

fn settle(ws: &mut Workspace, until: impl Fn(&Workspace) -> bool) {
    for _ in 0..5_000 {
        ws.poll();
        if until(ws) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("workspace did not settle");
}

/// A workbench document with one tone.
fn document(ws: &mut Workspace, frequency: f64) -> (DocumentId, FieldHandle) {
    ws.set_view(View::Workbench);
    let document = ws.editor().document();
    ws.add_component(document, CarrierKind::Tone).unwrap();
    let id = ws.editor().draft().components()[0].0;
    let handle = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ToneFrequency));
    ws.commit_field(handle, &frequency.to_string()).unwrap();
    (document, handle)
}

/// A file worker that sends `outcome` when released, then exits when released again.
fn held_file(ws: &mut Workspace, kind: FileKind, outcome: FileOutcome) -> (Sender<()>, TaskId) {
    let (release, wait) = mpsc::channel::<()>();
    let id = TaskId(ws.next_id());
    let worker = Worker::spawn(move |tx| {
        wait.recv().unwrap();
        let _ = tx.send(outcome);
        let _ = wait.recv();
    });
    ws.file = Some(FileTask { id, kind, worker });
    (release, id)
}

fn export_kind() -> FileKind {
    FileKind::Export {
        title: "export".into(),
        path: "unused.wav".into(),
        frames: 1,
        cancel: Cancel::default(),
        progress: Progress::default(),
    }
}

#[test]
fn the_admission_table_has_one_audio_and_one_file_slot() {
    let (mut ws, fakes) = workspace();
    document(&mut ws, 440.0);
    ws.play().unwrap();
    assert!(ws.audio_busy());
    // Save may run beside audio; export may not.
    assert_eq!(ws.export(), Err(Refusal::AudioPlaying));
    ws.save().unwrap();
    let effects = ws.take_effects();
    assert!(matches!(
        effects[0],
        Effect::ChoosePath {
            purpose: PathPurpose::SaveRecipe,
            ..
        }
    ));
    let Effect::ChoosePath { prompt, .. } = effects[0] else {
        unreachable!()
    };
    ws.respond(prompt, Response::Path(None));
    // A file task blocks a second file task, not audio.
    let (release, _) = held_file(
        &mut ws,
        export_kind(),
        FileOutcome::Exported(Err(Error::new(ErrorKind::Cancelled, "x"))),
    );
    assert_eq!(ws.import(), Err(Refusal::FileBusy));
    // Stop never cancels the file task; cancelling the export never stops audio.
    assert_eq!(ws.stop(), Ok(StopRequest::Accepted));
    let FileKind::Export { cancel, .. } = &ws.file.as_ref().unwrap().kind else {
        unreachable!()
    };
    assert!(!cancel.is_requested());
    let fake = fakes.last();
    fake.steps.send(Step::Finish).unwrap();
    fake.steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    // While exporting, audio cannot start.
    ws.set_view(View::Library);
    assert_eq!(ws.play(), Err(Refusal::Exporting));
    ws.cancel_export().unwrap();
    assert!(ws.audio.is_none());
    release.send(()).unwrap();
    release.send(()).unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    assert_eq!(ws.cancel_export(), Err(Refusal::NoExport));
    assert_eq!(ws.stop(), Err(Refusal::NoAudio));
}

#[test]
fn a_received_result_keeps_its_slot_finishing_until_the_thread_is_joined() {
    let (mut ws, _) = workspace();
    let (release, _) = held_file(
        &mut ws,
        FileKind::Import("unused.toml".into()),
        FileOutcome::Imported(Err(Error::new(ErrorKind::Unreadable, "scripted"))),
    );
    ws.close();
    assert!(ws.action_pending(), "close waits for the file task");
    release.send(()).unwrap();
    for _ in 0..50 {
        ws.poll();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        ws.file.as_ref().unwrap().worker.finishing(),
        "result received"
    );
    assert!(ws.file_busy(), "the slot stays occupied until the join");
    assert!(
        ws.take_effects().is_empty(),
        "no dependent continuation yet"
    );
    assert_eq!(ws.import(), Err(Refusal::ActionPending));
    release.send(()).unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    // Close depended on that task, so its actual failure denies the close.
    let effects = ws.take_effects();
    assert_eq!(effects, [Effect::CloseResolved { approved: false }]);
    assert!(ws.action_view().file_status.contains("scripted"));
}

#[test]
fn closing_cancels_an_export_and_treats_the_cancellation_as_cleanup() {
    let (mut ws, _) = workspace();
    let (release, _) = held_file(
        &mut ws,
        export_kind(),
        FileOutcome::Exported(Err(Error::new(ErrorKind::Cancelled, "export cancelled"))),
    );
    ws.close();
    let FileKind::Export { cancel, .. } = &ws.file.as_ref().unwrap().kind else {
        unreachable!()
    };
    assert!(cancel.is_requested(), "close requests export cancellation");
    release.send(()).unwrap();
    release.send(()).unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    assert_eq!(
        ws.take_effects(),
        [Effect::CloseResolved { approved: true }]
    );
}

#[test]
fn an_unrelated_file_failure_does_not_cancel_audio_replacement() {
    let (mut ws, fakes) = workspace();
    ws.play().unwrap();
    let first = fakes.last();
    let document = ws.editor().document();
    let (release, _) = held_file(
        &mut ws,
        FileKind::Save {
            document,
            content: Draft::new().recipe(),
            path: "unused.toml".into(),
        },
        FileOutcome::Saved(Err(Error::new(ErrorKind::Write, "disk full"))),
    );
    ws.play().unwrap();
    assert!(
        ws.action_pending(),
        "replacement waits for the first audio task"
    );
    assert_eq!(first.control.phase(), PlaybackPhase::Stopping);
    release.send(()).unwrap();
    release.send(()).unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    assert!(ws.action_pending(), "the file failure is not a dependency");
    assert!(ws.action_view().file_status.contains("disk full"));
    first.steps.send(Step::Finish).unwrap();
    first.steps.send(Step::Exit).unwrap();
    settle(&mut ws, |_| fakes.count() == 2);
    assert!(ws.audio_busy(), "replacement audio started");
    let second = fakes.last();
    second.steps.send(Step::Exit).unwrap();
}

#[test]
fn a_panic_after_publication_cannot_authorize_save_and_close() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let target = dir.path().join("saved.toml");
    let (mut ws, _) = workspace();
    let (_, handle) = document(&mut ws, 440.0);
    let _ = handle;
    ws.close();
    let Effect::AskUnsaved(prompt) = ws.take_effects()[0] else {
        panic!("unsaved prompt")
    };
    ws.respond(prompt, Response::Save);
    let Effect::ChoosePath { prompt, .. } = ws.take_effects()[0] else {
        panic!("save panel")
    };
    // Replace the real save with one that publishes, reports and then panics.
    let document = ws.editor().document();
    let content = ws.editor().draft().recipe();
    ws.respond(prompt, Response::Path(Some(target.clone())));
    let task = ws.file.take().unwrap();
    // Let the real worker finish so its thread is not left behind.
    let mut real = task.worker;
    while !matches!(real.poll(Some), tasks::Polled::Joined { .. }) {
        std::thread::yield_now();
    }
    let worker = Worker::spawn(|tx| {
        let _ = tx.send(FileOutcome::Saved(Ok(())));
        panic!("injected failure after publication");
    });
    ws.file = Some(FileTask {
        id: task.id,
        kind: FileKind::Save {
            document,
            content,
            path: target,
        },
        worker,
    });
    settle(&mut ws, |ws| !ws.file_busy());
    assert_eq!(
        ws.take_effects(),
        [Effect::CloseResolved { approved: false }]
    );
    assert!(!ws.editor().dirty(), "publication truth is recorded");
    assert!(
        ws.action_view()
            .file_status
            .contains(presentation::WORKER_FAILED)
    );
}

#[test]
fn one_prompt_coalesced_quit_stale_answers_and_failed_panels() {
    let (mut ws, fakes) = workspace();
    document(&mut ws, 440.0);
    ws.play().unwrap();
    ws.close();
    ws.close();
    let effects = ws.take_effects();
    assert_eq!(effects.len(), 1, "repeated Quit coalesces: {effects:?}");
    let Effect::AskUnsaved(prompt) = effects[0] else {
        panic!()
    };
    ws.new_document();
    assert!(
        ws.take_effects().is_empty(),
        "conflicting commands are rejected"
    );
    assert!(ws.action_view().file_status.contains("dialog"));
    assert_eq!(ws.stop(), Ok(StopRequest::Accepted), "Stop stays available");
    ws.respond(PromptId(prompt.0 + 1000), Response::Discard);
    assert!(ws.prompt_open(), "a stale answer changes nothing");
    ws.respond(prompt, Response::Failed("panel unavailable".into()));
    assert!(!ws.prompt_open(), "a failed panel resolves the prompt");
    assert_eq!(
        ws.take_effects(),
        [Effect::CloseResolved { approved: false }]
    );
    let fake = fakes.last();
    fake.steps.send(Step::Exit).unwrap();
}

#[test]
fn a_live_preparation_binds_committed_edits_at_the_actual_rate_only() {
    let (mut ws, fakes) = workspace();
    let (document, frequency) = document(&mut ws, 440.0);
    ws.play().unwrap();
    // During preparation: commit B, then stage C.
    ws.commit_field(frequency, "30000").unwrap();
    ws.stage(frequency, "500").unwrap();
    let fake = fakes.last();
    fake.steps
        .send(Step::Prepare(SampleRate::new(96_000).unwrap()))
        .unwrap();
    settle(&mut ws, |ws| ws.live_rate().is_some());
    let accepted = ws
        .audio
        .as_ref()
        .unwrap()
        .live
        .as_ref()
        .unwrap()
        .accepted()
        .unwrap();
    assert_eq!(accepted.0, 2, "committed B was submitted");
    assert_eq!(accepted.1.sample_rate().hz(), 96_000);
    let (_, facts) = accepted.1.components().next().unwrap();
    let auditorylab_core::CarrierFacts::Tone(rate) = facts.carrier else {
        panic!()
    };
    assert_eq!(rate.requested_hz, 30_000.0, "staged C was never sent");
    assert_eq!(ws.editor().staged(frequency.field), Some("500"));
    // Diagnostics use the live rate; the export rate selection is unchanged.
    assert_eq!(ws.diagnostic_rate().hz(), 96_000);
    assert_eq!(ws.export_rate(), SampleRate::DEFAULT);
    assert!(
        !ws.action_view().can_export,
        "30 kHz is invalid for 48 kHz export"
    );
    // An invalid committed edit keeps the last valid accepted target.
    ws.commit_field(frequency, "60000").unwrap();
    let still = ws
        .audio
        .as_ref()
        .unwrap()
        .live
        .as_ref()
        .unwrap()
        .accepted()
        .unwrap();
    assert_eq!(still.0, 2);
    // Stop closes admission; later commits are not accepted.
    ws.stop().unwrap();
    ws.commit_field(frequency, "440").unwrap();
    let closed = ws
        .audio
        .as_ref()
        .unwrap()
        .live
        .as_ref()
        .unwrap()
        .accepted()
        .unwrap();
    assert_eq!(closed.0, 2);
    let _ = document;
    fake.steps.send(Step::Finish).unwrap();
    fake.steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    assert!(ws.action_view().audio_status.starts_with("Stopped"));
}

#[test]
fn device_rate_validation_is_independent_of_the_export_rate() {
    let (mut ws, fakes) = workspace();
    document(&mut ws, 30_000.0);
    // Selected 48 kHz export: export invalid, preparation still eligible.
    let actions = ws.action_view();
    assert!(!actions.can_export);
    assert!(actions.can_play, "{}", actions.play_reason);
    ws.play().unwrap();
    fakes
        .last()
        .steps
        .send(Step::Prepare(SampleRate::new(96_000).unwrap()))
        .unwrap();
    settle(&mut ws, |ws| ws.live_rate().is_some());
    fakes.last().steps.send(Step::Finish).unwrap();
    fakes.last().steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    // Selected 96 kHz export, actual 48 kHz device: preparation fails before streaming.
    ws.set_export_rate(SampleRate::new(96_000).unwrap());
    assert!(ws.action_view().can_export);
    ws.play().unwrap();
    fakes
        .last()
        .steps
        .send(Step::Prepare(SampleRate::new(48_000).unwrap()))
        .unwrap();
    fakes.last().steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    assert!(ws.action_view().audio_status.contains("half the 48000 Hz"));
    assert_eq!(
        ws.export_rate().hz(),
        96_000,
        "device attempts never change the selection"
    );
}

#[test]
fn stop_before_preparation_never_revives_playback() {
    let (mut ws, fakes) = workspace();
    document(&mut ws, 440.0);
    ws.play().unwrap();
    ws.stop().unwrap();
    fakes
        .last()
        .steps
        .send(Step::Prepare(SampleRate::DEFAULT))
        .unwrap();
    fakes.last().steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    assert!(ws.output.reader().is_none(), "no Ready, no monitor binding");
    assert!(ws.action_view().audio_status.starts_with("Stopped"));
}

#[test]
fn a_late_preparation_for_a_replaced_document_attaches_monitoring_only() {
    let (mut ws, fakes) = workspace();
    let (old, frequency) = document(&mut ws, 440.0);
    ws.play().unwrap();
    // Replace the document while preparation runs (Discard the unsaved copy).
    ws.new_document();
    let Effect::AskUnsaved(prompt) = ws.take_effects()[0] else {
        panic!()
    };
    ws.respond(prompt, Response::Discard);
    assert_ne!(ws.editor().document(), old);
    assert_eq!(
        ws.commit_field(frequency, "880"),
        Err(Refusal::Edit(EditRefusal::StaleDocument))
    );
    fakes
        .last()
        .steps
        .send(Step::Prepare(SampleRate::DEFAULT))
        .unwrap();
    settle(&mut ws, |ws| ws.output.reader().is_some());
    assert_eq!(
        ws.live_rate(),
        None,
        "the new document is not bound to the old session"
    );
    let accepted = ws
        .audio
        .as_ref()
        .unwrap()
        .live
        .as_ref()
        .unwrap()
        .accepted()
        .unwrap();
    assert_eq!(accepted.0, 1, "nothing from the new document was submitted");
    fakes.last().steps.send(Step::Fail).unwrap();
    fakes.last().steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
    assert!(ws.action_view().audio_status.contains("scripted failure"));
}

#[test]
fn saving_records_only_the_captured_snapshot_for_its_own_document() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let (mut ws, _) = workspace();
    let (_, frequency) = document(&mut ws, 440.0);
    ws.save().unwrap();
    let Effect::ChoosePath { prompt, .. } = ws.take_effects()[0] else {
        panic!()
    };
    // The open panel blocks editing; edits after the choice do not reach the file.
    assert_eq!(ws.commit_field(frequency, "880"), Err(Refusal::PromptOpen));
    let path = dir.path().join("a.toml");
    ws.respond(prompt, Response::Path(Some(path.clone())));
    ws.commit_field(frequency, "880").unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    let saved = crate::load_file(&path).unwrap();
    assert_eq!(
        ParameterId::ToneFrequency.read(&saved.stimulus().components[0]),
        Some(440.0),
        "only the captured snapshot is written"
    );
    assert!(ws.editor().dirty(), "newer edits stay dirty");
    // A save completing after replacement leaves the replacement untouched.
    ws.save().unwrap();
    let Effect::ChoosePath { prompt, .. } = ws.take_effects()[0] else {
        panic!()
    };
    ws.respond(prompt, Response::Path(Some(dir.path().join("b.toml"))));
    ws.editor = EditorSession::new(Draft::new()).unwrap();
    settle(&mut ws, |ws| !ws.file_busy());
    assert!(!ws.editor().dirty());
    assert!(ws.action_view().file_status.contains("different document"));
}

#[test]
fn shutdown_joins_both_workers_and_reports_real_failures() {
    let (mut ws, fakes) = workspace();
    ws.play().unwrap();
    let fake = fakes.last();
    let (release, _) = held_file(
        &mut ws,
        FileKind::Import("unused.toml".into()),
        FileOutcome::Imported(Err(Error::new(ErrorKind::Unreadable, "unreadable import"))),
    );
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        fake.steps.send(Step::Finish).unwrap();
        fake.steps.send(Step::Exit).unwrap();
        release.send(()).unwrap();
        release.send(()).unwrap();
    });
    let result = ws.shutdown();
    stopper.join().unwrap();
    let errors = result.unwrap_err();
    assert!(errors.contains("unreadable import"), "{errors}");
    assert!(
        !errors.contains("Stopped"),
        "expected Stop is not a failure"
    );
}

#[test]
fn portable_workspaces_refuse_audio_before_taking_a_slot() {
    let mut ws = Workspace::new(crate::catalog().unwrap(), Capabilities::portable()).unwrap();
    assert_eq!(ws.play(), Err(Refusal::DevicePlaybackUnavailable));
    assert!(!ws.audio_busy());
    assert!(!ws.action_view().can_play);
    let (_, frequency) = document(&mut ws, 440.0);
    ws.commit_field(frequency, "880").unwrap();
    assert!(ws.action_view().can_save);
}

#[test]
fn idle_ticks_do_no_evaluation_or_preview_work() {
    let (mut ws, _) = workspace();
    document(&mut ws, 440.0);
    ws.poll();
    let view = ws.editor_view();
    let actions = ws.action_view();
    let previews = ws.output.previews;
    let models = ws.output.models;
    let evaluations = ws.editor.evaluations();
    let serializations = ws.editor.serializations();
    let revision = ws.revision();
    let capture = ws.output_view().capture.unwrap();
    let model = ws.model_view().unwrap();
    for _ in 0..100 {
        assert!(!ws.poll());
    }
    assert_eq!(ws.action_view(), actions);
    assert_eq!(ws.output.previews, previews);
    assert_eq!(ws.output.models, models);
    assert_eq!(ws.editor.evaluations(), evaluations);
    assert_eq!(
        ws.editor.serializations(),
        serializations,
        "save readiness is checked once per candidate"
    );
    assert!(Arc::ptr_eq(&model, &ws.model_view().unwrap()));
    assert_eq!(ws.revision(), revision);
    assert!(Arc::ptr_eq(&capture, &ws.output_view().capture.unwrap()));
    assert_eq!(ws.editor_view(), view);
    assert_eq!(
        ws.editor.evaluations(),
        evaluations,
        "views reuse cached evaluations"
    );
    // A scale change reuses the preview's samples.
    ws.set_waveform(40, WaveformScale::Fit);
    ws.poll();
    assert_eq!(ws.output.previews, previews);
    // A relevant change recomputes once.
    ws.set_waveform(10, WaveformScale::Fit);
    ws.poll();
    ws.poll();
    assert_eq!(ws.output.previews, previews + 1);
}

#[test]
fn invalid_commits_clear_unheld_previews_and_held_captures_stay_frozen() {
    let (mut ws, _) = workspace();
    let (_, frequency) = document(&mut ws, 440.0);
    ws.poll();
    let valid = ws.output_view().capture.unwrap();
    ws.stage(frequency, "30000").unwrap();
    ws.poll();
    assert!(
        Arc::ptr_eq(&valid, &ws.output_view().capture.unwrap()),
        "staging keeps the committed preview"
    );
    ws.set_hold(true);
    ws.commit().unwrap();
    ws.poll();
    let held = ws.output_view();
    assert!(held.held);
    assert!(
        Arc::ptr_eq(&valid, &held.capture.unwrap()),
        "held capture keeps its content"
    );
    ws.set_hold(false);
    ws.poll();
    let current = ws.output_view();
    assert!(
        current.capture.is_none(),
        "invalid committed source clears the preview"
    );
    assert!(current.unavailable.unwrap().contains("48000 Hz"));
    let evaluations = ws.editor.evaluations();
    for _ in 0..10 {
        ws.poll();
    }
    assert_eq!(
        ws.editor.evaluations(),
        evaluations,
        "invalid results are cached"
    );
}

#[test]
fn reimports_get_new_identities_and_based_on_details_use_the_retained_snapshot() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("same.toml");
    let first_text =
        "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle='First'\n";
    std::fs::write(&path, first_text).unwrap();
    let (mut ws, _) = workspace();
    for text in [
        first_text,
        "schema_version=2\nduration_seconds=2\nfade_seconds=0\n[about]\ntitle='Second'\n",
    ] {
        std::fs::write(&path, text).unwrap();
        ws.import().unwrap();
        let Effect::ChoosePath { prompt, .. } = ws.take_effects()[0] else {
            panic!()
        };
        ws.respond(prompt, Response::Path(Some(path.clone())));
        settle(&mut ws, |ws| !ws.file_busy());
        if text == first_text {
            ws.copy_selected();
            assert_eq!(ws.take_effects(), [Effect::ShowView(View::Workbench)]);
        }
    }
    let first_copy = ws.editor().draft().based_on().unwrap().clone();
    assert_eq!(first_copy.title(), "First");
    let current = ws.selected().unwrap().clone();
    assert_eq!(current.title(), "Second");
    assert_ne!(current.id(), first_copy.id(), "same path, new identity");
    assert_eq!(
        ws.catalog().len(),
        crate::catalog().unwrap().len() + 1,
        "row replaced in place"
    );
    ws.show_based_on_source();
    assert_eq!(
        ws.selected().unwrap().id(),
        first_copy.id(),
        "not resolved by path"
    );
    assert_eq!(ws.view(), View::Library);
}

#[test]
fn the_live_model_follows_accepted_revisions_even_for_identical_audio() {
    let (mut ws, fakes) = workspace();
    ws.set_view(View::Workbench);
    let document = ws.editor().document();
    ws.add_component(document, CarrierKind::Clicks).unwrap();
    let id = ws.editor().draft().components()[0].0;
    let width = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ClickWidth));
    ws.commit_field(width, "1").unwrap();
    ws.play().unwrap();
    let fake = fakes.last();
    fake.steps
        .send(Step::Prepare(SampleRate::new(8_000).unwrap()))
        .unwrap();
    settle(&mut ws, |ws| ws.live_rate().is_some());
    ws.poll();
    let first = ws.model_view().unwrap();
    let crate::ModelSource::Accepted(first_revision) = first.source else {
        panic!("a bound live document explains its accepted target");
    };
    let models = ws.output.models;
    for _ in 0..20 {
        ws.poll();
    }
    assert_eq!(
        ws.output.models, models,
        "unchanged revisions build nothing"
    );
    // 1.01 ms is still 8 frames at 8 kHz: identical samples, new definition.
    ws.commit_field(width, "1.01").unwrap();
    ws.poll();
    let second = ws.model_view().unwrap();
    let crate::ModelSource::Accepted(second_revision) = second.source else {
        panic!("still the accepted target");
    };
    assert!(second_revision > first_revision);
    assert!(second.target.same_audio(&first.target));
    assert_ne!(second.target, first.target);
    assert_eq!(ws.output.models, models + 1);
    fake.steps.send(Step::Finish).unwrap();
    fake.steps.send(Step::Exit).unwrap();
    settle(&mut ws, |ws| !ws.audio_busy());
}
