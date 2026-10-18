// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Playback of an `auditorylab-core` session on the default output device,
//! using its backend-reported default stream configuration.
//!
//! Application-owned sample processing uses preallocated storage and publishes
//! terminal outcomes with a single atomic compare-and-exchange. Native backend
//! dispatch and rich error handling can allocate or use backend locks; stream
//! destruction does not establish that every native callback has joined.

mod callback;
mod error;
mod observation;
mod output;
mod shared;

pub use error::PlaybackError;
pub use observation::{
    OBSERVATION_FRAMES, ObservationConsumer, ObservationPacket, ObservationProducer,
    observation_channel, transport_capacity_bytes,
};
pub use output::{Output, OutputDescription, Played, SampleRepresentation};
