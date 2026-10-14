// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! No-component silence on the existing default output; no configuration changes.
#![cfg(test)]
use auditory_core::{Session, Stimulus};
use auditory_device::{Output, PlaybackError};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
fn verified(output: &Output, seconds: f64) -> Session {
    let stimulus = Stimulus {
        duration_seconds: seconds,
        fade_seconds: 0.0,
        components: vec![],
    };
    let session = output.session(&stimulus).unwrap();
    assert_eq!(session.peak_dbfs(), [f64::NEG_INFINITY; 2]);
    let mut frames = [[1.0; 2]; 512];
    session.into_renderer().render(&mut frames);
    assert_eq!(frames, [[0.0; 2]; 512]);
    let session = output.session(&stimulus).unwrap();
    assert_eq!(session.peak_dbfs(), [f64::NEG_INFINITY; 2]);
    println!(
        "backend {:?}; description {:?}; no components; verified stereo silence",
        cpal::default_host().id(),
        output.description()
    );
    session
}
#[test]
#[ignore = "opens default output for one second of verified no-component silence"]
fn silence_completes() {
    let output = Output::default_device().unwrap();
    let rate = output.description().sample_rate;
    let session = verified(&output, 1.0);
    let frames = session.frames();
    let start = Instant::now();
    let played = output.play(session, &AtomicBool::new(false)).unwrap();
    let elapsed = start.elapsed();
    println!(
        "played {} frames as {} in {elapsed:?}; {} underruns",
        played.frames, played.representation, played.underruns
    );
    assert_eq!(played.frames, frames);
    assert_eq!(played.sample_rate, rate);
    assert!(elapsed >= Duration::from_secs(1));
    assert!(elapsed < Duration::from_secs(10));
}
#[test]
#[ignore = "opens default output for verified no-component silence until cancellation"]
fn silence_cancels() {
    let output = Output::default_device().unwrap();
    let session = verified(&output, 600.0);
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            cancel.store(true, Ordering::Relaxed);
        });
        output.play(session, &cancel)
    });
    println!("cancelled after {:?}: {result:?}", start.elapsed());
    assert!(matches!(result, Err(PlaybackError::Cancelled)));
    assert!(start.elapsed() < Duration::from_secs(10));
}
