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
                            (AudioKind::Live(_), Some(draft)) => draft.check(rate).into_result(),
                            (AudioKind::Finite(snapshot), _) => snapshot.stimulus().target(rate),
                            (AudioKind::Managed { snapshot, store }, _) => {
                                tasks::revalidate_managed(snapshot, store)
                                    .unwrap()
                                    .stimulus()
                                    .target(rate)
                            }
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        ws.poll();
        if until(ws) {
            return;
        }
        std::thread::yield_now();
    }
    panic!("workspace did not settle");
}

/// A workbench document with one tone.
fn document(ws: &mut Workspace, frequency: f64) -> (DocumentId, FieldHandle) {
    ws.set_view(View::Workbench);
    let document = ws.editor().document();
    ws.add_component(document, CarrierKind::Tone).unwrap();
    let id = ws.editor().draft().components()[0].id;
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
        sound: Draft::new()
            .unwrap()
            .snapshot()
            .at(SampleRate::DEFAULT)
            .unwrap(),
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
            content: Arc::new(Draft::new().unwrap().recipe()),
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
    let (mut ws, execution) = controlled();
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
    ws.respond(prompt, Response::Path(Some(target.clone())));
    let worker = execution.file().unwrap();
    worker.result().unwrap();
    ws.poll();
    assert!(ws.file_busy());
    assert!(target.exists(), "the captured content really published");
    worker.panic().unwrap();
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
    assert!(ws.transport_view().player_title.starts_with("Stopped"));
    assert_eq!(
        ws.action_view().audio_status,
        "",
        "the transport names an ordinary Stop; the status line does not repeat it"
    );
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
        saved.stimulus().components[0]
            .node
            .read(ParameterId::ToneFrequency),
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
    ws.editor = EditorSession::new(Draft::new().unwrap()).unwrap();
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
    let first_text = "schema_version=3\nduration_seconds=1\nfade_seconds=0\n[description]\ntitle='First'\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n";
    std::fs::write(&path, first_text).unwrap();
    let (mut ws, _) = workspace();
    for text in [
        first_text,
        "schema_version=3\nduration_seconds=2\nfade_seconds=0\n[description]\ntitle='Second'\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n",
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
    let id = ws.editor().draft().components()[0].id;
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

#[test]
fn receipt_age_reads_in_human_units() {
    use std::time::Duration;
    for (millis, held, text) in [
        (120, false, "Received samples just now"),
        (12_495, false, "Received samples 12 s ago"),
        (125_000, false, "Received samples 2 min ago"),
        (7_300_000, true, "Current session received samples 2 h ago"),
    ] {
        assert_eq!(
            super::views::received_text(Duration::from_millis(millis), held),
            text
        );
    }
}

#[test]
fn a_search_that_hides_every_recipe_leaves_nothing_to_play_or_export() {
    let (mut ws, fakes) = workspace();
    let chosen = ws.selected().unwrap().id();
    ws.set_query("no recipe has this text");
    assert!(ws.selected().is_none());
    assert_eq!(ws.play(), Err(Refusal::NoSelection));
    assert_eq!(ws.export(), Err(Refusal::NoSelection));
    assert_eq!(fakes.count(), 0, "no audio task started");
    assert!(!ws.action_view().can_play);
    assert!(ws.library_view().selected.is_none());
    ws.copy_selected();
    assert_eq!(ws.view(), View::Library, "nothing to copy");
    ws.set_query("");
    assert_eq!(
        ws.selected().unwrap().id(),
        chosen,
        "clearing restores the choice"
    );
}

#[test]
fn suggested_file_names_come_from_titles() {
    assert_eq!(
        super::file_name("5 Hz binaural beat (edited)", "toml"),
        "5 Hz binaural beat (edited).toml"
    );
    assert_eq!(super::file_name("a/b:c*?\"<>|\\d", "wav"), "a b c d.wav");
    assert_eq!(super::file_name("  \t ", "toml"), "sound.toml");
    assert_eq!(super::file_name("...", "toml"), "sound.toml");
    let long = super::file_name(&"x".repeat(500), "wav");
    assert_eq!(long.chars().count(), 84);
}

#[test]
fn readiness_is_pure_and_failed_preparation_keeps_all_staged_text() {
    let (mut ws, _) = workspace();
    let (_, frequency) = document(&mut ws, 440.0);
    ws.stage(frequency, "30000").unwrap();
    let revision = ws.revision();
    let committed = ws.editor.draft_revision();
    let input = ws.editor.input_revision();
    let next_id = ws.next_id;
    let effects = ws.take_effects();
    for _ in 0..10 {
        let actions = ws.action_view();
        assert!(actions.can_save);
        assert!(!actions.can_export);
    }
    assert_eq!(ws.next_id, next_id);
    assert_eq!(ws.revision(), revision);
    assert_eq!(ws.editor.draft_revision(), committed);
    assert_eq!(ws.editor.input_revision(), input);
    assert!(ws.take_effects().is_empty());
    assert!(!ws.audio_busy() && !ws.file_busy());
    assert!(ws.export().is_err());
    assert_eq!(ws.editor.staged(frequency.field), Some("30000"));
    assert_eq!(ws.editor.draft_revision(), committed);
    drop(effects);
    let notes = ws
        .editor
        .handle(EditingField::Metadata(crate::MetadataField::Summary));
    ws.stage(notes, &"🎧".repeat(Recipe::MAX_BYTES)).unwrap();
    assert!(ws.save().is_err());
    assert!(ws.editor.staged(notes.field).is_some());
    assert_eq!(ws.editor.draft_revision(), committed);
}

#[test]
fn library_dependencies_exclude_editor_and_telemetry_but_include_selection_query_and_rate() {
    let (mut ws, _) = workspace();
    let initial = ws.library_view();
    let evaluations = ws.library_evaluations.get();
    let (_, field) = document(&mut ws, 440.0);
    ws.stage(field, "880").unwrap();
    ws.action_view();
    ws.editor_view();
    ws.poll();
    assert_eq!(ws.library_view(), initial);
    assert_eq!(ws.library_evaluations.get(), evaluations);
    ws.select(1);
    assert_ne!(ws.library_view().title, initial.title);
    assert_eq!(ws.library_evaluations.get(), evaluations + 1);
    ws.set_query("no matching result");
    let empty = ws.library_view();
    assert!(empty.rows.is_empty() && empty.title.is_empty() && empty.details.is_empty());
    ws.set_query("");
    ws.set_export_rate(SampleRate::new(96000).unwrap());
    assert!(!ws.library_view().rows.is_empty());
    assert_eq!(ws.library_evaluations.get(), evaluations + 3);
}

#[test]
fn reopened_copy_navigation_keeps_declared_ancestry_without_reconstructing_original_audio() {
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("original.toml");
    let original = crate::catalog().unwrap().remove(0);
    crate::save_content(original.recipe(), &path, crate::Replace::Never).unwrap();
    let loaded = crate::load_file(&path).unwrap();
    let mut draft = Draft::from_snapshot(&loaded).unwrap();
    draft.set_summary("Own notes".into());
    let derivative = dir.path().join("derivative.toml");
    crate::save_content(&draft.recipe(), &derivative, crate::Replace::Never).unwrap();
    std::fs::remove_file(path).unwrap();
    let reopened = crate::load_file(&derivative).unwrap();
    let (mut ws, _) = workspace();
    ws.replace_document(Draft::from_snapshot(&reopened).unwrap());
    ws.show_based_on_source();
    assert_eq!(
        ws.selected().unwrap().recipe(),
        reopened.recipe(),
        "navigation addresses the captured copied-from document, not a fabricated ancestor waveform"
    );
    assert_eq!(
        ws.selected().unwrap().recipe().lineage[0].document_id,
        Some(original.recipe().document.id)
    );
    assert_eq!(
        ws.selected().unwrap().recipe().sources,
        original.recipe().sources
    );
    assert!(
        ws.selected()
            .unwrap()
            .recipe()
            .evidence
            .iter()
            .all(|e| e.scope == crate::EvidenceScope::Ancestor)
    );
    assert_eq!(ws.editor.draft().summary(), "Own notes");
}

#[test]
fn search_includes_own_notes_and_attributed_persisted_source_context() {
    let source = crate::catalog().unwrap().remove(0);
    let citation = source
        .about()
        .unwrap()
        .research
        .as_ref()
        .unwrap()
        .citation
        .clone()
        .unwrap();
    let mut draft = Draft::from_snapshot(&source).unwrap();
    draft.set_summary("unique-own-notes-marker 🎧".into());
    let bytes = draft.to_toml().unwrap();
    let reopened = crate::load_reader(bytes.as_bytes(), "reopened derivative").unwrap();
    let mut ws = Workspace::new(vec![reopened], Capabilities::portable()).unwrap();
    for query in ["unique-own-notes-marker", &citation] {
        ws.set_query(query);
        assert_eq!(ws.filtered().len(), 1, "{query}");
        assert!(ws.library_view().details.contains("Source citation:"));
        assert!(ws.selected().unwrap().about().unwrap().research.is_none());
    }
    ws.set_query("not-present-anywhere-marker");
    assert!(ws.filtered().is_empty());
    let mut custom = Draft::new().unwrap();
    custom.set_summary("custom-notes-marker".into());
    ws.add_import(custom.snapshot());
    ws.set_query("custom-notes-marker");
    assert_eq!(ws.filtered().len(), 1);
    ws.set_query("unique-own-notes-marker");
    assert_eq!(ws.filtered().len(), 1);
    ws.add_import(crate::RecipeSnapshot::new(
        crate::Origin::Stream("reopened derivative".into()),
        custom.recipe(),
    ));
    ws.set_query("unique-own-notes-marker");
    // A reimport under the same origin must remove stale search text.
    assert_eq!(ws.filtered().len(), 0);
    let mut direct = Workspace::new(vec![source], Capabilities::portable()).unwrap();
    direct.set_query(&citation);
    assert_eq!(direct.filtered().len(), 1);
}

#[test]
fn readiness_details_and_export_share_actual_snapshot_realizations_including_errors() {
    let mut ws = Workspace::new(crate::catalog().unwrap(), Capabilities::portable()).unwrap();
    let selected = ws.selected().unwrap().clone();
    let _ = ws.library_view();
    let count = selected.realization_count();
    assert_eq!(
        count, 1,
        "rows, structure and details share the same actual compile"
    );
    for _ in 0..10 {
        ws.action_view();
        let _ = ws.library_view();
        ws.set_hold(true);
        ws.set_hold(false);
    }
    assert_eq!(selected.realization_count(), count);
    ws.export().unwrap();
    let prompt = ws.prompt.as_ref().unwrap().id;
    ws.respond(prompt, Response::Path(None));
    assert_eq!(selected.realization_count(), count);
    ws.set_export_rate(SampleRate::new(96000).unwrap());
    ws.action_view();
    assert_eq!(selected.realization_count(), count + 1);
    ws.set_export_rate(SampleRate::DEFAULT);
    ws.action_view();
    assert_eq!(selected.realization_count(), count + 1);
    let mut invalid = Draft::new().unwrap();
    invalid.set_duration_seconds(-1.0);
    let snapshot = invalid.snapshot();
    let mut bad = Workspace::new(vec![snapshot.clone()], Capabilities::portable()).unwrap();
    for _ in 0..10 {
        assert!(!bad.action_view().can_export);
        let _ = bad.library_view();
    }
    assert_eq!(
        snapshot.realization_count(),
        1,
        "failures cache at the real compile path too"
    );
    for hz in [96000, 192000, 44100] {
        snapshot.at(SampleRate::new(hz).unwrap()).unwrap_err();
    }
    assert_eq!(snapshot.cached_rates(), 2);
    let replacement = crate::RecipeSnapshot::new(
        crate::Origin::File("same.toml".into()),
        selected.recipe().clone(),
    );
    let mut imported = Workspace::new(vec![replacement.clone()], Capabilities::portable()).unwrap();
    imported.action_view();
    assert_eq!(replacement.realization_count(), 1);
    let fresh = crate::RecipeSnapshot::new(
        crate::Origin::File("same.toml".into()),
        selected.recipe().clone(),
    );
    imported.add_import(fresh.clone());
    imported.action_view();
    assert_eq!(
        fresh.realization_count(),
        1,
        "reimports cannot reuse another identity's result"
    );
    assert_eq!(replacement.realization_count(), 1);
}

fn controlled() -> (Workspace, testing::Execution) {
    testing::workspace(
        crate::catalog().unwrap(),
        Capabilities {
            device_playback: true,
        },
    )
    .unwrap()
}

fn chosen_path(ws: &mut Workspace, path: &std::path::Path) {
    let prompt = ws
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::ChoosePath { prompt, .. } => Some(prompt),
            _ => None,
        })
        .expect("command opened a path prompt");
    ws.respond(prompt, Response::Path(Some(path.to_owned())));
}

