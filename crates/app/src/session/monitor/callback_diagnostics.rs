//! Bounded diagnostic only. Nested raw clock windows are deliberately distinct;
//! no instrumented measurement is a callback qualification result.
use super::*;
use serde::Serialize;
use std::io::{BufWriter, Write};

#[derive(Clone, Copy, Serialize)]
struct ProcessCounters {
    minor_faults: i64,
    major_faults: i64,
    voluntary_switches: i64,
    involuntary_switches: i64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn process_counters() -> Option<ProcessCounters> {
    use nix::sys::resource::{UsageWho, getrusage};
    let usage = getrusage(UsageWho::RUSAGE_SELF).unwrap();
    Some(ProcessCounters {
        minor_faults: usage.minor_page_faults(),
        major_faults: usage.major_page_faults(),
        voluntary_switches: usage.voluntary_context_switches(),
        involuntary_switches: usage.involuntary_context_switches(),
    })
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn thread_cpu_ns() -> Option<i64> {
    use nix::{
        sys::time::TimeValLike,
        time::{ClockId, clock_gettime},
    };
    Some(
        clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID)
            .unwrap()
            .num_nanoseconds(),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn cpu_resolution_ns() -> Option<i64> {
    use nix::{
        sys::time::TimeValLike,
        time::{ClockId, clock_getres},
    };
    Some(
        clock_getres(ClockId::CLOCK_THREAD_CPUTIME_ID)
            .unwrap()
            .num_nanoseconds(),
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_counters() -> Option<ProcessCounters> {
    None
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn thread_cpu_ns() -> Option<i64> {
    None
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cpu_resolution_ns() -> Option<i64> {
    None
}

#[derive(Serialize)]
struct ClockWindow {
    counter_before_start_ns: u128,
    counter_before_end_ns: u128,
    outer_start_ns: u128,
    cpu_start_ns: Option<i64>,
    inner_start_ns: u128,
    inner_end_ns: u128,
    cpu_end_ns: Option<i64>,
    outer_end_ns: u128,
    counter_after_start_ns: u128,
    counter_after_end_ns: u128,
    process_before: Option<ProcessCounters>,
    process_after: Option<ProcessCounters>,
}

fn measure(anchor: Instant, action: impl FnOnce()) -> ClockWindow {
    // Ordering is part of the diagnostic contract. getrusage is outside both
    // wall and CPU windows; the CPU samples enclose the two inner wall reads.
    let counter_before_start = Instant::now();
    let process_before = process_counters();
    let counter_before_end = Instant::now();
    let outer_start = Instant::now();
    let cpu_start_ns = thread_cpu_ns();
    let inner_start = Instant::now();
    action();
    let inner_end = Instant::now();
    let cpu_end_ns = thread_cpu_ns();
    let outer_end = Instant::now();
    let counter_after_start = Instant::now();
    let process_after = process_counters();
    let counter_after_end = Instant::now();
    ClockWindow {
        counter_before_start_ns: counter_before_start.duration_since(anchor).as_nanos(),
        counter_before_end_ns: counter_before_end.duration_since(anchor).as_nanos(),
        outer_start_ns: outer_start.duration_since(anchor).as_nanos(),
        cpu_start_ns,
        inner_start_ns: inner_start.duration_since(anchor).as_nanos(),
        inner_end_ns: inner_end.duration_since(anchor).as_nanos(),
        cpu_end_ns,
        outer_end_ns: outer_end.duration_since(anchor).as_nanos(),
        counter_after_start_ns: counter_after_start.duration_since(anchor).as_nanos(),
        counter_after_end_ns: counter_after_end.duration_since(anchor).as_nanos(),
        process_before,
        process_after,
    }
}

#[derive(Serialize)]
struct Transition {
    frame: u64,
    active_revision: u64,
    settled_revision: u64,
    transitioning: bool,
    voices: u32,
    incoming: u32,
    outgoing: u32,
}

fn transition(observation: Option<auditorylab_core::Observation>) -> Option<Transition> {
    observation.map(|value| {
        let mut voices = 0;
        let mut incoming = 0;
        let mut outgoing = 0;
        for voice in value.voices.iter().flatten() {
            voices += 1;
            incoming += u32::from(voice.role == crate::core::SlotRole::Incoming);
            outgoing += u32::from(voice.role == crate::core::SlotRole::Outgoing);
        }
        Transition {
            frame: value.frame,
            active_revision: value.active_revision,
            settled_revision: value.settled_revision,
            transitioning: value.transitioning,
            voices,
            incoming,
            outgoing,
        }
    })
}

#[derive(Serialize)]
struct Interval {
    kind: &'static str,
    block: usize,
    monitor: &'static str,
    iteration: u64,
    warmup: bool,
    nominal_budget_ns: u64,
    publication_deliberately_held: bool,
    missed_before: Option<u64>,
    missed_after: Option<u64>,
    monitor_disabled: bool,
    previous_reader_poll_iteration: Option<u64>,
    previous_reader_poll_end_ns: Option<u128>,
    reader_poll_after: bool,
    transition_before: Option<Transition>,
    transition_after: Option<Transition>,
    clocks: ClockWindow,
}

fn calibration_interval(kind: &'static str, iteration: u64, clocks: ClockWindow) -> Interval {
    Interval {
        kind,
        block: 0,
        monitor: "none",
        iteration,
        warmup: iteration < 100,
        nominal_budget_ns: 0,
        publication_deliberately_held: false,
        missed_before: None,
        missed_after: None,
        monitor_disabled: false,
        previous_reader_poll_iteration: None,
        previous_reader_poll_end_ns: None,
        reader_poll_after: false,
        transition_before: None,
        transition_after: None,
        clocks,
    }
}

#[test]
#[ignore = "bounded diagnostic only; requires a new absolute AUDITORYLAB_CALLBACK_DIAGNOSTICS path; no device"]
fn callback_interval_diagnostics() {
    let path = std::path::PathBuf::from(
        std::env::var_os("AUDITORYLAB_CALLBACK_DIAGNOSTICS")
            .expect("set an absolute new diagnostic output path"),
    );
    assert!(
        path.is_absolute() && path.parent().is_some_and(std::path::Path::is_dir) && !path.exists()
    );
    let unix_before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let anchor = Instant::now();
    let unix_after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let reported_cpu_resolution_ns = cpu_resolution_ns();
    let expected = 3100 + 2 + 18 * 3100;
    let mut intervals = Vec::with_capacity(expected);
    let initial_capacity = intervals.capacity();
    for iteration in 0..3100 {
        intervals.push(calibration_interval(
            "empty",
            iteration,
            measure(anchor, || std::hint::black_box(())),
        ));
    }
    for iteration in 0..2 {
        intervals.push(calibration_interval(
            "sleep_5ms",
            iteration,
            measure(anchor, || std::thread::sleep(Duration::from_millis(5))),
        ));
    }
    let hz = 768000;
    let frames = 256;
    let frame_count = 256_u64;
    let budget = frame_count * 1_000_000_000 / u64::from(hz);
    let components: Vec<Voice> = (0..16)
        .map(|n| Voice {
            ears: Ears::Both,
            peak_dbfs: -30.0,
            carrier: Carrier::Tone {
                frequency_hz: 200.0 + f64::from(n) * 20.0,
            },
            modulation: Some(crate::core::Modulation::Sinusoidal {
                rate_hz: 39.0625,
                depth: 1.0,
            }),
        })
        .collect();
    let rate = SampleRate::new(hz).unwrap();
    let tones = numbered(rate, &components);
    let pulses = numbered(
        rate,
        &components
            .iter()
            .map(|c| Voice {
                carrier: Carrier::Click {
                    rate_hz: 40.0,
                    width_seconds: 0.001,
                },
                ..*c
            })
            .collect::<Vec<_>>(),
    );
    assert_eq!(tones.len(), 16);
    assert_eq!(pulses.len(), 16);
    let order = [
        None,
        Some(MonitorDetail::Quantities),
        Some(MonitorDetail::Waveform),
        Some(MonitorDetail::Waveform),
        Some(MonitorDetail::Quantities),
        None,
    ];
    for (block, detail) in order.into_iter().cycle().take(18).enumerate() {
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
        let mut next_poll = 0;
        let mut previous_reader_poll_iteration = None;
        let mut previous_reader_poll_end_ns = None;
        for iteration in 0..3100_u64 {
            control
                .update(if iteration % 2 == 0 { &pulses } else { &tones })
                .unwrap();
            let held = iteration % 25 == 0;
            let guard = if held {
                monitor.as_ref().map(|m| m.0.history.lock().unwrap())
            } else {
                None
            };
            let transition_before = transition(callback.diagnostic_observation());
            let missed_before = monitor.as_ref().map(|m| m.0.missed.load(Ordering::Relaxed));
            let clocks = measure(anchor, || {
                callback.fill(
                    &mut output,
                    time(iteration * frame_count * 1_000_000_000 / u64::from(hz)),
                    &|v| v,
                )
            });
            let missed_after = monitor.as_ref().map(|m| m.0.missed.load(Ordering::Relaxed));
            let transition_after = transition(callback.diagnostic_observation());
            let monitor_disabled = monitor
                .as_ref()
                .is_some_and(|m| m.0.disabled.load(Ordering::Relaxed));
            drop(guard);
            let poll_due = iteration * frame_count >= next_poll;
            intervals.push(Interval {
                kind: "callback",
                block,
                monitor: match detail {
                    None => "none",
                    Some(MonitorDetail::Quantities) => "quantities",
                    Some(MonitorDetail::Waveform) => "waveform",
                },
                iteration,
                warmup: iteration < 100,
                nominal_budget_ns: budget,
                publication_deliberately_held: held && monitor.is_some(),
                missed_before,
                missed_after,
                monitor_disabled,
                previous_reader_poll_iteration,
                previous_reader_poll_end_ns,
                reader_poll_after: poll_due && reader.is_some(),
                transition_before,
                transition_after,
                clocks,
            });
            if poll_due {
                if let Some(reader) = reader.as_mut() {
                    reader.poll(100);
                    previous_reader_poll_iteration = Some(iteration);
                    previous_reader_poll_end_ns = Some(anchor.elapsed().as_nanos());
                }
                next_poll = iteration * frame_count + u64::from(hz) / 20;
            }
        }
    }
    // Persist all raw data, even if the subsequent integrity checks fail.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let mut writer = BufWriter::new(file);
    serde_json::to_writer(&mut writer, &serde_json::json!({
        "diagnostic_schema_version": 2, "kind": "callback_interval_header",
        "pid": std::process::id(), "rust_thread_id": format!("{:?}", std::thread::current().id()),
        "thread_name": std::thread::current().name(), "native_thread_id": null,
        "anchor_before_unix_ns": unix_before, "anchor_after_unix_ns": unix_after,
        "reported_cpu_clock_resolution_ns": reported_cpu_resolution_ns,
        "rate_hz": hz, "buffer_frames": frames, "blocks": 18,
        "warmup_per_block": 100, "measured_per_block": 3000,
        "empty_control_count": 3100, "sleep_control_count": 2, "interval_count": intervals.len(),
        "qualification_result": null,
        "ordering": "process_before; outer_start; cpu_start; inner_start; action; inner_end; cpu_end; outer_end; process_after",
        "limits": "CPU encloses inner wall reads; process counters are not per-thread. Raw signed differences retained. Resolution is not an accuracy guarantee. Metadata reads/pushes outside all windows can still perturb execution. No clock or median subtraction qualifies a callback."
    })).unwrap();
    writer.write_all(b"\n").unwrap();
    for interval in &intervals {
        serde_json::to_writer(&mut writer, interval).unwrap();
        writer.write_all(b"\n").unwrap();
    }
    writer.flush().unwrap();
    assert_eq!(intervals.len(), expected);
    assert_eq!(
        intervals.capacity(),
        initial_capacity,
        "no record Vec growth during capture"
    );
    for interval in &intervals {
        let c = &interval.clocks;
        assert!(
            c.counter_before_start_ns <= c.counter_before_end_ns
                && c.counter_before_end_ns <= c.outer_start_ns
                && c.outer_start_ns <= c.inner_start_ns
                && c.inner_start_ns <= c.inner_end_ns
                && c.inner_end_ns <= c.outer_end_ns
                && c.outer_end_ns <= c.counter_after_start_ns
                && c.counter_after_start_ns <= c.counter_after_end_ns
        );
        assert!(
            c.cpu_start_ns
                .zip(c.cpu_end_ns)
                .is_none_or(|(start, end)| start <= end)
        );
    }
}
