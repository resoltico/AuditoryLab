// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use auditorylab_core::{ComponentForm, ComponentId, Ears, SampleRate, Target};
use std::sync::Barrier;

const TERMINALS: [Outcome; 11] = [
    Outcome::Completed,
    Outcome::StoppedDrained,
    Outcome::StoppedUndrained,
    Outcome::FrameLimit,
    Outcome::PreparationFailed,
    Outcome::StreamSetupFailed,
    Outcome::Backend,
    Outcome::InvalidTiming,
    Outcome::MisalignedBuffer,
    Outcome::TimedOut,
    Outcome::Abandoned,
];

fn tone(frequency_hz: f64) -> Target {
    crate::core::Sound::new(
        finite(1.0),
        finite(0.0),
        vec![crate::core::Component {
            id: ComponentId::new(1).unwrap(),
            form: ComponentForm::Tone {
                frequency_hz: finite(frequency_hz),
                ears: Ears::Both,
                peak_dbfs: finite(-30.0),
                modulation: None,
            },
        }],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .target(SampleRate::new(8_000).unwrap())
    .unwrap()
}

#[test]
fn stop_is_accepted_once_and_never_after_a_terminal_outcome() {
    let state = SessionState::new();
    assert_eq!(state.request_stop(), StopRequest::Accepted);
    assert_eq!(state.request_stop(), StopRequest::AlreadyRequested);
    assert_eq!(state.outcome(), Outcome::StopRequested);
    // A completed drain after accepted Stop is Stopped, not Completed.
    assert_eq!(
        state.finish_drained(Outcome::Completed),
        Outcome::StoppedDrained
    );
    assert_eq!(state.request_stop(), StopRequest::AlreadyFinished);
    // Completion published first cannot be cancelled.
    let state = SessionState::new();
    assert_eq!(state.finish_drained(Outcome::Completed), Outcome::Completed);
    assert_eq!(state.request_stop(), StopRequest::AlreadyFinished);
    assert_eq!(state.outcome(), Outcome::Completed);
    // An actual failure wins from either active state.
    for stop in [false, true] {
        let state = SessionState::new();
        if stop {
            state.request_stop();
        }
        assert_eq!(state.finish(Outcome::Backend), Outcome::Backend);
        assert_eq!(state.finish_drained(Outcome::Completed), Outcome::Backend);
        assert!(state.report().is_none(), "a failure has no success report");
    }
    // The forced cutoff applies only to an accepted Stop.
    let state = SessionState::new();
    assert_eq!(state.finish_stop_undrained(), Outcome::Running);
}

#[test]
fn every_pair_of_terminal_publications_keeps_the_first() {
    for first in TERMINALS {
        for second in TERMINALS {
            let state = SessionState::new();
            assert_eq!(state.finish(first), first);
            assert_eq!(state.finish(second), first);
            assert_eq!(state.finish_drained(Outcome::Completed), first);
            assert_eq!(state.outcome(), first);
        }
    }
}

#[test]
fn racing_stop_and_completion_agree_on_one_consistent_winner() {
    for _ in 0..200 {
        let state = SessionState::new();
        let barrier = Arc::new(Barrier::new(2));
        let (s, b) = (Arc::clone(&state), Arc::clone(&barrier));
        let stopper = std::thread::spawn(move || {
            b.wait();
            s.request_stop()
        });
        barrier.wait();
        let terminal = state.finish_drained(Outcome::Completed);
        let stop = stopper.join().unwrap();
        match (stop, terminal) {
            (StopRequest::Accepted, Outcome::StoppedDrained)
            | (StopRequest::AlreadyFinished, Outcome::Completed) => {}
            other => panic!("inconsistent arbitration {other:?}"),
        }
        assert_eq!(state.outcome(), terminal);
    }
}

#[test]
fn the_phase_follows_terminal_then_draining_then_stopping_then_generation() {
    let state = SessionState::new();
    assert_eq!(state.phase(), PlaybackPhase::Preparing);
    state.set_generated(10);
    assert_eq!(state.phase(), PlaybackPhase::Running);
    state.request_stop();
    assert_eq!(state.phase(), PlaybackPhase::Stopping);
    state.end_source();
    assert_eq!(
        state.phase(),
        PlaybackPhase::Draining,
        "a late Stop keeps draining"
    );
    state.finish_drained(Outcome::Completed);
    assert_eq!(state.phase(), PlaybackPhase::Stopped);
    let limit = SessionState::new();
    limit.end_source();
    limit.finish_drained(Outcome::FrameLimit);
    assert_eq!(limit.phase(), PlaybackPhase::FrameLimit);
    assert_eq!(limit.report().unwrap().end, EndReason::FrameLimit);
}

#[test]
fn an_abandoned_owner_closes_its_lifecycle_but_never_replaces_an_outcome() {
    let session = Session::new();
    let control = session.control();
    let copy = control.clone();
    assert_eq!(
        copy.session(),
        session.id(),
        "clones address the same session"
    );
    drop(session);
    assert_eq!(control.phase(), PlaybackPhase::Failed);
    assert_eq!(control.request_stop(), StopRequest::AlreadyFinished);
    let session = Session::new();
    session.state().finish(Outcome::Completed);
    let control = session.control();
    drop(session);
    assert_eq!(control.phase(), PlaybackPhase::Completed);
}

#[test]
fn live_admission_closes_at_stop_source_end_and_generator_drop() {
    let target = tone(440.0);
    // Accepted Stop.
    let mut generator = LiveGenerator::new(&(target));
    let control = generator.control();
    assert_eq!(control.update(&tone(441.0)), Ok(2));
    assert_eq!(
        control.update(&tone(441.0)),
        Ok(2),
        "duplicates keep their revision"
    );
    assert_eq!(generator.stop(), StopRequest::Accepted);
    assert_eq!(
        control.update(&tone(442.0)),
        Err(LiveUpdateError::SessionClosed)
    );
    // Generation end without Stop (the engine stopped through the shared word).
    let mut out = std::vec![[0.0; 2]; 4_000];
    generator.render(&mut out);
    assert!(generator.is_finished());
    assert_eq!(
        generator.stop(),
        StopRequest::AlreadyRequested,
        "harmless after the end"
    );
    // Generator drop closes a surviving control.
    let generator = LiveGenerator::new(&(target));
    let control = generator.control();
    drop(generator);
    assert_eq!(
        control.update(&tone(443.0)),
        Err(LiveUpdateError::SessionClosed)
    );
    // Another rate is refused before anything changes.
    let generator = LiveGenerator::new(&(target));
    let other = Target::silence(SampleRate::new(16_000).unwrap());
    assert_eq!(
        generator.control().update(&other),
        Err(LiveUpdateError::RateMismatch {
            target_hz: 16_000,
            session_hz: 8_000
        })
    );
    assert_eq!(generator.control().accepted(), Some((1, target)));
}

#[test]
fn an_update_accepted_before_stop_may_never_generate() {
    let mut generator = LiveGenerator::new(&(tone(440.0)));
    let control = generator.control();
    let mut out = [[0.0; 2]; 512];
    generator.render(&mut out);
    assert_eq!(control.update(&tone(880.0)), Ok(2));
    generator.stop();
    for _ in 0..20 {
        generator.render(&mut out);
    }
    assert!(generator.is_finished());
    let last = generator.observation().unwrap();
    assert_eq!(
        last.active_revision, 1,
        "accepted revision 2 never generated"
    );
    assert_eq!(
        control.accepted().unwrap().0,
        2,
        "acceptance remains a fact"
    );
}

#[test]
fn the_revision_counter_fails_explicitly_instead_of_wrapping() {
    let generator = LiveGenerator::new(&(tone(440.0)));
    generator.mailbox().lock().unwrap().accepted = Some((u64::MAX, tone(440.0)));
    assert_eq!(
        generator.control().update(&tone(441.0)),
        Err(LiveUpdateError::RevisionLimit)
    );
}

#[test]
fn standalone_generation_has_no_transport_and_publishes_its_end() {
    let mut generator = LiveGenerator::new(&(Target::silence(SampleRate::new(8_000).unwrap())));
    let control = generator.control();
    let mut out = [[1.0; 2]; 800];
    assert_eq!(generator.render(&mut out), 800);
    assert!(!generator.is_finished());
    generator.stop();
    let mut frames = 0;
    for _ in 0..10 {
        frames += generator.render(&mut out);
    }
    assert!(
        generator.is_finished(),
        "ended after {frames} frames without a supervisor"
    );
    assert_eq!(
        control.update(&tone(440.0)),
        Err(LiveUpdateError::SessionClosed)
    );
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