#[test]
fn commanded_save_preserves_dialog_capture_and_owns_the_slot_through_real_join() {
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("captured.toml");
    let (mut ws, execution) = controlled();
    let (_, field) = document(&mut ws, 440.0);
    ws.save().unwrap();
    // Simulate independently changed editor content before the delayed panel reply.
    ws.editor.commit_field(field, "880").unwrap().unwrap();
    chosen_path(&mut ws, &path);
    let save = execution.file().unwrap();
    assert_eq!(save.path(), path);
    assert_eq!(
        save.captured_recipe().unwrap().stimulus.components[0]
            .node
            .read(ParameterId::ToneFrequency),
        Some(440.0)
    );
    assert_eq!(ws.import(), Err(Refusal::FileBusy));
    save.result().unwrap();
    ws.poll();
    assert!(ws.file_busy(), "result is not exit");
    assert!(ws.editor().dirty());
    assert_eq!(
        crate::load_file(&path).unwrap().stimulus().components[0]
            .node
            .read(ParameterId::ToneFrequency),
        Some(440.0)
    );
    ws.commit_field(field, "990").unwrap();
    save.exit().unwrap();
    settle(&mut ws, |w| !w.file_busy());
    assert!(
        ws.editor().dirty(),
        "later edits are not the published baseline"
    );
}

