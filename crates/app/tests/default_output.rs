//! Device integration using only component-free silence.
#![cfg(test)]
mod common;
use auditorylab_app::audio::device::{prepare_live, prepare_playback};
use auditorylab_app::audio::session::{EndReason, Preparation, Session, StopRequest};
use auditorylab_app::document::editing::Draft;
use auditorylab_app::storage::load::load_reader;
use std::time::{Duration, Instant};

fn silent(
    seconds: u32,
) -> (
    auditorylab_app::audio::device::PreparedPlayback,
    auditorylab_app::audio::session::PlaybackControl,
) {
    let recipe = common::silent(f64::from(seconds), 0.0).to_toml().unwrap();
    let snapshot = load_reader(recipe.as_bytes(), "silence").unwrap();
    assert!(snapshot.sound().components.is_empty());
    let session = Session::new();
    let control = session.control();
    let Preparation::Ready(playback) = prepare_playback(session, &snapshot).unwrap() else {
        panic!("no Stop was requested");
    };
    assert_eq!(playback.signal().peak_dbfs(), [f64::NEG_INFINITY; 2]);
    let mut frames = [[1.0; 2]; 512];
    playback.signal().render(0, &mut frames);
    assert_eq!(frames, [[0.0; 2]; 512]);
    println!("Verified no-component silence: {:?}", playback.output());
    (playback, control)
}

#[test]
#[ignore = "plays one second of component-free silence on the default output"]
fn silence_completes() {
    let (playback, control) = silent(1);
    let frames = playback.signal().frames();
    let now = Instant::now();
    let report = playback.play().unwrap();
    assert_eq!(report.end, EndReason::Completed);
    assert_eq!(report.generated_frames, frames);
    assert_eq!(report.estimated_output_frames, frames);
    assert!(report.drained);
    assert!(now.elapsed() >= Duration::from_secs(1));
    assert!(now.elapsed() < Duration::from_secs(12));
    assert_eq!(control.request_stop(), StopRequest::AlreadyFinished);
}

#[test]
#[ignore = "plays component-free silence until Stop"]
fn silence_stops() {
    let (playback, control) = silent(600);
    let now = Instant::now();
    let report = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            assert_eq!(control.request_stop(), StopRequest::Accepted);
        });
        playback.play()
    })
    .unwrap();
    assert_eq!(report.end, EndReason::Stopped);
    assert!(now.elapsed() < Duration::from_secs(12));
}

#[test]
#[ignore = "plays one second of an empty live draft on the default output"]
fn live_silence_stops() {
    let draft = Draft::new().unwrap();
    assert!(draft.components().is_empty());
    let session = Session::new();
    let control = session.control();
    let Preparation::Ready(live) = prepare_live(session, &draft).unwrap() else {
        panic!("no Stop was requested");
    };
    println!("Verified empty live draft: {:?}", live.output());
    let rate = live.sample_rate().hz();
    let now = Instant::now();
    let (report, reached) = std::thread::scope(|scope| {
        let waiter = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(8);
            while control.estimated_output_frames() < u64::from(rate) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let reached = control.estimated_output_frames() >= u64::from(rate);
            control.request_stop();
            reached
        });
        let report = live.play().unwrap();
        (report, waiter.join().unwrap())
    });
    println!(
        "Live generated {} frames in {:?}",
        report.generated_frames,
        now.elapsed()
    );
    assert!(reached, "one second of estimated output was not observed");
    assert_eq!(report.end, EndReason::Stopped);
    assert!(report.generated_frames >= u64::from(rate));
}

#[test]
#[ignore = "opens the default output without playing"]
fn stop_before_preparation_returns_an_early_report() {
    let snapshot = load_reader(
        common::silent(1.0, 0.0).to_toml().unwrap().as_bytes(),
        "silence",
    )
    .unwrap();
    let session = Session::new();
    session.control().request_stop();
    let Preparation::Stopped(report) = prepare_playback(session, &snapshot).unwrap() else {
        panic!("Stop before discovery must not create a stream");
    };
    assert_eq!(report.generated_frames, 0);
    assert!(report.drained);
}
