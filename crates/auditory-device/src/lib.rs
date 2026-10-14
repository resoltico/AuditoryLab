// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Playback of an `auditory-core` session on the default output device,
//! using its backend-reported default stream configuration.

mod callback;
mod error;
mod output;
mod shared;

pub use error::PlaybackError;
pub use output::{Output, OutputDescription, Played, SampleRepresentation};