#[test]
fn commanded_import_launch_missing_result_and_panic_are_visible_without_reviving_work() {
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("import.toml");
    crate::save_content(
        &Draft::new().unwrap().recipe(),
        &path,
        crate::Replace::Never,
    )
    .unwrap();
    let (mut ws, execution) = controlled();
    let before = ws.catalog().len();
    ws.import().unwrap();
    chosen_path(&mut ws, &path);
    let import = execution.file().unwrap();
    import.result().unwrap();
    ws.poll();
    assert!(ws.file_busy());
    assert_eq!(ws.catalog().len(), before);
    import.exit().unwrap();
    settle(&mut ws, |w| !w.file_busy());
    assert_eq!(ws.catalog().len(), before + 1);
    for panic in [false, true] {
        ws.import().unwrap();
        chosen_path(&mut ws, &path);
        let import = execution.file().unwrap();
        if panic {
            import.panic().unwrap();
        } else {
            import.exit().unwrap();
        }
        settle(&mut ws, |w| !w.file_busy());
        assert!(ws.action_view().file_status.contains(if panic {
            presentation::WORKER_FAILED
        } else {
            presentation::WORKER_ENDED_WITHOUT_RESULT
        }));
        assert_eq!(ws.catalog().len(), before + 1);
    }
    execution.fail_file_launch();
    ws.import().unwrap();
    chosen_path(&mut ws, &path);
    ws.poll();
    assert!(!ws.file_busy());
    assert!(ws.action_view().file_status.contains("Cannot start worker"));
}

