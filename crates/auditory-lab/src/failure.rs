// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::args::HelpTopic;
use auditory_core::{PresetError, StimulusError};
use auditory_device::PlaybackError;
use auditory_wav::ExportError;
use ctrlc::Error as InterruptError;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;

/// The exit status of a run that was interrupted, by shell convention.
pub(crate) const INTERRUPTED: u8 = 130;

/// Why an invocation did not succeed. The libraries say what went wrong;
/// this adds where, and gives each cause its exit status.
#[derive(Debug)]
pub(crate) enum Failure {
    Usage {
        topic: HelpTopic,
        message: String,
    },
    Output(io::Error),
    Read {
        origin: String,
        error: io::Error,
    },
    UnknownPreset {
        name: OsString,
        accepts_files: bool,
    },
    Stimulus {
        origin: String,
        error: StimulusError,
    },
    Preset {
        origin: String,
        error: PresetError,
    },
    Interrupts(InterruptError),
    Export {
        path: PathBuf,
        error: ExportError,
    },
    Playback {
        origin: String,
        error: PlaybackError,
    },
}

impl Failure {
    /// Final diagnostic, using the same authoritative cancellation classification.
    pub(crate) fn diagnostic(&self) -> String {
        if self.exit_code() == INTERRUPTED {
            match self {
                Self::Export { path, .. } => format!("cancelled: export to {path:?}\n"),
                Self::Playback { origin, .. } => format!("cancelled: playback of {origin}\n"),
                _ => unreachable!(),
            }
        } else {
            format!("error: {self}\n")
        }
    }

    /// 2 for input the user must correct, 130 for a cancelled run, 1 for any
    /// other failure.
    pub(crate) const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage { .. } => 2,
            Self::Preset { .. } | Self::Stimulus { .. } | Self::UnknownPreset { .. } => 2,
            Self::Export {
                error: ExportError::Cancelled,
                ..
            }
            | Self::Playback {
                error: PlaybackError::Cancelled,
                ..
            } => INTERRUPTED,
            Self::Read { .. }
            | Self::Output(_)
            | Self::Interrupts(_)
            | Self::Export { .. }
            | Self::Playback { .. } => 1,
        }
    }
}

impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Usage { message, .. } => f.write_str(message),
            Self::Output(error) => write!(f, "cannot write to standard output: {error}"),
            Self::Preset { origin, error } => write!(f, "{origin}: {error}"),
            Self::Stimulus { origin, error } => write!(f, "{origin}: {error}"),
            Self::Read { origin, error } => write!(f, "{origin}: preset cannot be read: {error}"),
            Self::UnknownPreset {
                name,
                accepts_files,
            } => {
                write!(
                    f,
                    "no built-in preset is named {name:?}; run auditory-lab list"
                )?;
                if *accepts_files {
                    f.write_str(", or name a file such as ./x")
                } else {
                    f.write_str(" and select a built-in preset name")
                }
            }
            Self::Interrupts(error) => write!(f, "cannot handle interrupts: {error}"),
            Self::Export { path, error } => {
                write!(f, "cannot export to {path:?}: {error}")
            }
            Self::Playback { origin, error } => write!(f, "{origin}: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Failure;
    use auditory_core::PresetError;
    use auditory_device::PlaybackError;
    use auditory_wav::ExportError;
    use std::io;

    fn usage(message: &str) -> Failure {
        Failure::Usage {
            topic: crate::args::HelpTopic::Global,
            message: message.into(),
        }
    }
    fn playback(error: PlaybackError) -> Failure {
        Failure::Playback {
            origin: "tone.toml".into(),
            error,
        }
    }
    fn export(error: ExportError) -> Failure {
        Failure::Export {
            path: "out.wav".into(),
            error,
        }
    }

    fn preset(error: PresetError) -> Failure {
        Failure::Preset {
            origin: "tone.toml".into(),
            error,
        }
    }

    #[test]
    fn each_cause_has_its_exit_status() {
        assert_eq!(usage("").exit_code(), 2);
        assert_eq!(preset(PresetError::TooLarge).exit_code(), 2);
        assert_eq!(preset(PresetError::NotUtf8).exit_code(), 2);
        assert_eq!(
            preset(PresetError::UnsupportedVersion { found: 2 }).exit_code(),
            2
        );
        assert_eq!(
            Failure::Read {
                origin: "input".into(),
                error: io::ErrorKind::Other.into()
            }
            .exit_code(),
            1
        );
        assert_eq!(Failure::Output(io::ErrorKind::Other.into()).exit_code(), 1);
        assert_eq!(export(ExportError::Cancelled).exit_code(), 130);
        assert_eq!(export(ExportError::DestinationExists).exit_code(), 1);
        assert_eq!(playback(PlaybackError::Cancelled).exit_code(), 130);
        assert_eq!(playback(PlaybackError::NoDefaultOutput).exit_code(), 1);
        // A cancelled export that left its incomplete file behind is a failure.
        assert_eq!(
            export(ExportError::IncompleteFileLeft {
                failure: Box::new(ExportError::Cancelled),
                removal: io::ErrorKind::PermissionDenied.into()
            })
            .exit_code(),
            1
        );
    }

    #[test]
    fn cancellation_has_dedicated_wording_and_cleanup_failure_keeps_both_causes() {
        assert_eq!(
            export(ExportError::Cancelled).diagnostic(),
            "cancelled: export to \"out.wav\"\n"
        );
        assert_eq!(
            playback(PlaybackError::Cancelled).diagnostic(),
            "cancelled: playback of tone.toml\n"
        );
        let failed = export(ExportError::IncompleteFileLeft {
            failure: Box::new(ExportError::Cancelled),
            removal: io::Error::other("removal denied"),
        });
        let text = failed.diagnostic();
        assert!(text.starts_with("error: cannot export to \"out.wav\":"));
        assert!(text.contains("cancelled"));
        assert!(text.contains("removal denied"));
        assert_eq!(failed.exit_code(), 1);
    }

    #[test]
    fn messages_add_where_to_what_the_libraries_report() {
        assert_eq!(
            preset(PresetError::NotUtf8).to_string(),
            "tone.toml: preset is not UTF-8 text"
        );
        assert_eq!(
            export(ExportError::DestinationExists).to_string(),
            "cannot export to \"out.wav\": the destination already exists and was left untouched"
        );
        assert_eq!(
            Failure::Output(io::Error::other("disk full")).to_string(),
            "cannot write to standard output: disk full"
        );
        assert_eq!(
            playback(PlaybackError::NoDefaultOutput).to_string(),
            "tone.toml: no default audio output device found"
        );
        assert_eq!(usage("bad").to_string(), "bad");
    }
}
