// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::audio::session::live::LiveGenerator;
use crate::audio::session::source::{FiniteSource, Source};
use crate::audio::session::transport::{BufferTime, Transport};
use crate::audio::session::{Outcome, SampleFormat};
use crate::core::{ComponentForm, Ears, Sound};
use crate::document::recipe::Origin;
fn descriptor(hz: u32) -> SessionDescriptor {
    SessionDescriptor {
        id: SessionState::new().id(),
        title: "test".into(),
        origin: Origin::Stream("synthetic".into()),
        output: OutputInfo {
            name: Some("synthetic callback".into()),
            channels: 2,
            sample_rate: SampleRate::new(hz).unwrap(),
            format: SampleFormat::F64,
        },
        frames: Some(u64::from(hz)),
        target: Target::silence(SampleRate::new(hz).unwrap()),
    }
}
fn numbered(rate: SampleRate, components: &[ComponentForm]) -> Target {
    let components = components
        .iter()
        .enumerate()
        .map(|(i, c)| auditorylab_core::Component {
            id: auditorylab_core::ComponentId::numbered(i).unwrap(),
            form: *c,
        })
        .collect();
    Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components,
    }
    .target(rate)
    .unwrap()
}
fn finite(signal: crate::core::Signal) -> Source {
    Source::Finite(FiniteSource::new(Arc::new(signal)))
}
fn signal(hz: u32) -> crate::core::Signal {
    Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: (vec![ComponentForm::Tone {
            frequency_hz: 100.0,
            ears: Ears::Left,
            peak_dbfs: -20.0,
            modulation: None,
        }])
        .into_iter()
        .enumerate()
        .map(|(index, voice)| auditorylab_core::Component {
            id: auditorylab_core::ComponentId::numbered(index).unwrap(),
            form: voice,
        })
        .collect(),
    }
    .signal(SampleRate::new(hz).unwrap())
    .unwrap()
}
fn time(nanos: u64) -> BufferTime {
    let at = Duration::from_nanos(nanos);
    BufferTime {
        callback: at,
        playback: at,
    }
}
#[test]
fn rms_of_one_impulse_in_four_is_half_the_peak_at_tiny_and_large_levels() {
    for level in [1.0_f64, 1e-200, 1e200] {
        let samples = [[level, -level], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
        let meters = sample_meters(&samples);
        let expected_peak = 20.0 * level.log10();
        let expected_rms = expected_peak - 6.020599913279624;
        for meter in meters {
            assert!((meter.peak_dbfs - expected_peak).abs() < 1e-10);
            assert!((meter.rms_dbfs - expected_rms).abs() < 1e-10);
        }
    }
}
#[test]
fn real_callback_samples_match_history_and_finite_stop() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut reader = monitor.reader().unwrap();
    let mut a = Transport::new(finite(signal(8000)), 2, monitor.session())
        .with_monitor(Some(monitor.clone()));
    let other = SessionState::new();
    let mut b = Transport::new(finite(signal(8000)), 2, other.clone());
    let mut x = [0.0; 1024];
    let mut y = x;
    a.fill(&mut x, time(0), &|v| v);
    b.fill(&mut y, time(0), &|v| v);
    assert_eq!(x, y);
    assert!(reader.poll(100));
    assert_eq!(
        reader.samples(),
        x.as_chunks::<2>()
            .0
            .iter()
            .map(|f| [f[0], f[1]])
            .collect::<Vec<_>>()
    );
    let frozen = crate::presentation::waveform_view::WaveformCapture::monitored(
        &reader,
        crate::presentation::waveform_view::WaveformScale::Fit,
    )
    .unwrap()
    .unwrap();
    let caption = frozen.caption(true, false);
    let captured_meters = frozen.meters();
    let captured_id = frozen.session().unwrap().id;
    assert_eq!(frozen.span(), (0, 512));
    assert_eq!(frozen.observation().unwrap().frame, 511);
    a.fill(&mut x, time(64_000_000), &|v| v);
    b.fill(&mut y, time(64_000_000), &|v| v);
    assert_eq!(x, y);
    assert!(reader.poll(100));
    assert_eq!(reader.status().generated_frames, 1024);
    a.fill(&mut x, time(128_000_000), &|v| v);
    b.fill(&mut y, time(128_000_000), &|v| v);
    assert_eq!(x, y);
    reader.poll(100);
    assert_eq!(reader.status().gaps, 0);
    assert_eq!(reader.status().underruns, 0);
    assert_eq!(reader.status().window_start, 736);
    assert_eq!(reader.samples().len(), 800);
    monitor.0.session.request_stop();
    other.request_stop();
    a.fill(&mut x, time(192_000_000), &|v| v);
    b.fill(&mut y, time(192_000_000), &|v| v);
    assert_eq!(x, y);
    reader.poll(100);
    assert_eq!(reader.status().phase, PlaybackPhase::Draining);
    a.fill(&mut x, time(256_000_000), &|v| v);
    b.fill(&mut y, time(256_000_000), &|v| v);
    assert_eq!(x, y);
    reader.poll(100);
    assert_eq!(reader.status().phase, PlaybackPhase::Stopped);
    let latest = crate::presentation::waveform_view::WaveformCapture::monitored(
        &reader,
        crate::presentation::waveform_view::WaveformScale::Fixed,
    )
    .unwrap()
    .unwrap();
    assert_ne!(latest.span(), frozen.span());
    assert!(latest.sequence() > frozen.sequence());
    assert_eq!(frozen.caption(true, false), caption);
    assert_eq!(frozen.meters(), captured_meters);
    assert_eq!(frozen.rate().hz(), 8000);
    assert_eq!(frozen.session().unwrap().id, captured_id);
    assert_eq!(
        frozen.scale(),
        crate::presentation::waveform_view::WaveformScale::Fit
    );
}
#[test]
fn ring_limits_wrap_large_writes_and_age_are_bounded() {
    for hz in [1, 768000] {
        let m = PlaybackMonitor::new(
            descriptor(hz),
            MonitorDetail::Waveform,
            SessionState::new(),
            None,
        );
        let cap = capacity(SampleRate::new(hz).unwrap());
        let mut p = m.writer().unwrap();
        let data = vec![[0.5, -0.25]; cap + 17];
        {
            p.append(0, &data);
            p.finish(data.len() as u64, None);
        }
        let mut r = m.reader().unwrap();
        assert!(r.poll(100));
        assert_eq!(r.samples().len(), cap);
        assert_eq!(r.status().window_start, 0);
        assert_eq!(r.status().gaps, 1);
        let received = r.received;
        r.poll(100);
        assert_eq!(r.received, received);
        p.append(data.len() as u64, &[[0.25, -0.125]]);
        p.finish(data.len() as u64 + 1, None);
        r.poll(100);
        assert_eq!(r.status().gaps, 1);
        {
            p.append(data.len() as u64 + 100, &[[0.0; 2]]);
            p.finish(data.len() as u64 + 101, None);
        }
        r.poll(100);
        assert_eq!(r.samples(), [[0.0; 2]]);
        assert_eq!(r.status().gaps, 2);
    }
}
#[test]
fn endpoints_are_claimed_once_before_callback_use() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut writer = monitor.writer().unwrap();
    assert!(monitor.writer().is_none());
    let mut reader = monitor.reader().unwrap();
    assert!(monitor.reader().is_none());
    writer.append(0, &[[0.5, -0.25]]);
    writer.finish(1, None);
    assert!(reader.poll(100));
    assert_eq!(reader.samples(), [[0.5, -0.25]]);
    monitor.session().finish(Outcome::Backend);
    reader.poll(100);
    assert_eq!(reader.status().phase, PlaybackPhase::Failed);
}

