// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::session::live::LiveGenerator;
use crate::session::source::{FiniteSource, Source};
use crate::session::transport::{BufferTime, Transport};
use crate::session::{Outcome, SampleFormat};
use crate::{Carrier, Component, Ears, Origin, Stimulus};
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
fn numbered(rate: SampleRate, components: &[Component]) -> Target {
    let pairs: Vec<_> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (auditorylab_core::ComponentId::numbered(i).unwrap(), *c))
        .collect();
    Target::new(rate, &pairs).unwrap()
}
fn finite(signal: crate::Signal) -> Source {
    Source::Finite(FiniteSource::new(Arc::new(signal)))
}
fn signal(hz: u32) -> crate::Signal {
    Stimulus {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: vec![Component {
            ears: Ears::Left,
            peak_dbfs: -20.0,
            carrier: Carrier::Tone {
                frequency_hz: 100.0,
            },
            modulation: None,
        }],
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
fn real_callback_samples_match_history_with_finite_stop_and_contention() {
    let monitor = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut reader = monitor.reader();
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
    let frozen = crate::WaveformCapture::monitored(&reader, crate::WaveformScale::Fit)
        .unwrap()
        .unwrap();
    let caption = frozen.caption(true, false);
    let captured_meters = frozen.meters();
    let captured_id = frozen.session().unwrap().id;
    assert_eq!(frozen.span(), (0, 512));
    assert_eq!(frozen.observation().unwrap().frame, 511);
    let guard = monitor.0.history.lock().unwrap();
    a.fill(&mut x, time(64_000_000), &|v| v);
    b.fill(&mut y, time(64_000_000), &|v| v);
    assert_eq!(x, y);
    assert!(!reader.poll(100));
    assert_eq!(reader.status().generated_frames, 1024);
    drop(guard);
    a.fill(&mut x, time(128_000_000), &|v| v);
    b.fill(&mut y, time(128_000_000), &|v| v);
    assert_eq!(x, y);
    reader.poll(100);
    assert_eq!(reader.status().gaps, 1);
    assert_eq!(reader.status().missed_publications, 1);
    assert_eq!(reader.status().underruns, 0);
    assert_eq!(reader.status().window_start, 1024);
    assert_eq!(reader.samples().len(), 512);
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
    let latest = crate::WaveformCapture::monitored(&reader, crate::WaveformScale::Fixed)
        .unwrap()
        .unwrap();
    assert_ne!(latest.span(), frozen.span());
    assert!(latest.sequence() > frozen.sequence());
    assert_eq!(frozen.caption(true, false), caption);
    assert_eq!(frozen.meters(), captured_meters);
    assert_eq!(frozen.rate().hz(), 8000);
    assert_eq!(frozen.session().unwrap().id, captured_id);
    assert_eq!(frozen.scale(), crate::WaveformScale::Fit);
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
        assert_eq!(m.0.history.lock().unwrap().samples.len(), cap);
        let data = vec![[0.5, -0.25]; cap + 17];
        {
            let mut p = m.try_publish().unwrap();
            p.append(0, &data);
            p.finish(data.len() as u64, None);
        }
        let mut r = m.reader();
        assert!(r.poll(100));
        assert_eq!(r.samples().len(), cap);
        assert_eq!(r.status().window_start, 17);
        let received = r.received;
        r.poll(100);
        assert_eq!(r.received, received);
        {
            let mut p = m.try_publish().unwrap();
            p.append(data.len() as u64 + 100, &[[0.0; 2]]);
            p.finish(data.len() as u64 + 101, None);
        }
        r.poll(100);
        assert_eq!(r.samples(), [[0.0; 2]]);
        assert_eq!(r.status().gaps, 1);
    }
}
#[test]
fn poisoned_monitor_never_fails_audio_and_terminal_survives_missing_snapshot() {
    let m = PlaybackMonitor::new(
        descriptor(8000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let c = m.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = c.0.history.lock().unwrap();
            panic!("injected monitor poisoning")
        })
        .join()
        .is_err()
    );
    let mut callback =
        Transport::new(finite(signal(8000)), 2, m.session()).with_monitor(Some(m.clone()));
    let mut out = [0.0; 1024];
    callback.fill(&mut out, time(0), &|v| v);
    assert!(out.iter().any(|v| *v != 0.0));
    let mut reader = m.reader();
    assert!(!reader.poll(40));
    assert!(reader.status().disabled);
    m.0.session.finish(Outcome::Backend);
    reader.poll(40);
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
    let mut reader = m.reader();
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
    assert!(m.0.history.lock().unwrap().samples.is_empty());
    assert!(m.reader().samples.is_empty());
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
    let tones: Vec<Component> = (0..16)
        .map(|n| Component {
            ears: Ears::Both,
            peak_dbfs: -30.0,
            carrier: Carrier::Tone {
                frequency_hz: 200.0 + f64::from(n) * 20.0,
            },
            modulation: None,
        })
        .collect();
    let clicks: Vec<Component> = tones
        .iter()
        .map(|c| Component {
            carrier: Carrier::Click {
                rate_hz: 40.0,
                width_seconds: 0.001,
            },
            ..*c
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
    let mut reader = monitor.reader();
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
        let guard = (block == 6).then(|| monitor.0.history.lock().unwrap());
        a.fill(&mut x, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
        b.fill(&mut y, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
        assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
        drop(guard);
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
    assert_eq!(reader.status().missed_publications, 1);
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
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let other = monitor.clone();
    let holder = std::thread::spawn(move || {
        let _guard = other.0.history.lock().unwrap();
        ready_tx.send(()).unwrap();
        let _ = release_rx.recv_timeout(Duration::from_secs(3));
    });
    ready_rx.recv().unwrap();
    let mut callback = Transport::new(finite(signal(8000)), 2, monitor.session())
        .with_monitor(Some(monitor.clone()));
    let mut output = [0.0; 1024];
    for n in 0..10 {
        callback.fill(&mut output, time(n * 64_000_000), &|v| v);
    }
    assert_eq!(monitor.0.missed.load(Ordering::Relaxed), 10);
    let _ = release_tx.send(());
    holder.join().unwrap();
    let mut reader = monitor.reader();
    reader.poll(40);
    assert_eq!(reader.status().generated_frames, 5120);
    assert_eq!(reader.status().phase, PlaybackPhase::Running);
    callback.fill(&mut output, time(640_000_000), &|v| v);
    reader.poll(40);
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
    let mut reader = monitor.reader();
    assert_eq!(reader.freshness(), ObservationFreshness::Waiting);
    {
        let mut p = monitor.try_publish().unwrap();
        p.finish(1, None);
    }
    reader.poll(0);
    assert_eq!(reader.freshness(), ObservationFreshness::Current);
    reader.received = Some(Instant::now() - Duration::from_secs(1));
    reader.poll(0);
    assert_eq!(reader.freshness(), ObservationFreshness::Stale);
}
#[test]
#[ignore = "synthetic callback timing qualification; no audio device"]
fn callback_monitor_costs() {
    for hz in [48000, 768000] {
        for frames in [64_usize, 256, 1024, 4096] {
            for detail in [
                None,
                Some(MonitorDetail::Quantities),
                Some(MonitorDetail::Waveform),
            ] {
                let rate = SampleRate::new(hz).unwrap();
                let components: Vec<Component> = (0..16)
                    .map(|n| Component {
                        ears: Ears::Both,
                        peak_dbfs: -30.0,
                        carrier: Carrier::Tone {
                            frequency_hz: 200.0 + f64::from(n) * 20.0,
                        },
                        modulation: Some(crate::Modulation::Sinusoidal {
                            rate_hz: 39.0625,
                            depth: 1.0,
                        }),
                    })
                    .collect();
                let tones = numbered(rate, &components);
                let pulses = numbered(
                    rate,
                    &components
                        .iter()
                        .map(|c| Component {
                            carrier: Carrier::Click {
                                rate_hz: 40.0,
                                width_seconds: 0.001,
                            },
                            ..*c
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
                let mut reader = monitor.as_ref().map(PlaybackMonitor::reader);
                let mut callback =
                    Transport::new(Source::Live(Box::new(generator)), 2, Arc::clone(&shared))
                        .with_monitor(monitor.clone());
                let mut output = vec![0.0; frames * 2];
                let mut timings = Vec::with_capacity(300);
                let mut next_poll = 0;
                for n in 0..300_u64 {
                    if n > 0 && n % 60 == 0 {
                        control
                            .update(if (n / 60) % 2 == 0 { &tones } else { &pulses })
                            .unwrap();
                    }
                    let guard = if n % 25 == 0 {
                        monitor.as_ref().map(|m| m.0.history.lock().unwrap())
                    } else {
                        None
                    };
                    let start = Instant::now();
                    callback.fill(
                        &mut output,
                        time(n * frames as u64 * 1_000_000_000 / u64::from(hz)),
                        &|v| v,
                    );
                    timings.push(start.elapsed().as_nanos());
                    drop(guard);
                    if n * frames as u64 >= next_poll {
                        if let Some(reader) = reader.as_mut() {
                            reader.poll(100);
                        }
                        next_poll = n * frames as u64 + u64::from(hz) / 20;
                    }
                }
                timings.sort_unstable();
                println!(
                    "rate={hz} frames={frames} monitor={detail:?} median_ns={} p95_ns={} max_ns={} budget_ns={} retained_bytes={} consumer_bytes={} misses={} backend_underruns={}",
                    timings[150],
                    timings[285],
                    timings[299],
                    frames as u64 * 1_000_000_000 / u64::from(hz),
                    monitor
                        .as_ref()
                        .map_or(0, |m| m.0.history.lock().unwrap().samples.len() * 16),
                    reader.as_ref().map_or(0, |r| r.samples.len() * 16),
                    monitor
                        .as_ref()
                        .map_or(0, |m| m.0.missed.load(Ordering::Relaxed)),
                    shared.underruns()
                );
            }
        }
    }
}

#[test]
fn current_impairment_recovers_without_erasing_historical_gaps_or_held_identity() {
    use crate::{
        OutputMonitorState as State, WaveformCapture, WaveformScale, monitor_warning,
        output_monitor_state,
    };
    let monitor = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut reader = monitor.reader();
    reader.poll(40);
    assert_eq!(
        output_monitor_state(Some(&reader), None, false, true),
        State::Waiting
    );
    {
        let mut writer = monitor.try_publish().unwrap();
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
        Some(crate::MonitorWarning::IncompleteCoverage)
    );
    // A missed span is history; enough subsequent contiguous data repairs current coverage.
    {
        let mut writer = monitor.try_publish().unwrap();
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
        Some(crate::MonitorWarning::Stale)
    );
    let prior = held.caption(true, false);
    let another = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut other = another.reader();
    other.poll(40);
    assert_ne!(other.descriptor().id, held.session().unwrap().id);
    assert_eq!(
        output_monitor_state(Some(&other), Some(&held), true, true),
        State::Held
    );
    assert_eq!(held.caption(true, false), prior);
    {
        let mut writer = monitor.try_publish().unwrap();
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
    monitor.0.disabled.store(true, Ordering::Relaxed);
    reader.poll(40);
    assert_eq!(
        monitor_warning(&reader, 40),
        Some(crate::MonitorWarning::Unavailable)
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
    let unavailable = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    let mut absent = unavailable.reader();
    unavailable.0.disabled.store(true, Ordering::Relaxed);
    absent.poll(40);
    assert_eq!(
        output_monitor_state(Some(&absent), None, false, true),
        State::Unavailable
    );
    let underrun = PlaybackMonitor::new(
        descriptor(48000),
        MonitorDetail::Waveform,
        SessionState::new(),
        None,
    );
    underrun.0.session.count_underrun();
    let mut reader = underrun.reader();
    reader.poll(40);
    assert_eq!(monitor_warning(&reader, 40), None);
    assert!(crate::output_underrun_warning(reader.status().underruns).contains("underruns: 1"));
}
#[test]
fn recurring_coverage_gaps_are_readable_without_masking_backend_warnings() {
    use crate::{MonitorWarning, monitor_warning, output_underrun_warning};
    let rate = SampleRate::new(48000).unwrap();
    let state = SessionState::new();
    let generator = LiveGenerator::with_state(Target::silence(rate), Arc::clone(&state));
    let mut desc = descriptor(48000);
    desc.frames = None;
    let monitor = PlaybackMonitor::new(
        desc,
        MonitorDetail::Waveform,
        state,
        Some(Arc::clone(generator.mailbox())),
    );
    let mut callback = Transport::new(Source::Live(Box::new(generator)), 2, monitor.session())
        .with_monitor(Some(monitor.clone()));
    let mut reader = monitor.reader();
    let mut output = [0.0; 1024];
    let mut block = 0;
    let fill = |callback: &mut Transport, block: &mut u64, output: &mut [f64]| {
        callback.fill(output, time(*block * 512 * 1_000_000_000 / 48000), &|v| v);
        *block += 1;
    };
    for _ in 0..4 {
        fill(&mut callback, &mut block, &mut output);
    }
    reader.poll(40);
    assert_eq!(monitor_warning(&reader, 40), None);
    assert!(output_underrun_warning(reader.status().underruns).is_empty());
    for episode in 1..=10 {
        let guard = monitor.0.history.lock().unwrap();
        fill(&mut callback, &mut block, &mut output);
        drop(guard);
        if episode == 5 {
            monitor.session().count_underrun();
        }
        fill(&mut callback, &mut block, &mut output);
        reader.poll(40);
        let warning = monitor_warning(&reader, 40).unwrap();
        assert_eq!(warning, MonitorWarning::IncompleteCoverage);
        assert_eq!(
            warning.message(),
            "Incomplete current waveform coverage; waiting for a contiguous window."
        );
        assert!(!warning.announces_changes());
        assert_eq!(reader.status().gaps, episode);
        assert_eq!(reader.status().missed_publications, episode);
        // A reported output problem stays independently readable throughout partial coverage.
        if episode >= 5 {
            assert_eq!(
                output_underrun_warning(reader.status().underruns),
                "Backend-reported output underruns: 1 · see details."
            );
        }
        for _ in 0..20 {
            reader.poll(40);
            assert_eq!(monitor_warning(&reader, 40), Some(warning));
            assert_eq!(reader.status().gaps, episode);
        }
        for _ in 0..3 {
            fill(&mut callback, &mut block, &mut output);
        }
        reader.poll(40);
        assert_eq!(monitor_warning(&reader, 40), None);
        assert_eq!(
            reader.status().gaps,
            episode,
            "recovery does not erase history"
        );
        if episode >= 5 {
            assert_eq!(
                output_underrun_warning(reader.status().underruns),
                "Backend-reported output underruns: 1 · see details."
            );
        }
    }
    reader.received = Some(Instant::now() - Duration::from_secs(1));
    assert_eq!(monitor_warning(&reader, 40), Some(MonitorWarning::Stale));
    assert!(monitor_warning(&reader, 40).unwrap().announces_changes());
    assert_eq!(
        monitor_warning(&reader, 40).unwrap().message(),
        "Monitoring is stale; retained samples are not current output."
    );
    monitor.0.disabled.store(true, Ordering::Relaxed);
    reader.poll(40);
    assert_eq!(
        monitor_warning(&reader, 40),
        Some(MonitorWarning::Unavailable)
    );
    assert!(monitor_warning(&reader, 40).unwrap().announces_changes());
    assert_eq!(
        monitor_warning(&reader, 40).unwrap().message(),
        "Monitoring unavailable; audio transport is independent."
    );
}

#[test]
fn stationary_periodic_samples_have_advancing_real_publication_coverage() {
    for hz in [3750.0, 3751.0] {
        let signal = Stimulus {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: vec![Component {
                ears: Ears::Both,
                peak_dbfs: -15.3,
                carrier: Carrier::Tone { frequency_hz: hz },
                modulation: None,
            }],
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
        let mut reader = monitor.reader();
        let mut samples = [[0.0; 2]; 512];
        let mut before = Vec::new();
        let mut old_sequence = 0;
        for start in [0, 512] {
            signal.render(start, &mut samples);
            {
                let mut writer = monitor.try_publish().unwrap();
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
            let capture = crate::WaveformCapture::monitored(&reader, crate::WaveformScale::Fixed)
                .unwrap()
                .unwrap();
            assert_eq!(
                crate::output_monitor_state(Some(&reader), Some(&capture), false, false),
                crate::OutputMonitorState::Live
            );
        }
    }
}
