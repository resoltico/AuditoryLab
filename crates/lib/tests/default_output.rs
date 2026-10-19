//! Device integration using only component-free silence.
#![cfg(test)]
use auditorylab::{Cancel, ErrorKind, Progress, load_reader, prepare_playback};
use std::time::{Duration, Instant};
fn silent(seconds: u32) -> auditorylab::Playback {
    let recipe = format!("schema_version=2\nduration_seconds={seconds}\nfade_seconds=0\n");
    let preset = load_reader(recipe.as_bytes(), "silence").unwrap();
    assert!(preset.stimulus().components.is_empty());
    let playback = prepare_playback(&preset).unwrap();
    assert_eq!(
        playback.sound().signal().peak_dbfs(),
        [f64::NEG_INFINITY; 2]
    );
    let mut frames = [[1.0; 2]; 512];
    playback.sound().signal().render(0, &mut frames);
    assert_eq!(frames, [[0.0; 2]; 512]);
    println!("Verified no-component silence: {:?}", playback.output());
    playback
}
#[test]
#[ignore = "plays one second of component-free silence on the default output"]
fn silence_completes() {
    let playback = silent(1);
    let frames = playback.sound().signal().frames();
    let progress = Progress::default();
    let now = Instant::now();
    let played = playback.play(&Cancel::default(), &progress).unwrap();
    assert_eq!(played.frames, frames);
    assert!(now.elapsed() >= Duration::from_secs(1));
    assert!(now.elapsed() < Duration::from_secs(12));
    assert!(progress.frames() > 0);
}
#[test]
#[ignore = "plays component-free silence until cancellation"]
fn silence_cancels() {
    let playback = silent(600);
    let cancel = Cancel::default();
    let now = Instant::now();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            cancel.request();
        });
        playback.play(&cancel, &Progress::default())
    });
    assert_eq!(result.unwrap_err().kind(), ErrorKind::Cancelled);
    assert!(now.elapsed() < Duration::from_secs(12));
}

#[test]
#[ignore = "plays one second of an empty live draft on the default output"]
fn live_silence_stops() {
    let draft = auditorylab::Draft::new();
    assert!(draft.components().is_empty());
    let live = auditorylab::prepare_live(&draft).unwrap();
    println!("Verified empty live draft: {:?}", live.output());
    let rate = live.output().sample_rate.hz();
    let cancel = Cancel::default();
    let now = Instant::now();
    let progress = Progress::default();
    let (played, reached) = std::thread::scope(|scope| {
        let waiter = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(8);
            while progress.frames() < u64::from(rate) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let reached = progress.frames() >= u64::from(rate);
            cancel.request();
            reached
        });
        let played = live.play(&cancel, &progress).unwrap();
        (played, waiter.join().unwrap())
    });
    println!(
        "Live rendered {} frames in {:?}",
        played.frames,
        now.elapsed()
    );
    assert!(reached, "one second of live progress was not observed");
    assert!(now.elapsed() >= Duration::from_secs(1) && now.elapsed() < Duration::from_secs(12));
    assert!(played.frames >= u64::from(rate));
}
