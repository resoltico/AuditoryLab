// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Why a session was not played to its end.
#[derive(Debug, Clone)]
pub(crate) enum PlaybackError {
    /// Cancellation was observed before playback completed.
    Cancelled,
    /// No default output device is available.
    NoDefaultOutput,
    /// The audio backend refused an operation or reported that the stream
    /// cannot continue.
    Backend(cpal::Error),
    /// The device's stream uses a sample representation playback cannot fill.
    UnsupportedSampleFormat(cpal::SampleFormat),
    /// The device runs at a sample rate sessions cannot have.
    UnsupportedSampleRate {
        /// The device's sample rate in frames per second.
        hz: u32,
    },
    /// The session was built for another sample rate than the device's.
    SampleRateMismatch {
        /// The session's sample rate in frames per second.
        session_hz: u32,
        /// The device's sample rate in frames per second.
        device_hz: u32,
    },
    /// The default configuration cannot present separate ear channels.
    TooFewChannels {
        /// Available channels in each device frame.
        channels: u16,
    },
    /// The backend's clocks ran backwards or beyond their range.
    InvalidTiming,
    /// The backend supplied a buffer holding a partial frame.
    MisalignedBuffer,
    /// The stream stopped making progress or outlasted its time limit.
    TimedOut,
}

impl Display for PlaybackError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "playback cancelled"),
            Self::NoDefaultOutput => write!(f, "no default audio output device found"),
            Self::Backend(error) if error.kind() == cpal::ErrorKind::DeviceChanged => write!(
                f,
                "the audio output route changed and playback stopped; select the intended output and play again"
            ),
            Self::Backend(error) => write!(f, "audio backend: {error}"),
            Self::UnsupportedSampleFormat(format) => write!(
                f,
                "the output device's {format} sample format is not supported"
            ),
            Self::UnsupportedSampleRate { hz } => write!(
                f,
                "the output device's {hz} Hz sample rate is not supported"
            ),
            Self::SampleRateMismatch {
                session_hz,
                device_hz,
            } => write!(
                f,
                "the session was built for {session_hz} Hz but the output device runs at {device_hz} Hz"
            ),
            Self::TooFewChannels { channels } => write!(
                f,
                "the default output device has {channels} channel{}; stimuli need two, one for each ear",
                if *channels == 1 { "" } else { "s" }
            ),
            Self::InvalidTiming => write!(f, "the audio backend supplied invalid playback timing"),
            Self::MisalignedBuffer => {
                write!(
                    f,
                    "the audio backend supplied a buffer holding a partial frame"
                )
            }
            Self::TimedOut => write!(
                f,
                "the audio stream stopped making progress or outlasted its time limit"
            ),
        }
    }
}

impl Error for PlaybackError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            _ => None,
        }
    }
}

impl From<cpal::Error> for PlaybackError {
    fn from(error: cpal::Error) -> Self {
        Self::Backend(error)
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackError;
    use std::error::Error;

    #[test]
    fn a_route_change_says_what_happened_and_what_to_do() {
        let changed = PlaybackError::from(cpal::Error::new(cpal::ErrorKind::DeviceChanged));
        assert_eq!(
            changed.to_string(),
            "the audio output route changed and playback stopped; select the intended output and play again"
        );
        assert!(changed.source().is_some());
        let busy = PlaybackError::from(cpal::Error::with_message(
            cpal::ErrorKind::DeviceBusy,
            "in use",
        ));
        assert_eq!(busy.to_string(), "audio backend: in use");
        assert!(PlaybackError::TimedOut.source().is_none());
    }

    #[test]
    fn mismatches_name_both_sides() {
        assert_eq!(
            PlaybackError::TooFewChannels { channels: 0 }.to_string(),
            "the default output device has 0 channels; stimuli need two, one for each ear"
        );
        assert_eq!(
            PlaybackError::SampleRateMismatch {
                session_hz: 48_000,
                device_hz: 44_100
            }
            .to_string(),
            "the session was built for 48000 Hz but the output device runs at 44100 Hz"
        );
        assert_eq!(
            PlaybackError::TooFewChannels { channels: 1 }.to_string(),
            "the default output device has 1 channel; stimuli need two, one for each ear"
        );
        assert_eq!(
            PlaybackError::UnsupportedSampleRate { hz: 1_536_000 }.to_string(),
            "the output device's 1536000 Hz sample rate is not supported"
        );
        assert_eq!(
            PlaybackError::UnsupportedSampleFormat(cpal::SampleFormat::U8).to_string(),
            "the output device's u8 sample format is not supported"
        );
    }
}
