//! Physical worker ownership and coordinator protocol tests without a device.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::core::{CarrierKind, ParameterId, SampleRate};
use crate::document::editing::{EditingField, FieldHandle};
use crate::services::library::{Library, Mutation};
use crate::workspace::status::{AudioStatus, FileStatus};
use crate::workspace::{AudioCommand, Command, EditorCommand, LibraryCommand, OutputCommand};
use crate::workspace::{
    Capabilities, Effect, PromptAnswer, ResponseDisposition, View, WorkerFailure,
};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use {
    crate::audio::session::EndReason, crate::audio::session::LiveGenerator,
    crate::audio::session::MonitorDetail, crate::audio::session::Outcome,
    crate::audio::session::OutputInfo, crate::audio::session::PlaybackMonitor,
    crate::audio::session::SampleFormat, crate::audio::session::SessionDescriptor,
};

fn fixture() -> (tempfile::TempDir, Library, Workspace) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let directory = tempfile::tempdir_in(base).unwrap();
    let store =
        crate::services::library::Library::at_path(directory.path().join("library")).unwrap();
    store.initialize().unwrap();
    let mut workspace = Workspace::with_library(
        crate::services::catalog().unwrap(),
        Capabilities::available(),
        store.clone(),
    )
    .unwrap();
    idle_files(&mut workspace);
    (directory, store, workspace)
}
fn idle_files(workspace: &mut Workspace) {
    for _ in 0..100 {
        let mut retained = Vec::new();
        let mut ran = false;
        for effect in workspace.take_effects() {
            match effect {
                Effect::Run(job) if job.kind() == JobKind::File => {
                    let response = job.execute(|_| panic!("file job prepared audio"));
                    workspace.respond(response);
                    ran = true;
                }
                effect => retained.push(effect),
            }
        }
        workspace.effects.splice(0..0, retained);
        if !ran {
            return;
        }
    }
    panic!("file work did not settle");
}
fn take_job(workspace: &mut Workspace, kind: JobKind) -> Job {
    let mut picked = None;
    let mut retained = Vec::new();
    for effect in workspace.take_effects() {
        match effect {
            Effect::Run(job) if job.kind() == kind && picked.is_none() => picked = Some(job),
            effect => retained.push(effect),
        }
    }
    workspace.effects.extend(retained);
    picked.expect("command emitted the required job")
}
fn tone(workspace: &mut Workspace, hz: &str) -> FieldHandle {
    let _ = workspace.apply(Command::SetView(View::Workbench));
    workspace
        .apply(Command::Editor(EditorCommand::AddComponent {
            row: workspace.editor().document_key(),
            kind: CarrierKind::Tone,
        }))
        .unwrap();
    let id = workspace.editor().draft().components()[0].id;
    let handle = workspace
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ToneFrequency))
        .unwrap();
    workspace
        .apply(Command::Editor(EditorCommand::CommitField {
            field: handle,
            text: (hz).to_string(),
        }))
        .unwrap();
    handle
}
fn close_after_save(workspace: &mut Workspace) -> Job {
    let _ = workspace.apply(Command::Close);
    let prompt = workspace
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::AskUnsaved(prompt) => Some(prompt),
            _ => None,
        })
        .unwrap();
    workspace
        .apply(Command::Answer {
            prompt,
            answer: PromptAnswer::Save,
        })
        .unwrap();
    take_job(workspace, JobKind::File)
}
fn pump(runner: &mut Runner, workspace: &mut Workspace) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for response in runner.poll() {
            workspace.respond(response);
        }
        if !runner.busy() {
            break;
        }
        assert!(Instant::now() < deadline, "worker did not join");
        thread::yield_now();
    }
}
fn closed(workspace: &mut Workspace, approved: bool) -> bool {
    workspace
        .take_effects()
        .iter()
        .any(|effect| matches!(effect,Effect::CloseResolved {approved:actual} if *actual==approved))
}
// Private application-unit preparation fixture: core validation and session state
// are real, while the declared output metadata never creates a native stream.
fn prepared(
    job: &Job,
    rate: SampleRate,
) -> Result<(Response, LiveGenerator), crate::audio::PlaybackError> {
    prepared_with(job, rate, MonitorDetail::Quantities)
        .map(|(response, generator, _monitor)| (response, generator))
}
fn prepared_with(
    job: &Job,
    rate: SampleRate,
    detail: MonitorDetail,
) -> Result<(Response, LiveGenerator, PlaybackMonitor), crate::audio::PlaybackError> {
    let Work::Audio {
        session,
        kind,
        draft,
    } = &*job.work
    else {
        panic!("audio job")
    };
    let target =
        match (kind, draft) {
            (AudioKind::Live(_), Some(draft)) => draft.check(rate).into_result().map_err(|error| {
                crate::audio::PlaybackError::Sound {
                    title: "unit capture".into(),
                    rate,
                    error,
                }
            }),
            (AudioKind::Finite(snapshot), _) => snapshot
                .at(rate)
                .map(|sound| *sound.signal().target())
                .map_err(Into::into),
            (AudioKind::Managed { snapshot, store }, _) => revalidate_managed(snapshot, store)?
                .at(rate)
                .map(|sound| *sound.signal().target())
                .map_err(Into::into),
            _ => panic!("complete audio capture"),
        };
    let target = match target {
        Ok(target) => target,
        Err(error) => {
            session.state().finish(Outcome::PreparationFailed);
            return Err(error);
        }
    };
    let generator = LiveGenerator::with_state(&(target), Arc::clone(session.state()));
    let live = matches!(kind, AudioKind::Live(_)).then(|| generator.control());
    let monitor = PlaybackMonitor::new(
        SessionDescriptor {
            id: session.id(),
            title: "unit output".into(),
            origin: crate::document::recipe::Origin::Stream("unit capture".into()),
            output: OutputInfo {
                name: None,
                channels: 2,
                sample_rate: rate,
                format: SampleFormat::F64,
            },
            frames: None,
            target,
        },
        detail,
        Arc::clone(session.state()),
        Some(Arc::clone(generator.mailbox())),
    );
    Ok((
        Response::Prepared {
            job: job.id(),
            reader: Box::new(monitor.reader().unwrap()),
            rate,
            live,
        },
        generator,
        monitor,
    ))
}

