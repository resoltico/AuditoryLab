// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::session::SessionState;
use cpal::ErrorKind;
use std::sync::Barrier;

#[test]
fn discovery_requires_stereo_or_wider_and_a_supported_rate_and_format() {
    for channels in [0, 1, 2, 5, u16::MAX] {
        let snapshot = cpal::SupportedStreamConfig::new(
            channels,
            44_100,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::F32,
        );
        let result = supported_format(&snapshot);
        if channels < 2 {
            assert!(
                matches!(result, Err(DeviceError::TooFewChannels { channels: c }) if c == channels)
            );
        } else {
            assert!(matches!(result, Ok((rate, SampleFormat::F32)) if rate.hz() == 44_100));
        }
    }
    for hz in [0, 768_001] {
        let snapshot = cpal::SupportedStreamConfig::new(
            2,
            hz,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::F32,
        );
        assert!(
            matches!(supported_format(&snapshot), Err(DeviceError::UnsupportedSampleRate { hz: r }) if r == hz)
        );
    }
    let snapshot = cpal::SupportedStreamConfig::new(
        2,
        48_000,
        cpal::SupportedBufferSize::Unknown,
        cpal::SampleFormat::U16,
    );
    assert!(matches!(
        supported_format(&snapshot),
        Err(DeviceError::UnsupportedSampleFormat(
            cpal::SampleFormat::U16
        ))
    ));
    for (native, format) in [
        (cpal::SampleFormat::F64, SampleFormat::F64),
        (cpal::SampleFormat::F32, SampleFormat::F32),
        (cpal::SampleFormat::I32, SampleFormat::I32),
        (cpal::SampleFormat::I24, SampleFormat::I24),
        (cpal::SampleFormat::I16, SampleFormat::I16),
    ] {
        assert_eq!(sample_format(native), Some(format));
    }
}

#[test]
fn integer_formats_reach_but_never_pass_full_scale() {
    assert_eq!(i16::from_sample(-1.0_f64), -32_768);
    assert_eq!(i16::from_sample(1.0_f64), 32_767);
    assert_eq!(i32::from_sample(-1.0_f64), -2_147_483_648);
    assert_eq!(i32::from_sample(1.0_f64), 2_147_483_647);
    assert_eq!(to_i24(-1.0).inner(), -8_388_608);
    assert_eq!(to_i24(1.0).inner(), 8_388_607);
    assert_eq!(to_i24(0.5).inner(), 4_194_304);
    assert_eq!(&to_i24(1.0).inner().to_le_bytes()[..3], &[0xff, 0xff, 0x7f]);
    // Direct multiplication/rounding exceeds the signed 24-bit range.
    assert_eq!(cpal::I24::from_sample(1.0_f64).inner(), 8_388_608);
    let step24 = 1.0 / 8_388_608.0;
    for (steps, expected) in [(0.5, 0), (-0.5, -1), (1.5, 1), (-1.5, -2)] {
        assert_eq!(to_i24(steps * step24).inner(), expected);
    }
}

#[test]
fn a_route_change_says_what_happened_and_what_to_do() {
    let changed = DeviceError::from(cpal::Error::new(ErrorKind::DeviceChanged));
    assert_eq!(
        changed.to_string(),
        "the audio output route changed and playback stopped; select the intended output and play again"
    );
    let busy = DeviceError::from(cpal::Error::with_message(ErrorKind::DeviceBusy, "in use"));
    assert_eq!(busy.to_string(), "audio backend: in use");
    assert_eq!(
        DeviceError::TooFewChannels { channels: 1 }.to_string(),
        "the default output device has 1 channel; stimuli need two, one for each ear"
    );
}

#[test]
fn a_paused_backend_writer_cannot_block_or_replace_an_earlier_winner() {
    for winner in [
        Outcome::Completed,
        Outcome::StoppedDrained,
        Outcome::StreamSetupFailed,
    ] {
        let state = SessionState::new();
        let diagnostics = Arc::new(Diagnostics::new(Arc::clone(&state)));
        let claimed = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let writer = {
            let (d, c, r) = (
                Arc::clone(&diagnostics),
                Arc::clone(&claimed),
                Arc::clone(&resume),
            );
            std::thread::spawn(move || {
                assert!(d.claim_for_test());
                c.wait();
                r.wait();
                d.publish_for_test(cpal::Error::new(ErrorKind::DeviceChanged));
            })
        };
        claimed.wait();
        // A second report cannot claim the occupied slot or block.
        diagnostics.report(cpal::Error::new(ErrorKind::DeviceBusy));
        state.finish(winner);
        assert_eq!(state.outcome(), winner);
        resume.wait();
        writer.join().unwrap();
        assert_eq!(
            state.outcome(),
            winner,
            "the delayed diagnostic never wins late"
        );
    }
}

#[test]
fn a_winning_backend_failure_carries_its_own_payload() {
    let state = SessionState::new();
    let diagnostics = Diagnostics::new(Arc::clone(&state));
    diagnostics.report(cpal::Error::new(ErrorKind::Xrun));
    diagnostics.report(cpal::Error::new(ErrorKind::RealtimeDenied));
    assert_eq!(state.underruns(), 1);
    assert!(state.outcome().is_active(), "underruns are not failures");
    diagnostics.report(cpal::Error::with_message(
        ErrorKind::DeviceNotAvailable,
        "unplugged",
    ));
    diagnostics.report(cpal::Error::new(ErrorKind::DeviceBusy));
    assert_eq!(state.outcome(), Outcome::Backend);
    let resolved = resolve(&state, Outcome::Backend, None, &diagnostics).unwrap_err();
    assert_eq!(resolved.to_string(), "audio backend: unplugged");
    // A setup failure resolves to its worker-local diagnostic.
    let state = SessionState::new();
    let diagnostics = Diagnostics::new(Arc::clone(&state));
    let mut local = None;
    let winner = setup_failed(
        &state,
        cpal::Error::with_message(ErrorKind::DeviceBusy, "original setup failure"),
        &mut local,
    );
    let resolved = resolve(&state, winner, local, &diagnostics).unwrap_err();
    assert_eq!(
        resolved.to_string(),
        "audio backend: original setup failure"
    );
}

#[test]
fn stop_before_stream_start_reports_an_early_drained_stop() {
    let session = Session::new();
    let control = session.control();
    assert_eq!(stopped_before_start(&session), None);
    control.request_stop();
    let report = stopped_before_start(&session).unwrap();
    assert_eq!(report.end, crate::session::EndReason::Stopped);
    assert_eq!(
        (report.generated_frames, report.estimated_output_frames),
        (0, 0)
    );
    assert!(report.drained);
    assert_eq!(control.phase(), crate::session::PlaybackPhase::Stopped);
}

#[cfg(target_os = "linux")]
#[test]
fn backend_recovery_can_run_before_the_stall_deadline() {
    let state = SessionState::new();
    let mut supervision = Supervision::new(None);
    assert!(supervision.poll(&state, Duration::ZERO).is_none());
    let recovered_at = STREAM_TIMEOUT + POLL_INTERVAL;
    assert!(supervision.poll(&state, recovered_at).is_none());
    state.mark_progress();
    assert!(supervision.poll(&state, recovered_at).is_none());
    assert!(
        supervision
            .poll(&state, recovered_at + STALL_LIMIT - POLL_INTERVAL)
            .is_none()
    );
    assert_eq!(
        supervision.poll(&state, recovered_at + STALL_LIMIT),
        Some(Outcome::TimedOut)
    );
}
