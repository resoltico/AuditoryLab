// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Physical worker ownership and coordinator protocol tests without a device.
use super::*;
use crate::audio::session::{
    EndReason, LiveGenerator, MonitorDetail, Outcome, OutputInfo, PlaybackMonitor, SampleFormat,
    SessionDescriptor,
};
use crate::core::{CarrierKind, ParameterId, SampleRate};
use crate::document::editing::{EditingField, FieldHandle};
use crate::services::library::{Library, Mutation};
use crate::workspace::status::{AudioStatus, FileStatus};
use crate::workspace::{
    Capabilities, Effect, PromptAnswer, ResponseDisposition, View, WorkerFailure,
};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

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
    workspace.set_view(View::Workbench);
    workspace
        .add_component(workspace.editor().document_key(), CarrierKind::Tone)
        .unwrap();
    let id = workspace.editor().draft().components()[0].id;
    let handle = workspace
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ToneFrequency))
        .unwrap();
    workspace.commit_field(handle, hz).unwrap();
    handle
}
fn close_after_save(workspace: &mut Workspace) -> Job {
    workspace.close();
    let prompt = workspace
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::AskUnsaved(prompt) => Some(prompt),
            _ => None,
        })
        .unwrap();
    workspace.respond(Response::Prompt {
        prompt,
        answer: PromptAnswer::Save,
    });
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
) -> Result<(Response, LiveGenerator), crate::error::PlaybackError> {
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
                crate::error::PlaybackError::Sound {
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
    let generator = LiveGenerator::with_state(target, Arc::clone(session.state()));
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
        MonitorDetail::Quantities,
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
    let outcome = workspace.action_view().operation_details;
    assert!(outcome.contains("thread failed"));
    assert!(outcome.contains("does not establish that storage was unchanged"));
    assert!(outcome.contains(&store.path().display().to_string()));
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
    workspace.play().unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    workspace.stop().unwrap();
    let response = job.execute(|_| panic!("stopped audio must not prepare a device"));
    assert!(
        matches!(&response,Response::Finished {outcome:JobOutcome::Audio(Ok(report)),..} if report.end==EndReason::Stopped && report.generated_frames==0)
    );
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert!(!workspace.audio_busy());
    assert!(workspace.live_rate().is_none());
}
#[test]
fn actual_rate_preparation_submits_committed_edits_but_never_staged_text() {
    let (_directory, _store, mut workspace) = fixture();
    let frequency = tone(&mut workspace, "440");
    workspace.play().unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    workspace.commit_field(frequency, "30000").unwrap();
    workspace.stage(frequency, "500").unwrap();
    let (response, _generator) = prepared(&job, SampleRate::new(96000).unwrap()).unwrap();
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    let accepted = workspace
        .audio
        .slot
        .as_ref()
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
    assert!(!workspace.action_view().can_export);
    workspace.commit_field(frequency, "60000").unwrap();
    assert_eq!(
        workspace
            .audio
            .slot
            .as_ref()
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
    workspace.stop().unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn device_rate_validation_is_independent_of_export_rate() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "30000");
    workspace.set_export_rate(SampleRate::new(96000).unwrap());
    workspace.play().unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let error = match prepared(&job, SampleRate::new(8000).unwrap()) {
        Err(error) => error,
        Ok(_) => panic!("invalid at actual rate"),
    };
    assert_eq!(error.outcome(), crate::error::Outcome::InvalidSound);
    workspace.respond(Response::Finished {
        job: job.id(),
        outcome: JobOutcome::Audio(Err(error)),
    });
    assert!(!workspace.audio_busy());
    assert!(workspace.action_view().audio_status.contains("8000"));
    drop(job);
}
#[test]
fn stale_and_duplicate_preparations_cannot_rebind_audio() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.play().unwrap();
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
    assert!(workspace.audio.slot.as_ref().unwrap().protocol_failed);
    let response = job.execute(|_| panic!("rejected preparation requested Stop"));
    assert_eq!(workspace.respond(response), ResponseDisposition::Rejected);
    assert!(!workspace.audio_busy());
}
#[test]
fn wrong_preparation_kind_rate_and_session_are_rejected() {
    for fault in 0..3 {
        let (_directory, _store, mut workspace) = fixture();
        tone(&mut workspace, "440");
        workspace.play().unwrap();
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
                let foreign = LiveGenerator::with_state(target, Arc::clone(other.state()));
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
    workspace.play().unwrap();
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
    workspace.play().unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let old = workspace.editor().document();
    assert!(workspace.replace_document(Draft::new().unwrap()));
    assert_ne!(workspace.editor().document(), old);
    let (response, _generator) = prepared(&job, SampleRate::new(96000).unwrap()).unwrap();
    workspace.respond(response);
    assert!(workspace.live_rate().is_none());
    assert_eq!(workspace.diagnostic_rate(), workspace.export_rate());
    workspace.stop().unwrap();
    workspace.respond(job.execute(|_| {}));
}
#[test]
fn accepted_live_revision_changes_update_model_even_for_identical_audio() {
    let (_directory, _store, mut workspace) = fixture();
    workspace.set_view(View::Workbench);
    workspace
        .add_component(workspace.editor().document_key(), CarrierKind::Clicks)
        .unwrap();
    let id = workspace.editor().draft().components()[0].id;
    let width = workspace
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ClickWidth))
        .unwrap();
    workspace.commit_field(width, "1").unwrap();
    workspace.play().unwrap();
    let job = take_job(&mut workspace, JobKind::Audio);
    let (response, _generator) = prepared(&job, SampleRate::new(8000).unwrap()).unwrap();
    workspace.respond(response);
    workspace.poll();
    let first = workspace.model_view().unwrap();
    let models = workspace.audio.output.models;
    for _ in 0..20 {
        workspace.poll();
    }
    assert_eq!(workspace.audio.output.models, models);
    workspace.commit_field(width, "1.01").unwrap();
    workspace.poll();
    let second = workspace.model_view().unwrap();
    assert!(second.target.same_audio(&first.target));
    assert_ne!(second.target, first.target);
    assert_eq!(workspace.audio.output.models, models + 1);
    workspace.stop().unwrap();
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
        workspace.close();
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
            workspace.editor.location,
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
        .set_catalog_query(crate::discovery::collection::CatalogQuery {
            scope: crate::discovery::collection::Scope::User,
            ..Default::default()
        })
        .unwrap();
    idle_files(&mut workspace);
    assert_eq!(
        crate::storage::library::LibraryKey::User(
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
    workspace.refresh_library();
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
    assert!(workspace.library_alert().contains("read failed"));
    drop(load);
}

#[test]
fn invalid_editor_replacement_is_refused_before_becoming_a_snapshot() {
    let (_directory, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle_files(&mut workspace);
    let record = store.scan(&Cancel::default()).entries[0].key.clone();
    let original = workspace.editor().document();
    let location = workspace.document_location();
    let mut candidate = store
        .load(&record)
        .unwrap()
        .snapshot
        .recipe()
        .clone()
        .into_builder();
    let component = crate::core::Component {
        id: crate::core::ComponentId::new(1).unwrap(),
        form: crate::core::ComponentForm::Voice(crate::document::controls::default_component(
            CarrierKind::Tone,
        )),
    };
    candidate.sound.components = vec![component; 2];
    assert!(candidate.build().is_err());
    assert_eq!(workspace.editor().document(), original);
    assert_eq!(workspace.document_location(), location);
}

#[test]
fn shutdown_reports_prior_rejected_preparation_even_when_real_job_stops_ok() {
    let (_directory, _store, mut workspace) = fixture();
    tone(&mut workspace, "440");
    workspace.play().unwrap();
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
    assert!(
        runner
            .shutdown(workspace)
            .unwrap_err()
            .contains("another operation")
    );
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
    assert!(error.contains("another operation"));
    assert!(error.contains("library"));
}
#[test]
fn shutdown_does_not_reclassify_a_historical_resolved_failure() {
    let (_directory, _store, mut workspace) = fixture();
    workspace.files.status = FileStatus::Failed(crate::error::FileError::MissingFilename.into());
    workspace.audio.status = AudioStatus::WorkerFailed(WorkerFailure::Panicked);
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
        workspace.commit_field(field, "880").unwrap();
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
        let notice = if shutdown {
            runner.shutdown(workspace).unwrap_err()
        } else {
            pump(&mut runner, &mut workspace);
            assert!(workspace.editor().dirty());
            assert_eq!(
                workspace.editor().draft().components()[0]
                    .form
                    .read(ParameterId::ToneFrequency),
                Some(880.0)
            );
            assert!(closed(&mut workspace, false));
            workspace.action_view().operation_details
        };
        assert_eq!(std::fs::read(&path).unwrap(), concurrent_bytes);
        assert_eq!(
            store.load(&key).unwrap().snapshot.title(),
            "Concurrent writer"
        );
        assert!(notice.contains(&format!("Selector: {key}")), "{notice}");
        assert!(
            notice.contains(&format!("Location: {}", store.path().display())),
            "{notice}"
        );
        assert!(
            notice.contains("conflicts with the captured operation"),
            "{notice}"
        );
        assert!(notice.contains("did not publish a replacement"), "{notice}");
        assert!(notice.contains("Draft retained"), "{notice}");
        assert!(
            !notice.contains("inspect"),
            "a known conflict is not an uncertain publication: {notice}"
        );
    }
}

#[test]
fn library_failure_notices_keep_conflicts_refusals_and_publication_io_distinct() {
    let (_directory, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle_files(&mut workspace);
    let key = store.scan(&Cancel::default()).entries[0].key.clone();
    for (kind, publishing, inspect, replacement) in [
        (crate::error::Outcome::HistoryConflict, true, false, true),
        (crate::error::Outcome::Conflict, false, false, false),
        (crate::error::Outcome::HistoryConflict, false, false, false),
        (crate::error::Outcome::Write, true, true, false),
        (crate::error::Outcome::Unreadable, true, true, false),
        (
            crate::error::Outcome::IncompleteInventory,
            true,
            true,
            false,
        ),
        (crate::error::Outcome::Write, false, false, false),
        (crate::error::Outcome::Busy, true, false, false),
        (crate::error::Outcome::LockUnavailable, true, false, false),
        (crate::error::Outcome::Cancelled, true, false, false),
        (crate::error::Outcome::ReadOnlyBuiltIn, true, false, false),
        (crate::error::Outcome::InvalidRecipe, true, false, false),
        (crate::error::Outcome::InvalidSound, true, false, false),
        (crate::error::Outcome::InvalidArgument, true, false, false),
        (crate::error::Outcome::Identity, true, false, false),
    ] {
        let status = FileStatus::Library(super::super::status::LibraryStatus::Failed {
            error: captured_error(kind),
            publishing,
            context: super::super::status::LibraryContext {
                path: store.path().into(),
                selector: Some(key.clone()),
            },
        });
        let notice = status.library_failure_notice().unwrap();
        assert_eq!(
            notice.contains("inspect this location"),
            inspect,
            "{notice}"
        );
        assert_eq!(
            notice.contains("did not publish a replacement"),
            replacement,
            "{notice}"
        );
        assert!(
            notice.lines().next().unwrap().contains(&key.to_string()),
            "{notice}"
        );
        if !publishing
            && matches!(
                kind,
                crate::error::Outcome::Conflict | crate::error::Outcome::HistoryConflict
            )
        {
            assert!(notice.starts_with("Library read conflict"), "{notice}");
        }
        assert!(
            notice.contains(&store.path().display().to_string()),
            "{notice}"
        );
    }
}

// Real typed representatives for the presentation policy matrix below. No
// production failure can be constructed from a category and an arbitrary sentence.
fn captured_error(outcome: crate::error::Outcome) -> crate::error::OperationError {
    use crate::error::{
        FileError, IdentityError, IdentityScope, IoCause, LibraryError, LoadError, OperationError,
        Outcome,
    };
    let id = crate::services::catalog().unwrap()[0]
        .recipe()
        .document()
        .id;
    match outcome {
        Outcome::HistoryConflict => LibraryError::HistoryReplacement { id }.into(),
        Outcome::Conflict => LibraryError::StaleContent { id }.into(),
        Outcome::Write => FileError::MissingFilename.into(),
        Outcome::Unreadable => LoadError::Read {
            origin: crate::document::recipe::Origin::Stream("captured".into()),
            cause: IoCause::new(std::io::Error::other("read failed")),
        }
        .into(),
        Outcome::IncompleteInventory => LibraryError::RootMissing.into(),
        Outcome::Busy => LibraryError::Busy.into(),
        Outcome::LockUnavailable => LibraryError::LockUnavailable {
            error: IoCause::new(std::io::Error::other("locking unavailable")),
        }
        .into(),
        Outcome::Cancelled => LibraryError::Cancelled.into(),
        Outcome::ReadOnlyBuiltIn => LibraryError::ReadOnlyBuiltin {
            id: "captured".into(),
        }
        .into(),
        Outcome::InvalidRecipe => OperationError::Recipe(crate::core::RecipeError::NotUtf8),
        Outcome::InvalidSound => LoadError::Sound {
            origin: crate::document::recipe::Origin::Stream("captured".into()),
            rate: SampleRate::DEFAULT,
            error: crate::core::SoundError::Duration {
                seconds: 0.0,
                sample_rate_hz: 48000,
            },
        }
        .into(),
        Outcome::InvalidArgument => LibraryError::PurgeUnconfirmed.into(),
        Outcome::Identity => IdentityError::Exhausted(IdentityScope::ExternalEntry).into(),
        _ => panic!("category not used by this notice matrix"),
    }
}