#[test]
fn real_file_result_waits_for_thread_exit_before_releasing_save_and_close() {
    let (_directory, store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    let job = close_after_save(&mut workspace);
    let id = job.id();
    let release = Arc::new(Barrier::new(2));
    let worker_release = release.clone();
    let (computed, received) = mpsc::channel();
    let (_, events) = mpsc::channel();
    let handle = thread::spawn(move || {
        let result = job.execute(|_| panic!("file only"));
        computed.send(()).unwrap();
        worker_release.wait();
        result
    });
    let mut runner = Runner::new();
    runner.started(id, events, Ok(handle));
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        store.scan(&Cancel::default()).entries.len(),
        1,
        "actual publication has already happened"
    );
    assert!(runner.poll().is_empty());
    assert!(workspace.file_busy());
    assert!(workspace.action_pending());
    assert!(workspace.editor().dirty());
    assert!(!closed(&mut workspace, true));
    release.wait();
    pump(&mut runner, &mut workspace);
    assert!(!workspace.file_busy());
    assert!(!workspace.editor().dirty());
    assert!(closed(&mut workspace, true));
}
#[test]
fn a_panic_after_actual_publication_retains_dirty_state_and_denies_close() {
    let (_directory, store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    let job = close_after_save(&mut workspace);
    let id = job.id();
    let (_, events) = mpsc::channel();
    let handle = thread::spawn(move || {
        let _ = job.execute(|_| {});
        panic!("after actual publication")
    });
    let mut runner = Runner::new();
    runner.started(id, events, Ok(handle));
    pump(&mut runner, &mut workspace);
    assert_eq!(store.scan(&Cancel::default()).entries.len(), 1);
    assert!(workspace.editor().dirty());
    assert!(closed(&mut workspace, false));
    assert!(
        matches!(workspace.files.status(),FileStatus::WorkerFailed {error:WorkerFailure::Panicked,
            activity} if activity.path==store.path())
    );
}
#[test]
fn launch_failure_is_a_typed_completion_and_blocks_dependent_close() {
    let (_directory, store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    let job = close_after_save(&mut workspace);
    let id = job.id();
    drop(job);
    let (_, events) = mpsc::channel();
    let mut runner = Runner::new();
    runner.started(
        id,
        events,
        Err(std::io::Error::other("injected unavailable thread")),
    );
    assert!(workspace.file_busy());
    pump(&mut runner, &mut workspace);
    assert!(closed(&mut workspace, false));
    assert!(workspace.editor().dirty());
    assert!(store.scan(&Cancel::default()).entries.is_empty());
}
#[test]
fn stop_before_job_execution_returns_a_real_stopped_report_without_discovery() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    workspace.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    let response = job.execute(|_| panic!("stopped audio must not prepare a device"));
    assert!(
        matches!(&response,Response::Finished {outcome:JobOutcome::Audio(Ok(report)),..}
            if report.end==EndReason::Stopped && report.generated_frames==0)
    );
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert!(!workspace.audio_busy());
    assert!(workspace.live_rate().is_none());
}
#[test]
fn actual_rate_preparation_submits_committed_edits_but_never_staged_text() {
    let (_directory, _store, mut workspace) = fixture();
    let frequency = tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    workspace
        .apply(Command::Editor(EditorCommand::CommitField {
            field: frequency,
            text: ("30000").to_string(),
        }))
        .unwrap();
    workspace
        .apply(Command::Editor(EditorCommand::Stage {
            field: frequency,
            text: ("500").to_string(),
        }))
        .unwrap();
    let (response, _generator) = prepared(&job, SampleRate::new(96000).unwrap()).unwrap();
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    let accepted = workspace
        .audio
        .task()
        .unwrap()
        .live
        .as_ref()
        .unwrap()
        .accepted()
        .unwrap();
    assert_eq!(accepted.0, 2);
    assert_eq!(accepted.1.sample_rate().hz(), 96000);
    let (_, facts) = accepted.1.voices().next().unwrap();
    let crate::core::CarrierFacts::Tone(rate) = facts.carrier else {
        panic!()
    };
    assert_eq!(rate.requested_hz, 30000.0);
    assert_eq!(workspace.editor().staged(frequency.field), Some("500"));
    assert_eq!(workspace.diagnostic_rate().hz(), 96000);
    assert_eq!(workspace.export_rate(), SampleRate::DEFAULT);
    assert!(!workspace.action_view().can_export());
    workspace
        .apply(Command::Editor(EditorCommand::CommitField {
            field: frequency,
            text: ("60000").to_string(),
        }))
        .unwrap();
    assert_eq!(
        workspace
            .audio
            .task()
            .unwrap()
            .live
            .as_ref()
            .unwrap()
            .accepted()
            .unwrap()
            .0,
        2,
        "invalid committed edit retains the last accepted target"
    );
    workspace.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn device_rate_validation_is_independent_of_export_rate() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "30000");
    let _ = workspace.apply(Command::Editor(EditorCommand::SetExportRate(
        SampleRate::new(96000).unwrap(),
    )));
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let Err(error) = prepared(&job, SampleRate::new(8000).unwrap()) else {
        panic!("invalid at actual rate")
    };
    assert_eq!(error.outcome(), crate::outcome::Outcome::InvalidSound);
    workspace.respond(Response::Finished {
        job: job.id(),
        outcome: JobOutcome::Audio(Err(error)),
    });
    assert!(!workspace.audio_busy());
    assert!(matches!(workspace.audio.status(), AudioStatus::Failed(_)));
    drop(job);
}
#[test]
fn stale_and_duplicate_preparations_cannot_rebind_audio() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, _generator) = prepared(&job, SampleRate::DEFAULT).unwrap();
    let Response::Prepared {
        reader, rate, live, ..
    } = response
    else {
        panic!()
    };
    assert_eq!(
        workspace.respond(Response::Prepared {
            job: TaskId(job.id().0 + 100),
            reader,
            rate,
            live
        }),
        ResponseDisposition::Ignored
    );
    assert!(workspace.live_rate().is_none());
    let (response, _generator2) = prepared(&job, SampleRate::DEFAULT).unwrap();
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    let (response, _generator3) = prepared(&job, SampleRate::DEFAULT).unwrap();
    assert_eq!(workspace.respond(response), ResponseDisposition::Rejected);
    assert!(workspace.audio.task().unwrap().protocol_failed);
    let response = job.execute(|_| panic!("rejected preparation requested Stop"));
    assert_eq!(workspace.respond(response), ResponseDisposition::Rejected);
    assert!(!workspace.audio_busy());
}
#[test]
fn wrong_preparation_kind_rate_and_session_are_rejected() {
    for fault in 0..3 {
        let (_directory, _store, mut workspace) = fixture();
        tone(&mut workspace, "440");
        workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
        let job = take_job(&mut workspace, JobKind::Audio);
        let (response, _generator) = prepared(&job, SampleRate::DEFAULT).unwrap();
        let Response::Prepared {
            reader, rate, live, ..
        } = response
        else {
            panic!()
        };
        let response = match fault {
            0 => Response::Prepared {
                job: job.id(),
                reader,
                rate,
                live: None,
            },
            1 => Response::Prepared {
                job: job.id(),
                reader,
                rate: SampleRate::new(96000).unwrap(),
                live,
            },
            _ => {
                let other = Session::new();
                let target = crate::core::Target::silence(rate);
                let foreign = LiveGenerator::with_state(&(target), Arc::clone(other.state()));
                let control = foreign.control();
                Response::Prepared {
                    job: job.id(),
                    reader,
                    rate,
                    live: Some(control),
                }
            }
        };
        assert_eq!(workspace.respond(response), ResponseDisposition::Rejected);
        workspace.respond(job.execute(|_| {}));
        assert!(!workspace.audio_busy());
    }
}
#[test]
fn finite_preparation_refuses_a_live_control() {
    let (_directory, _store, mut workspace) = fixture();
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, generator) = prepared(&job, SampleRate::DEFAULT).unwrap();
    let Response::Prepared { reader, rate, .. } = response else {
        panic!()
    };
    assert_eq!(
        workspace.respond(Response::Prepared {
            job: job.id(),
            reader,
            rate,
            live: Some(generator.control())
        }),
        ResponseDisposition::Rejected
    );
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn later_document_preparation_keeps_current_document_unbound() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let old = workspace.editor().document();
    assert!(workspace.replace_document(Draft::new().unwrap()));
    assert_ne!(workspace.editor().document(), old);
    let (response, _generator) = prepared(&job, SampleRate::new(96000).unwrap()).unwrap();
    workspace.respond(response);
    assert!(workspace.live_rate().is_none());
    assert_eq!(workspace.diagnostic_rate(), workspace.export_rate());
    workspace.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn live_monitor_captures_are_telemetry_and_leave_the_revision_unchanged() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, _generator, monitor) = prepared_with(
        &job,
        SampleRate::new(8000).unwrap(),
        MonitorDetail::Waveform,
    )
    .unwrap();
    let mut writer = monitor.writer().unwrap();
    workspace.respond(response);
    workspace.poll();
    let mut frame = 0;
    let mut tick = |workspace: &mut Workspace| {
        let block: Vec<[f64; 2]> = (0..80u32)
            .map(|i| {
                let v = (f64::from(i) * 0.3).sin() * 0.1;
                [v, -v]
            })
            .collect();
        writer.append(frame, &block);
        frame += 80;
        writer.finish(frame, None);
        workspace.poll()
    };
    tick(&mut workspace);
    for held in [false, true] {
        let _ = workspace.apply(Command::Output(OutputCommand::SetHold(held)));
        workspace.poll();
        let revision = workspace.revision();
        let first = workspace.output_view().capture;
        let mut changed = 0;
        for _ in 0..20 {
            changed += usize::from(tick(&mut workspace));
        }
        assert_eq!(
            workspace.revision(),
            revision,
            "captures must not ask frontends to rebuild document views (held: {held})"
        );
        if held {
            assert!(Arc::ptr_eq(
                first.as_ref().unwrap(),
                workspace.output_view().capture.as_ref().unwrap()
            ));
        } else {
            assert!(changed > 0, "poll still reports changed telemetry");
            assert!(!Arc::ptr_eq(
                first.as_ref().unwrap(),
                workspace.output_view().capture.as_ref().unwrap()
            ));
        }
    }
    workspace.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn accepted_live_revision_changes_update_model_even_for_identical_audio() {
    let (_directory, _store, mut workspace) = fixture();
    let _ = workspace.apply(Command::SetView(View::Workbench));
    workspace
        .apply(Command::Editor(EditorCommand::AddComponent {
            row: workspace.editor().document_key(),
            kind: CarrierKind::PulseTrain,
        }))
        .unwrap();
    let id = workspace.editor().draft().components()[0].id;
    let width = workspace
        .editor()
        .handle(EditingField::Parameter(
            id,
            ParameterId::SourcePulseDuration,
        ))
        .unwrap();
    workspace
        .apply(Command::Editor(EditorCommand::CommitField {
            field: width,
            text: ("1").to_string(),
        }))
        .unwrap();
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, _generator) = prepared(&job, SampleRate::new(8000).unwrap()).unwrap();
    workspace.respond(response);
    workspace.poll();
    let first = workspace.model_view().unwrap();
    let models = workspace.audio.output().models;
    for _ in 0..20 {
        workspace.poll();
    }
    assert_eq!(workspace.audio.output().models, models);
    workspace
        .apply(Command::Editor(EditorCommand::CommitField {
            field: width,
            text: ("1.01").to_string(),
        }))
        .unwrap();
    workspace.poll();
    let second = workspace.model_view().unwrap();
    assert!(second.target.same_audio(&first.target));
    assert_ne!(second.target, first.target);
    assert_eq!(workspace.audio.output().models, models + 1);
    workspace.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn wrong_library_identity_or_kind_cannot_mark_a_clean_draft_saved_or_close_it() {
    for fault in 0..3 {
        let (_directory, store, mut workspace) = fixture();
        let original = workspace.editor().document();
        workspace.save().unwrap();
        let job = take_job(&mut workspace, JobKind::File);
        let id = job.id();
        let _ = workspace.apply(Command::Close);
        let mut response = job.execute(|_| {});
        let Response::Finished {
            outcome:
                JobOutcome::File(FileOutcome::Managed(Ok(super::super::LibraryOutcome::Written(
                    outcome,
                )))),
            ..
        } = &mut response
        else {
            panic!()
        };
        match fault {
            0 => {
                let other = Draft::new().unwrap().recipe().unwrap();
                **outcome = store
                    .apply(Mutation::Create(Arc::new(other)), &Cancel::default())
                    .unwrap();
            }
            1 => outcome.kind = crate::storage::library::MutationKind::Rename,
            _ => {
                let mut recipe = outcome
                    .record
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .recipe()
                    .clone()
                    .into_builder();
                recipe.document.id = Draft::new().unwrap().recipe().unwrap().document().id;
                outcome.record.as_mut().unwrap().snapshot = RecipeSnapshot::new(
                    crate::document::recipe::Origin::Stream("wrong record".into()),
                    recipe.build().unwrap(),
                );
            }
        }
        assert_eq!(workspace.respond(response), ResponseDisposition::Rejected);
        assert_eq!(workspace.editor().document(), original);
        assert!(matches!(
            workspace.editor.location(),
            super::super::DocumentLocation::Unsaved
        ));
        assert!(closed(&mut workspace, false));
        assert!(!workspace.file_busy());
        assert_eq!(
            workspace.respond(Response::Finished {
                job: id,
                outcome: JobOutcome::Failed(WorkerFailure::Panicked)
            }),
            ResponseDisposition::Ignored
        );
    }
}
#[test]
fn wrong_loaded_identity_cannot_replace_the_selected_record() {
    let (_directory, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle_files(&mut workspace);
    let a = store.scan(&Cancel::default()).entries[0].key.clone();
    workspace
        .set_catalog_query(Box::new(crate::discovery::collection::CatalogQuery {
            scope: crate::discovery::query::Scope::User,
            ..Default::default()
        }))
        .unwrap();
    idle_files(&mut workspace);
    assert_eq!(
        crate::document::origin::LibraryKey::User(
            workspace.selected().unwrap().recipe().document().id
        ),
        a
    );
    let first = store.load(&a).unwrap();
    store
        .apply(
            Mutation::Rename {
                key: a.clone(),
                expect: Some(first.version_token),
                title: "changed before refresh".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    workspace
        .apply(Command::Library(LibraryCommand::Refresh))
        .unwrap();
    let scan = take_job(&mut workspace, JobKind::File);
    let inventory = scan.execute(|_| {});
    assert!(matches!(
        &inventory,
        Response::Finished {
            outcome: JobOutcome::File(FileOutcome::Managed(Ok(
                crate::workspace::LibraryOutcome::Inventory(_)
            ))),
            ..
        }
    ));
    workspace.respond(inventory);
    let load = take_job(&mut workspace, JobKind::File);
    let Work::File(FileKind::Managed(request)) = &*load.work else {
        panic!("refresh must capture a managed load");
    };
    assert_eq!(request.context().selector.as_ref(), Some(&a));
    assert!(
        request.accepts(&crate::workspace::LibraryOutcome::Loaded(Box::new(
            store.load(&a).unwrap()
        )))
    );
    let other = store
        .apply(
            Mutation::Create(Arc::new(Draft::new().unwrap().recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert_ne!(other.key, a);
    assert_eq!(
        workspace.respond(Response::Finished {
            job: load.id(),
            outcome: JobOutcome::File(FileOutcome::Managed(Ok(
                crate::workspace::LibraryOutcome::Loaded(Box::new(other))
            )))
        }),
        ResponseDisposition::Rejected
    );
    assert!(workspace.selected().is_none());
    assert!(workspace.library_data().read_failure.is_some());
    drop(load);
}

#[test]
fn invalid_editor_replacement_is_refused_before_becoming_a_snapshot() {
    let (_directory, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle_files(&mut workspace);
    let record = store.scan(&Cancel::default()).entries[0].key.clone();
    let original = workspace.editor().document();
    let location = workspace
        .library_data()
        .managed
        .map(|(key, token)| (key.clone(), token));
    let candidate = store
        .load(&record)
        .unwrap()
        .snapshot
        .recipe()
        .clone()
        .into_builder();
    let component = crate::core::Component {
        id: crate::core::ComponentId::new(1).unwrap(),
        form: crate::workspace::controls::default_component(CarrierKind::Tone),
    };
    assert!(
        auditorylab_core::test_support::FiniteFixture::new(
            candidate.content.timing.duration_seconds,
            candidate.content.timing.fade_seconds,
            vec![component; 2]
        )
        .is_err()
    );
    assert_eq!(workspace.editor().document(), original);
    assert_eq!(
        workspace
            .library_data()
            .managed
            .map(|(key, token)| (key.clone(), token)),
        location
    );
}

#[test]
fn shutdown_reports_prior_rejected_preparation_even_when_real_job_stops_ok() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, _generator) = prepared(&job, SampleRate::DEFAULT).unwrap();
    let Response::Prepared { reader, rate, .. } = response else {
        panic!()
    };
    workspace.respond(Response::Prepared {
        job: job.id(),
        reader,
        rate,
        live: None,
    });
    let mut runner = Runner::new();
    runner.start(job);
    let error = runner.shutdown(workspace).unwrap_err();
    assert!(error.issues.iter().any(|issue| matches!(
        issue,
        ShutdownIssue::Task {
            error: TaskFailure::Worker(WorkerFailure::InvalidOutcome),
            ..
        }
    )));
}
#[test]
fn shutdown_reports_wrong_kind_success_after_coordinator_validation() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.save().unwrap();
    let job = take_job(&mut workspace, JobKind::File);
    let id = job.id();
    drop(job);
    let snapshot = Draft::new().unwrap().snapshot().unwrap();
    let (_, prepared) = mpsc::channel();
    let mut runner = Runner::new();
    runner.started(
        id,
        prepared,
        Ok(thread::spawn(move || Response::Finished {
            job: id,
            outcome: JobOutcome::File(FileOutcome::Imported(Ok(snapshot))),
        })),
    );
    let error = runner.shutdown(workspace).unwrap_err();
    assert!(error.issues.iter().any(|issue| matches!(
        issue,
        ShutdownIssue::Task {
            error: TaskFailure::Worker(WorkerFailure::InvalidOutcome),
            context: JobContext::Library(_)
        }
    )));
}
#[test]
fn shutdown_does_not_reclassify_a_historical_resolved_failure() {
    let (_directory, _store, mut workspace) = fixture();
    workspace.files.record_status(FileStatus::Failed(
        crate::storage::error::FileError::MissingFilename.into(),
    ));
    workspace
        .audio
        .record_status(AudioStatus::WorkerFailed(WorkerFailure::Panicked));
    assert!(Runner::new().shutdown(workspace).is_ok());
}

#[test]
fn real_conflicting_save_reports_captured_selector_and_location_without_uncertain_publication() {
    for shutdown in [false, true] {
        let (_directory, store, mut workspace) = fixture();
        let field = tone(&mut workspace, "440");
        workspace.save().unwrap();
        idle_files(&mut workspace);
        let key = store.scan(&Cancel::default()).entries[0].key.clone();
        let observed = store.load(&key).unwrap();
        workspace
            .apply(Command::Editor(EditorCommand::CommitField {
                field,
                text: ("880").to_string(),
            }))
            .unwrap();
        let job = close_after_save(&mut workspace);
        // The same locked mutation used by the CLI changes the observed saved
        // version after this coordinator has captured its immutable Save.
        store
            .apply(
                Mutation::Rename {
                    key: key.clone(),
                    expect: Some(observed.version_token),
                    title: "Concurrent writer".into(),
                },
                &Cancel::default(),
            )
            .unwrap();
        let path = store
            .path()
            .join("recipes")
            .join(format!("{}.toml", observed.snapshot.recipe().document().id));
        let concurrent_bytes = std::fs::read(&path).unwrap();
        let mut runner = Runner::new();
        runner.start(job);
        let (failure, context) = if shutdown {
            let error = runner.shutdown(workspace).unwrap_err();
            let ShutdownIssue::Library(failure) = error.issues.into_iter().next().unwrap() else {
                panic!("library failure")
            };
            (failure.error, failure.context)
        } else {
            pump(&mut runner, &mut workspace);
            assert!(workspace.editor().dirty());
            assert_eq!(
                workspace.editor().draft().components()[0]
                    .form
                    .get(ParameterId::ToneFrequency),
                Some(finite(880.0))
            );
            assert!(closed(&mut workspace, false));
            let FileStatus::Library(super::super::status::LibraryStatus::Failed {
                error,
                context,
                ..
            }) = workspace.files.status().clone()
            else {
                panic!("library failure")
            };
            (error, context)
        };
        assert_eq!(std::fs::read(&path).unwrap(), concurrent_bytes);
        assert_eq!(
            store.load(&key).unwrap().snapshot.title(),
            "Concurrent writer"
        );
        assert_eq!(failure.outcome(), crate::outcome::Outcome::Conflict);
        assert_eq!(context.selector, Some(key));
        assert_eq!(context.path, store.path());
    }
}

// Real typed representatives for the presentation policy matrix below. No
// production failure can be constructed from a category and an arbitrary sentence.

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[test]
fn completion_notification_waits_for_real_worker_exit_and_preserves_join_ownership() {
    let (_directory, store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    let job = close_after_save(&mut workspace);
    let id = job.id();
    let release = Arc::new(Barrier::new(2));
    let worker_release = release.clone();
    let (computed, received) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = job.execute(|_| panic!("file only"));
        computed.send(()).unwrap();
        worker_release.wait();
        result
    });
    let (notified, notification) = mpsc::channel();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        notified.send(()).unwrap();
    });
    let mut runner = Runner::with_waker(wake.clone());
    let (terminal, completed) = mpsc::channel();
    let observer =
        thread::spawn(move || Runner::post_joined(id, Ok(worker), &terminal, wake.as_ref()));
    let (_, prepared) = mpsc::channel();
    runner.running.push(Running {
        id,
        handle: Worker::Notifying {
            observer,
            completed,
        },
        prepared,
    });
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(store.scan(&Cancel::default()).entries.len(), 1);
    assert!(notification.try_recv().is_err());
    assert!(runner.poll().is_empty());
    assert!(workspace.file_busy() && workspace.action_pending() && workspace.editor().dirty());
    release.wait();
    notification.recv_timeout(Duration::from_secs(5)).unwrap();
    for response in runner.poll() {
        workspace.respond(response);
    }
    workspace.poll();
    assert!(!workspace.file_busy());
    assert!(!workspace.editor().dirty());
    assert!(closed(&mut workspace, true));
    runner.shutdown(workspace).unwrap();
}