#[test]
fn callbacks_and_preallocated_reads_allocate_nothing() {
    let m = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut reader = m.reader().unwrap();
    let mut callback = Transport::new(finite(signal(8000)), 2, m.session()).with_monitor(Some(m));
    let mut out = [0.0; 4096];
    let measured = allocation_counter::measure(|| {
        callback.fill(&mut out, time(0), &|v| v);
        reader.poll(100);
        std::hint::black_box(sample_meters(reader.samples()));
    });
    assert_eq!(measured.count_total, 0);
    let m = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Quantities,
        SessionState::new(),
        None,
    );
    assert!(m.reader().unwrap().samples.is_empty());
}
#[test]
fn digital_meters_and_extrema_use_the_entire_window() {
    let frames = [[0.5, 0.0], [-0.5, 0.0]];
    let meters = sample_meters(&frames);
    assert!((meters[0].peak_dbfs + 6.020599913279624).abs() < 1e-12);
    assert_eq!(meters[0].rms_dbfs, meters[0].peak_dbfs);
    assert_eq!(meters[1].peak_dbfs, f64::NEG_INFINITY);
    assert_eq!(sample_meters(&[])[0].rms_dbfs, f64::NEG_INFINITY);
    let mut data = [[0.0; 2]; 1024];
    data[500][0] = 1.0;
    let mut bins = Vec::new();
    auditorylab_core::waveform::sample_trace(&data, 16, &mut bins).unwrap();
    assert!(bins.iter().any(|b| b[0].max == 1.0));
}
#[test]
fn actual_live_crossfades_and_stop_match_with_or_without_monitoring() {
    let rate = SampleRate::DEFAULT;
    let tones: Vec<ComponentForm> = (0..16)
        .map(|n| ComponentForm::Tone {
            frequency_hz: 200.0 + f64::from(n) * 20.0,
            ears: Ears::Both,
            peak_dbfs: -30.0,
            modulation: None,
        })
        .collect();
    let clicks: Vec<ComponentForm> = tones
        .iter()
        .map(|c| {
            let base = *c;
            ComponentForm::Clicks {
                rate_hz: 40.0,
                width_seconds: 0.001,
                ears: base.route().unwrap(),
                peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                modulation: base.modulation(),
            }
        })
        .collect();
    let monitored = SessionState::new();
    let other = SessionState::new();
    let a = LiveGenerator::with_state(numbered(rate, &tones), Arc::clone(&monitored));
    let b = LiveGenerator::with_state(numbered(rate, &tones), Arc::clone(&other));
    let ca = a.control();
    let cb = b.control();
    let mut desc = descriptor(rate.hz());
    desc.frames = None;
    let monitor = PlaybackMonitor::new(
        desc,
        MonitorDetail::Waveform,
        Arc::clone(&monitored),
        Some(Arc::clone(a.mailbox())),
    );
    let mut reader = monitor.reader().unwrap();
    let mut a =
        Transport::new(Source::Live(Box::new(a)), 2, monitored).with_monitor(Some(monitor.clone()));
    let mut b = Transport::new(Source::Live(Box::new(b)), 2, other.clone());
    let mut x = [0.0; 2048];
    let mut y = x;
    for block in 0..12_u64 {
        if block == 4 {
            let target = numbered(rate, &clicks);
            assert_eq!(ca.update(&target).unwrap(), 2);
            assert_eq!(cb.update(&target).unwrap(), 2);
        }
        if block == 8 {
            monitor.0.session.request_stop();
            other.request_stop();
        }
        a.fill(&mut x, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
        b.fill(&mut y, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
        assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
        reader.poll(100);
        if block == 4 {
            let obs = reader.status().observation.unwrap();
            assert_eq!(obs.voices.iter().flatten().count(), 32);
            assert_eq!(reader.status().accepted_revision, 2);
            assert_eq!(obs.active_revision, 2);
            assert_eq!(obs.settled_revision, 1);
        }
    }
    assert_eq!(reader.status().phase, PlaybackPhase::Stopped);
    assert_eq!(reader.status().gaps, 0);
    assert_eq!(reader.status().underruns, 0);
}
#[test]
fn a_real_stalled_reader_never_blocks_callback_completion() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let reader = monitor.reader().unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        reader
    });
    ready_rx.recv().unwrap();
    let mut callback = Transport::new(finite(signal(8000)), 2, monitor.session())
        .with_monitor(Some(monitor.clone()));
    let mut output = [0.0; 1024];
    for n in 0..10 {
        callback.fill(&mut output, time(n * 64_000_000), &|v| v);
    }
    release_tx.send(()).unwrap();
    let mut reader = holder.join().unwrap();
    assert!(reader.poll(100));
    assert_eq!(reader.status().generated_frames, 5120);
    assert_eq!(
        reader.status().observation.unwrap().frame,
        5119,
        "state publication retains the latest observation during a sample-ring stall"
    );
    assert_eq!(reader.status().phase, PlaybackPhase::Running);
    assert_eq!(reader.status().gaps, 1);
    assert_eq!(
        reader.status().window_end,
        800,
        "full sample ring drops new frames rather than overwriting unread samples"
    );
    assert!(!reader.has_current_window());
    assert!(
        crate::presentation::waveform_view::WaveformCapture::monitored(
            &reader,
            crate::presentation::waveform_view::WaveformScale::Fit
        )
        .unwrap()
        .is_none()
    );
    for n in 10..12 {
        callback.fill(&mut output, time(n * 64_000_000), &|v| v);
        reader.poll(100);
    }
    assert!(reader.has_current_window());
    assert_eq!(reader.samples().len(), 800);
    assert_eq!(reader.status().window_start, 5344);
    assert_eq!(reader.status().window_end, 6144);
    assert_eq!(reader.status().gaps, 1);
}

