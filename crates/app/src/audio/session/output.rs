// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Plain descriptions of an output stream's configuration.
use auditorylab_core::SampleRate;
use std::fmt::{Display, Formatter};

/// How samples are represented in the device's stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 64-bit floating point, which holds every sample exactly.
    F64,
    /// 32-bit floating point.
    F32,
    /// 32-bit signed integers.
    I32,
    /// 24-bit signed integers.
    I24,
    /// 16-bit signed integers.
    I16,
}

impl Display for SampleFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::F64 => "64-bit float",
            Self::F32 => "32-bit float",
            Self::I32 => "32-bit integer",
            Self::I24 => "24-bit integer",
            Self::I16 => "16-bit integer",
        })
    }
}

/// The discovery snapshot used for playback; the route may later change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    /// Best-effort backend name.
    pub name: Option<String>,
    /// Number of channels in each playback stream frame.
    pub channels: u16,
    /// The backend-reported default stream sample rate.
    pub sample_rate: SampleRate,
    /// The backend-reported default stream format.
    pub format: SampleFormat,
}
