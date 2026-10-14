// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Command-line discovery, inspection, WAV export and playback of auditory presets.
//!
//! The CLI owns argument/input grammar, reporting and interrupts. Libraries
//! own preset/stimulus rules and limits; their results become text and exit statuses.

mod args;
mod failure;
mod report;
mod run;

use std::process::ExitCode;
use std::{env, io};

fn main() -> ExitCode {
    let outcome = args::parse(env::args_os().skip(1)).and_then(run::run);
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            let mut stderr = io::stderr().lock();
            let _ = run::output(&mut stderr, &failure.diagnostic());
            if let failure::Failure::Usage { topic, .. } = &failure {
                let _ = run::output(&mut stderr, &format!("\n{}\n", args::help(*topic)));
            }
            ExitCode::from(failure.exit_code())
        }
    }
}