#[test]
fn commanded_export_cancellation_and_unrelated_file_failure_preserve_audio_dependencies() {
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("cancelled.wav");
    let (mut ws, execution) = controlled();
    ws.export().unwrap();
    assert_eq!(ws.play(), Err(Refusal::PromptOpen));
    chosen_path(&mut ws, &path);
    let export = execution.file().unwrap();
    assert_eq!(ws.play(), Err(Refusal::Exporting));
    ws.cancel_export().unwrap();
    assert!(export.cancelled());
    export.result().unwrap();
    ws.poll();
    assert!(ws.file_busy());
    export.exit().unwrap();
    settle(&mut ws, |w| !w.file_busy());
    assert!(!path.exists());
    ws.import().unwrap();
    chosen_path(&mut ws, &path);
    let import = execution.file().unwrap();
    ws.play().unwrap();
    let first = execution.audio().unwrap();
    ws.play().unwrap();
    assert!(ws.action_pending());
    import.fail().unwrap();
    import.exit().unwrap();
    settle(&mut ws, |w| !w.file_busy());
    first.result().unwrap();
    first.exit().unwrap();
    settle(&mut ws, |w| !w.action_pending());
    let replacement = execution.audio().unwrap();
    assert!(
        ws.audio_busy(),
        "unrelated import failure cannot block replacement audio"
    );
    replacement.result().unwrap();
    replacement.exit().unwrap();
    settle(&mut ws, |w| !w.audio_busy());
    ws.shutdown().unwrap();
}