#[test]
fn unchanged_receipts_do_not_hide_stale_observations() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Quantities,
        SessionState::new(),
        None,
    );
    let mut reader = monitor.reader().unwrap();
    assert_eq!(reader.freshness(), ObservationFreshness::Waiting);
    {
        let mut p = monitor.writer().unwrap();
        p.finish(1, None);
    }
    reader.poll(0);
    assert_eq!(reader.freshness(), ObservationFreshness::Current);
    reader.received = Some(Instant::now() - Duration::from_secs(1));
    reader.poll(0);
    assert_eq!(reader.freshness(), ObservationFreshness::Stale);
}
#[test]
#[ignore = "optimized callback median cost gate; qualify with --profile dist; no device is opened"]
#[expect(
    clippy::assertions_on_constants,
    reason = "The ignored cost gate rejects a debug-profile invocation instead of recording misleading regression measurements"
)]
fn callback_cost() {
    assert!(
        !cfg!(debug_assertions),
        "run callback_cost with --profile dist"
    );
    const WARMUP: u64 = 100;
    const MEASURED: usize = 3000;
    let mut failures = Vec::new();
    let mut configurations = 0;
    println!(
        "callback_cost os={} arch={} warmup={WARMUP} measured={MEASURED} median_limit_percent=50",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for hz in [48_000, 768_000] {
        for frames in [64_usize, 256, 1024, 4096] {
            for detail in [
                None,
                Some(MonitorDetail::Quantities),
                Some(MonitorDetail::Waveform),
            ] {
                let rate = SampleRate::new(hz).unwrap();
                let components: Vec<ComponentForm> = (0..16)
                    .map(|n| ComponentForm::Tone {
                        frequency_hz: 200.0 + f64::from(n) * 20.0,
                        ears: Ears::Both,
                        peak_dbfs: -30.0,
                        modulation: Some(crate::core::Modulation::Am {
                            rate_hz: 23.0 + f64::from(n) * 0.75,
                            depth: 1.0,
                        }),
                    })
                    .collect();
                let tones = numbered(rate, &components);
                let pulses = numbered(
                    rate,
                    &components
                        .iter()
                        .enumerate()
                        .map(|(n, c)| {
                            let base = *c;
                            ComponentForm::Clicks {
                                rate_hz: 40.0 + f64::from(u32::try_from(n).unwrap()),
                                width_seconds: 0.001,
                                ears: base.route().unwrap(),
                                peak_dbfs: base
                                    .get(auditorylab_core::ParameterId::PeakLevel)
                                    .unwrap(),
                                modulation: base.modulation(),
                            }
                        })
                        .collect::<Vec<_>>(),
                );
                let shared = SessionState::new();
                let generator = LiveGenerator::with_state(tones, Arc::clone(&shared));
                let control = generator.control();
                let mut desc = descriptor(hz);
                desc.frames = None;
                let monitor = detail.map(|d| {
                    PlaybackMonitor::new(
                        desc,
                        d,
                        Arc::clone(&shared),
                        Some(Arc::clone(generator.mailbox())),
                    )
                });
                let mut reader = monitor.as_ref().and_then(PlaybackMonitor::reader);
                let mut callback =
                    Transport::new(Source::Live(Box::new(generator)), 2, Arc::clone(&shared))
                        .with_monitor(monitor);
                let mut output = vec![0.0; frames * 2];
                let mut timings = Vec::with_capacity(MEASURED);
                let mut next_poll = 0;
                for n in 0..WARMUP + u64::try_from(MEASURED).unwrap() {
                    control
                        .update(if n % 2 == 0 { &pulses } else { &tones })
                        .unwrap();
                    let start = Instant::now();
                    callback.fill(
                        &mut output,
                        time(n * frames as u64 * 1_000_000_000 / u64::from(hz)),
                        &|v| v,
                    );
                    let elapsed = start.elapsed().as_nanos();
                    if n >= WARMUP {
                        timings.push(elapsed);
                    }
                    if n * frames as u64 >= next_poll {
                        if let Some(reader) = reader.as_mut() {
                            reader.poll(100);
                        }
                        next_poll = n * frames as u64 + u64::from(hz) / 20;
                    }
                }
                assert_eq!(timings.len(), MEASURED);
                timings.sort_unstable();
                // Keep the even-sized median exact, including a half-nanosecond.
                let median_twice = timings[(MEASURED - 1) / 2] + timings[MEASURED / 2];
                let median = median_twice / 2;
                let median_fraction = if median_twice.is_multiple_of(2) {
                    ""
                } else {
                    ".5"
                };
                let p99 = timings[(MEASURED * 99).div_ceil(100) - 1];
                let maximum = timings[MEASURED - 1];
                let period_numerator = u128::from(frames as u64) * 1_000_000_000;
                let period = period_numerator / u128::from(hz);
                if median_twice * u128::from(hz) > period_numerator {
                    failures.push(format!("{hz} Hz / {frames} frames / {detail:?}: median {median}{median_fraction} ns exceeds 50% of the buffer period"));
                }
                println!(
                    "rate_hz={hz} frames={frames} monitor={detail:?} median_ns={median}{median_fraction} p99_ns={p99} max_ns={maximum} buffer_period_ns={period} gaps={} backend_underruns={}",
                    reader.as_ref().map_or(0, |r| r.status().gaps),
                    shared.underruns()
                );
                configurations += 1;
            }
        }
    }
    assert_eq!(configurations, 24);
    assert!(
        failures.is_empty(),
        "callback median cost gate failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn current_impairment_recovers_without_erasing_historical_gaps_or_held_identity() {
    use crate::presentation::waveform_view::{
        OutputMonitorState as State, WaveformCapture, WaveformScale, monitor_warning,
        output_monitor_state,
    };
    let monitor = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut reader = monitor.reader().unwrap();
    let mut writer = monitor.writer().unwrap();
    reader.poll(40);
    assert_eq!(
        output_monitor_state(Some(&reader), None, false, true),
        State::Waiting
    );
    {
        writer.append(0, &[[0.0; 2]; 512]);
        writer.finish(512, None);
    }
    monitor.0.session.set_generated(512);
    reader.poll(40);
    // A fresh status publication does not by itself provide a valid plotted capture.
    assert_eq!(
        output_monitor_state(Some(&reader), None, false, false),
        State::Waiting
    );
    assert_eq!(
        monitor_warning(&reader, 40),
        Some(crate::presentation::waveform_view::MonitorWarning::IncompleteCoverage)
    );
    // A missed span is history; enough subsequent contiguous data repairs current coverage.
    {
        writer.append(1024, &[[0.0; 2]; 2400]);
        writer.finish(3424, None);
    }
    monitor.0.session.set_generated(3424);
    reader.poll(40);
    assert_eq!(reader.status().gaps, 1);
    assert_eq!(reader.status().underruns, 0);
    assert_eq!(monitor_warning(&reader, 40), None);
    let held = WaveformCapture::monitored(&reader, WaveformScale::Fit)
        .unwrap()
        .unwrap();
    assert_eq!(
        output_monitor_state(Some(&reader), Some(&held), false, false),
        State::Live
    );
    reader.received = Some(Instant::now() - Duration::from_secs(1));
    reader.poll(40);
    assert_eq!(
        output_monitor_state(Some(&reader), Some(&held), false, false),
        State::Stale
    );
    assert_eq!(
        monitor_warning(&reader, 40),
        Some(crate::presentation::waveform_view::MonitorWarning::Stale)
    );
    let prior = held.caption(true, false);
    let another = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut other = another.reader().unwrap();
    other.poll(40);
    assert_ne!(other.descriptor().id, held.session().unwrap().id);
    assert_eq!(
        output_monitor_state(Some(&other), Some(&held), true, true),
        State::Held
    );
    assert_eq!(held.caption(true, false), prior);
    {
        writer.append(3424, &[[0.0; 2]; 512]);
        writer.finish(3936, None);
    }
    monitor.0.session.set_generated(3936);
    reader.poll(40);
    assert_eq!(monitor_warning(&reader, 40), None);
    assert_eq!(reader.status().gaps, 1);
    monitor.0.session.finish(Outcome::Completed);
    reader.poll(40);
    assert_eq!(
        output_monitor_state(Some(&reader), Some(&held), false, false),
        State::LastCapture
    );
    assert_eq!(
        output_monitor_state(None, Some(&held), false, false),
        State::Preview
    );
    assert_eq!(
        output_monitor_state(None, None, false, false),
        State::Unavailable
    );
    assert_eq!(output_monitor_state(None, None, true, true), State::Waiting);
    let underrun = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    underrun.0.session.count_underrun();
    let mut reader = underrun.reader().unwrap();
    reader.poll(40);
    assert_eq!(monitor_warning(&reader, 40), None);
    assert!(
        crate::presentation::waveform_view::output_underrun_warning(reader.status().underruns)
            .contains("underruns: 1")
    );
}
#[test]
fn recurring_coverage_gaps_are_readable_without_masking_backend_warnings() {
    use crate::presentation::waveform_view::{
        MonitorWarning, monitor_warning, output_underrun_warning,
    };
    let rate = SampleRate::DEFAULT;
    let state = SessionState::new();
    let generator = LiveGenerator::with_state(Target::silence(rate), Arc::clone(&state));
    let mut desc = descriptor(rate.hz());
    desc.frames = None;
    let monitor = PlaybackMonitor::new(
        desc,
        MonitorDetail::Waveform,
        state,
        Some(Arc::clone(generator.mailbox())),
    );
    let mut callback = Transport::new(Source::Live(Box::new(generator)), 2, monitor.session())
        .with_monitor(Some(monitor.clone()));
    let mut reader = monitor.reader().unwrap();
    let mut output = [0.0; 1024];
    let mut block = 0;
    let fill = |callback: &mut Transport, block: &mut u64, output: &mut [f64]| {
        callback.fill(output, time(*block * 512 * 1_000_000_000 / 48000), &|v| v);
        *block += 1;
    };
    for _ in 0..4 {
        fill(&mut callback, &mut block, &mut output);
        reader.poll(40);
    }
    assert_eq!(monitor_warning(&reader, 40), None);
    for episode in 1..=10 {
        for _ in 0..12 {
            fill(&mut callback, &mut block, &mut output);
        }
        if episode == 5 {
            monitor.session().count_underrun();
        }
        reader.poll(40);
        let warning = monitor_warning(&reader, 40).unwrap();
        assert_eq!(warning, MonitorWarning::IncompleteCoverage);
        assert!(!warning.announces_changes());
        assert_eq!(reader.status().gaps, episode);
        if episode >= 5 {
            assert_eq!(
                output_underrun_warning(reader.status().underruns),
                "Backend-reported output underruns: 1 · see details."
            );
        }
        for _ in 0..20 {
            reader.poll(40);
            assert_eq!(monitor_warning(&reader, 40), Some(warning));
        }
        for _ in 0..4 {
            fill(&mut callback, &mut block, &mut output);
            reader.poll(40);
        }
        assert_eq!(monitor_warning(&reader, 40), None);
        assert_eq!(reader.status().gaps, episode);
    }
    reader.received = Some(Instant::now() - Duration::from_secs(1));
    assert_eq!(monitor_warning(&reader, 40), Some(MonitorWarning::Stale));
    assert!(monitor_warning(&reader, 40).unwrap().announces_changes());
}

#[test]
fn stationary_periodic_samples_have_advancing_real_publication_coverage() {
    for hz in [3750.0, 3751.0] {
        let signal = Sound {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: (vec![ComponentForm::Tone {
                frequency_hz: hz,
                ears: Ears::Both,
                peak_dbfs: -15.3,
                modulation: None,
            }])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab_core::Component {
                id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                form: voice,
            })
            .collect(),
        }
        .signal(SampleRate::DEFAULT)
        .unwrap();
        assert_eq!(SampleRate::DEFAULT.hz(), 48000);
        // Independent rational oracle: 3750*512/48000 = exactly 40 cycles.
        assert_eq!(3750_u64 * 512, 48000 * 40);
        let monitor = PlaybackMonitor::new(
            descriptor(48000),
            MonitorDetail::Waveform,
            SessionState::new(),
            None,
        );
        let mut reader = monitor.reader().unwrap();
        let mut writer = monitor.writer().unwrap();
        let mut samples = [[0.0; 2]; 512];
        let mut before = Vec::new();
        let mut old_sequence = 0;
        for start in [0, 512] {
            signal.render(start, &mut samples);
            {
                writer.append(start, &samples);
                writer.finish(start + 512, signal.observation(start + 511));
            }
            reader.poll(2);
            assert!(reader.status().sequence > old_sequence);
            old_sequence = reader.status().sequence;
            assert_eq!(reader.status().window_end, start + 512);
            if start == 0 {
                before = reader.samples().to_vec();
            } else if hz == 3750.0 {
                assert_eq!(reader.samples(), before);
            } else {
                assert_ne!(reader.samples(), before);
            }
            let capture = crate::presentation::waveform_view::WaveformCapture::monitored(
                &reader,
                crate::presentation::waveform_view::WaveformScale::Fixed,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                crate::presentation::waveform_view::output_monitor_state(
                    Some(&reader),
                    Some(&capture),
                    false,
                    false
                ),
                crate::presentation::waveform_view::OutputMonitorState::Live
            );
        }
    }
}

#[test]
fn maximum_rate_waveform_callback_does_not_allocate_or_free_at_32_slots() {
    let rate = SampleRate::new(768_000).unwrap();
    let voices: Vec<_> = (0..16)
        .map(|n| ComponentForm::Tone {
            frequency_hz: 200.0 + f64::from(n) * 20.0,
            ears: Ears::Both,
            peak_dbfs: -30.0,
            modulation: Some(crate::core::Modulation::Am {
                rate_hz: 23.0 + f64::from(n) * 0.75,
                depth: 1.0,
            }),
        })
        .collect();
    let tones = numbered(rate, &voices);
    let clicks = numbered(
        rate,
        &voices
            .iter()
            .map(|voice| {
                let base = *voice;
                ComponentForm::Clicks {
                    rate_hz: 40.0,
                    width_seconds: 0.001,
                    ears: base.route().unwrap(),
                    peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                    modulation: base.modulation(),
                }
            })
            .collect::<Vec<_>>(),
    );
    let state = SessionState::new();
    let generator = LiveGenerator::with_state(tones, Arc::clone(&state));
    let control = generator.control();
    let mut desc = descriptor(rate.hz());
    desc.frames = None;
    let monitor = PlaybackMonitor::new(
        desc,
        MonitorDetail::Waveform,
        state,
        Some(Arc::clone(generator.mailbox())),
    );
    let mut reader = monitor.reader().unwrap();
    let mut callback = Transport::new(Source::Live(Box::new(generator)), 2, monitor.session())
        .with_monitor(Some(monitor));
    let mut out = vec![0.0; 4096 * 2];
    // Finish the initial 30 ms fade before replacing all carrier kinds.
    for block in 0..6 {
        callback.fill(
            &mut out,
            time(block * 4096 * 1_000_000_000 / 768000),
            &|v| v,
        );
        reader.poll(100);
    }
    control.update(&clicks).unwrap();
    let measured = allocation_counter::measure(|| {
        callback.fill(&mut out, time(6 * 4096 * 1_000_000_000 / 768000), &|v| v);
        reader.poll(100);
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(
        measured.count_current, 0,
        "callback and preallocated reader must not free allocations"
    );
    assert_eq!(
        reader
            .status()
            .observation
            .unwrap()
            .voices
            .iter()
            .flatten()
            .count(),
        32
    );
    assert!(reader.status().observation.unwrap().transitioning);
    assert_eq!(reader.status().window_end, 7 * 4096);
    assert!(reader.has_current_window());
    assert!(
        reader
            .samples()
            .iter()
            .flatten()
            .any(|sample| *sample != 0.0)
    );
    // Overflow is also allocation/free-free when the consumer stops draining.
    let measured = allocation_counter::measure(|| {
        for block in 7..30 {
            callback.fill(
                &mut out,
                time(block * 4096 * 1_000_000_000 / 768000),
                &|v| v,
            );
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    reader.poll(100);
    assert!(reader.status().gaps > 0);
    assert_eq!(reader.status().observation.unwrap().frame, 30 * 4096 - 1);
    assert!(!reader.has_current_window());
}

#[test]
fn sample_prefix_waits_for_its_coherent_state_publication() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut writer = monitor.writer().unwrap();
    let mut reader = monitor.reader().unwrap();
    writer.append(0, &[[0.25, -0.125]; 512]);
    reader.poll(100);
    assert!(!reader.has_current_window());
    assert_eq!(reader.freshness(), ObservationFreshness::Waiting);
    assert!(
        crate::presentation::waveform_view::WaveformCapture::monitored(
            &reader,
            crate::presentation::waveform_view::WaveformScale::Fit
        )
        .unwrap()
        .is_none()
    );
    writer.finish(512, None);
    reader.poll(100);
    assert!(reader.has_current_window());
    assert_eq!(reader.freshness(), ObservationFreshness::Current);
    assert!(
        crate::presentation::waveform_view::WaveformCapture::monitored(
            &reader,
            crate::presentation::waveform_view::WaveformScale::Fit
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn concurrent_next_buffer_samples_do_not_invalidate_the_published_window() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut writer = monitor.writer().unwrap();
    let mut reader = monitor.reader().unwrap();
    writer.append(0, &[[0.25, -0.125]; 512]);
    writer.finish(512, None);
    reader.poll(100);
    assert!(reader.has_current_window());
    // Model a producer paused after writing its next sample chunk but before
    // publishing that buffer's generated state. The prior watermark stays usable.
    writer.append(512, &[[0.5, -0.25]; 256]);
    reader.poll(100);
    assert!(reader.has_current_window());
    assert_eq!(reader.status().window_end, 512);
    assert_eq!(reader.samples(), &[[0.25, -0.125]; 512]);
    writer.finish(768, None);
    reader.poll(100);
    assert!(reader.has_current_window());
    assert_eq!(reader.status().window_end, 768);
    assert_eq!(&reader.samples()[512..], &[[0.5, -0.25]; 256]);
}

#[test]
fn allocation_counter_detects_freeing_prepared_storage() {
    let prepared = std::hint::black_box(vec![0_u8; 4096]);
    let measured = allocation_counter::measure(|| drop(prepared));
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, -1);
}
