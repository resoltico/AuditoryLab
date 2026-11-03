//! The interactive `auditorylab device-check` sequence.
use super::{CliError, Ended, eprose, on_interrupt, say, usage};
use auditorylab_app::device_check::{DeviceAnswer, DeviceCheck, DeviceTest};
use auditorylab_app::services::playback::{
    EndReason, PlaybackControl, Preparation, Session, prepare_snapshot,
};
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex, PoisonError};

/// Typing this instead of an answer plays the tone again.
const REPLAY: &str = "replay";

pub(super) fn device_check() -> Result<Ended, CliError> {
    // One handler for the whole sequence: the first Ctrl-C stops the tone
    // that is playing, or ends the check while it waits for an answer.
    let playing: Arc<Mutex<Option<PlaybackControl>>> = Arc::default();
    let current = Arc::clone(&playing);
    on_interrupt(move || {
        match current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            Some(control) => {
                control.request_stop();
            }
            None => std::process::exit(130),
        }
    })?;
    say(auditorylab_presentation::device_check::INTRODUCTION)?;
    let mut check = DeviceCheck::default();
    let mut input = io::stdin().lock();
    for test in DeviceTest::ALL {
        say(&format!(
            "\n{}: {}",
            test.label(),
            auditorylab_presentation::device_check::test_prompt(test)
        ))?;
        let mut play = true;
        loop {
            if std::mem::take(&mut play) && play_tone(test, &playing)? == EndReason::Stopped {
                return Ok(Ended::Stopped);
            }
            let keys: Vec<_> = test.answers().iter().map(|answer| answer.key()).collect();
            eprint!(
                "{}",
                eprose(&format!("Answer {} or {REPLAY}: ", keys.join(", ")))
            );
            io::stderr().flush()?;
            let mut line = String::new();
            if input.read_line(&mut line)? == 0 {
                return Err(usage(
                    "device-check needs an answer for every tone on standard input",
                ));
            }
            let line = line.trim();
            if line == REPLAY {
                play = true;
                continue;
            }
            match line.parse::<DeviceAnswer>() {
                Ok(answer) if check.record(test, answer).is_ok() => break,
                _ => say(&format!("Choose one of: {}, {REPLAY}.", keys.join(", ")))?,
            }
        }
    }
    say(&format!(
        "\n{}",
        auditorylab_presentation::device_check::assessment(check.assessment())
    ))?;
    Ok(Ended::Normally)
}

fn play_tone(
    test: DeviceTest,
    playing: &Mutex<Option<PlaybackControl>>,
) -> Result<EndReason, CliError> {
    let snapshot = test.snapshot()?;
    let session = Session::new();
    *playing.lock().unwrap_or_else(PoisonError::into_inner) = Some(session.control());
    let result = match prepare_snapshot(session, &snapshot) {
        Ok(Preparation::Ready(playback)) => playback.play().map(|report| report.end),
        Ok(Preparation::Stopped(_)) => Ok(EndReason::Stopped),
        Err(error) => Err(error),
    };
    *playing.lock().unwrap_or_else(PoisonError::into_inner) = None;
    Ok(result?)
}