#[test]
fn commanded_launch_failure_blocks_dependent_close_and_save_success_waits_for_join() {
    let (mut ws, execution) = controlled();
    execution.fail_audio_launch();
    ws.play().unwrap();
    ws.close();
    ws.poll();
    assert_eq!(
        ws.take_effects(),
        [Effect::CloseResolved { approved: false }]
    );
    assert!(
        ws.action_view()
            .audio_status
            .contains("Cannot start worker")
    );
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("close.toml");
    document(&mut ws, 440.0);
    ws.close();
    let Effect::AskUnsaved(prompt) = ws.take_effects()[0] else {
        panic!("unsaved prompt");
    };
    ws.respond(prompt, Response::Save);
    chosen_path(&mut ws, &path);
    let save = execution.file().unwrap();
    save.result().unwrap();
    ws.poll();
    assert!(
        ws.take_effects().is_empty(),
        "publication alone cannot authorize close"
    );
    save.exit().unwrap();
    settle(&mut ws, |w| !w.file_busy());
    assert_eq!(
        ws.take_effects(),
        [Effect::CloseResolved { approved: true }]
    );
}

#[test]
fn search_retains_flat_ancestor_notes_and_reimport_invalidates_old_metadata() {
    let mut original = Draft::new().unwrap();
    original.set_summary("original-custom-note-marker".into());
    let mut edited = Draft::from_snapshot(&original.snapshot()).unwrap();
    edited.set_summary("current-own-notes-marker".into());
    let reopened = crate::load_reader(edited.to_toml().unwrap().as_bytes(), "derivative").unwrap();
    let recipe = reopened.recipe().clone();
    let first = RecipeSnapshot::new(crate::Origin::File("same.toml".into()), recipe.clone());
    let mut ws = Workspace::new(vec![first.clone()], Capabilities::portable()).unwrap();
    ws.set_query("original-custom-note-marker");
    assert_eq!(ws.filtered().len(), 1);
    let initial = first.realization_count();
    let mut changed = recipe;
    changed.lineage[0].note = "reimport-source-marker".into();
    let replacement = RecipeSnapshot::new(crate::Origin::File("same.toml".into()), changed);
    ws.add_import(replacement.clone());
    ws.set_query("original-custom-note-marker");
    assert!(ws.library_view().rows.is_empty());
    ws.set_query("reimport-source-marker");
    assert_eq!(ws.filtered().len(), 1);
    assert!(ws.library_view().details.contains("reimport-source-marker"));
    ws.action_view();
    assert_eq!(replacement.realization_count(), 1);
    assert_eq!(first.realization_count(), initial);
}

#[test]
fn copy_availability_and_execution_share_the_component_limit() {
    let mut recipe = Draft::new().unwrap().recipe();
    recipe.stimulus.components = (0..=Sound::MAX_COMPONENTS)
        .map(|i| crate::Component {
            id: ComponentId::numbered(i).unwrap(),
            node: crate::Node::Voice(crate::default_component(CarrierKind::Tone)),
        })
        .collect();
    let snapshot = RecipeSnapshot::new(crate::Origin::Stream("too many".into()), recipe);
    let mut ws = Workspace::new(vec![snapshot], Capabilities::portable()).unwrap();
    let document = ws.editor().document();
    assert!(!ws.action_view().can_copy);
    ws.copy_selected();
    assert_eq!(ws.editor().document(), document);
    assert!(
        ws.action_view()
            .file_status
            .contains("Edit the recipe file to reduce them")
    );
    assert!(ws.take_effects().is_empty());
}

#[test]
fn copy_preflight_refuses_nonfinite_or_full_metadata_before_touching_dirty_work() {
    let mut source = Draft::new().unwrap().recipe();
    source.stimulus.components.push(crate::Component {
        id: ComponentId::new(1),
        node: crate::Node::Voice(crate::Voice {
            carrier: crate::Carrier::Tone {
                frequency_hz: f64::NAN,
            },
            ..crate::default_component(CarrierKind::Tone)
        }),
    });
    let snapshot = RecipeSnapshot::new(
        crate::Origin::Stream("nonfinite source".into()),
        source.clone(),
    );
    let mut ws = Workspace::new(vec![snapshot], Capabilities::portable()).unwrap();
    let doc = ws.editor().document();
    ws.set_view(View::Workbench);
    ws.add_component(doc, CarrierKind::Tone).unwrap();
    ws.set_view(View::Library);
    let before = ws.editor().draft().recipe();
    assert!(!ws.action_view().can_copy);
    ws.copy_selected();
    assert_eq!(ws.editor().document(), doc);
    assert_eq!(ws.editor().draft().recipe(), before);
    assert!(ws.take_effects().is_empty());
    source.stimulus.components[0]
        .node
        .write(auditorylab_core::ParameterId::ToneFrequency, 440.0);
    source.stimulus.duration_seconds = -1.0;
    let finite = RecipeSnapshot::new(crate::Origin::Stream("finite invalid".into()), source);
    assert!(Workspace::copy_content_admission(&finite).is_ok());
    let mut full = finite.recipe().clone();
    full.lineage = vec![
        crate::Lineage {
            title: "ancestor".into(),
            note: "attributed".into(),
            document_id: None,
            catalog_id: None,
            source_keys: Vec::new()
        };
        16
    ];
    let full = RecipeSnapshot::new(crate::Origin::Stream("full ancestry".into()), full);
    assert!(Workspace::copy_content_admission(&full).is_err());
}

#[test]
fn global_summary_preserves_full_refusal_and_outcome_independent_of_editor_scroll() {
    let (mut ws, _) = workspace();
    ws.set_view(View::Workbench);
    let notes = ws
        .editor
        .handle(EditingField::Metadata(crate::MetadataField::Summary));
    ws.stage(notes, &"Z".repeat(66000)).unwrap();
    let actions = ws.action_view();
    assert!(!actions.can_save);
    assert!(actions.operation_summary.contains("65536"));
    ws.file_status = "Exported 100 frames at 48000 Hz".into();
    let actions = ws.action_view();
    assert!(actions.operation_summary.contains("65536"));
    assert!(actions.operation_outcome.starts_with("Exported 100"));
    assert!(actions.operation_summary.chars().count() <= 181);
    ws.editor.restore(notes).unwrap();
    ws.file_status = format!("Export failed: {}", "deep/path/".repeat(50));
    let actions = ws.action_view();
    assert!(actions.operation_summary.starts_with("Export failed"));
    assert!(actions.operation_details.len() > actions.operation_summary.len());
}

#[test]
fn nominal_listing_a_thousand_rows_does_not_compile_every_sound() {
    let base = crate::builtin("ioannou-2015-binaural-2-hz").unwrap();
    let snapshots = (0..1000)
        .map(|i| {
            RecipeSnapshot::new(
                crate::Origin::Stream(format!("fixture-{i}")),
                base.recipe().clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut ws = Workspace::new(snapshots.clone(), Capabilities::portable()).unwrap();
    ws.set_rhythm_query(crate::query::RhythmQuery {
        bands: vec![crate::nominal::Band::Delta],
        ..Default::default()
    });
    assert_eq!(ws.library_view().rows.len(), 1000);
    assert_eq!(
        snapshots
            .iter()
            .map(RecipeSnapshot::realization_count)
            .sum::<u64>(),
        1,
        "only the selected detail is compiled"
    );
    ws.set_export_rate(SampleRate::new(96_000).unwrap());
    assert_eq!(ws.library_view().rows.len(), 1000);
    assert_eq!(
        snapshots
            .iter()
            .map(RecipeSnapshot::realization_count)
            .sum::<u64>(),
        2
    );
}
