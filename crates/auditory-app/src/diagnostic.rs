// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Immutable diagnostic detail and original technical causes.
use crate::{Operation, Origin};
use auditory_core::{Preset, PresetError, StimulusError};
use auditory_device::PlaybackError;
use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt::{self, Display, Formatter},
    io,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Coarse classification suitable for frontend policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Input must be corrected.
    InvalidInput,
    /// Clean cooperative cancellation.
    Cancelled,
    /// Runtime or filesystem failure.
    Operational,
}
/// Coarse reason derived from the authoritative diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Unknown catalog query.
    UnknownCatalog,
    /// Input open/read failure.
    Read,
    /// Strict document decoding failed.
    Preset,
    /// Rate-dependent validation failed.
    Validation,
    /// Native output failed.
    Device,
    /// Cooperative cancellation.
    Cancelled,
    /// Final no-clobber commit conflict.
    DestinationExists,
    /// Staging, writing, syncing or publication failed.
    Publication,
    /// Bounded generated-signal observation could not be prepared.
    Observation,
}
/// Parser-independent input rejection facts.
#[derive(Debug, Clone)]
pub enum InputDiagnostic {
    /// The opened document handle is not a regular file. Intentional streams use load_reader.
    NotRegularFile,
    /// Input exceeded the strict byte limit. A bounded reader only observes a prefix.
    TooLarge {
        /// Maximum accepted bytes.
        limit: usize,
        /// Bytes actually observed, possibly only a lower bound on stream length.
        observed_bytes: usize,
    },
    /// A UTF-16 byte-order mark was found.
    Utf16,
    /// Input was not valid UTF-8.
    InvalidUtf8,
    /// The parser rejected the exact retained UTF-8 snapshot.
    Format(FormatDiagnostic),
    /// Explicit schema version is unsupported.
    UnsupportedSchema {
        /// Rejected version.
        found: u32,
        /// Version supported by this build.
        supported: u32,
    },
    /// Exact native query did not identify a catalog entry.
    UnknownCatalog {
        /// Unmodified query, including non-Unicode native values.
        query: OsString,
    },
}
/// Parser message and optional validated source span from one immutable failed input.
#[derive(Debug, Clone)]
pub struct FormatDiagnostic {
    message: String,
    input: Arc<str>,
    span: Option<Range<usize>>,
}
impl FormatDiagnostic {
    fn new(message: String, input: Arc<str>, span: Option<Range<usize>>) -> Self {
        let span =
            span.filter(|range| range.start <= range.end && input.get(range.clone()).is_some());
        Self {
            message,
            input,
            span,
        }
    }
    /// Parser-provided explanation, without extracting structure from its prose.
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Exact bounded input used by the parser; no file is reread.
    pub fn input(&self) -> &str {
        &self.input
    }
    /// Parser-provided UTF-8 byte range, absent if unavailable or invalid.
    pub fn span(&self) -> Option<Range<usize>> {
        self.span.clone()
    }
    /// The original input substring for a valid parser span.
    pub fn snippet(&self) -> Option<&str> {
        self.span
            .as_ref()
            .and_then(|span| self.input.get(span.clone()))
    }
}
/// Filesystem boundary where an operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemPhase {
    /// Open the requested input and inspect its handle for document admission.
    OpenInput,
    /// Read the bounded input stream.
    ReadInput,
    /// Create a same-directory staging file.
    CreateStage,
    /// Write the owned stage.
    WriteStage,
    /// Sync the complete stage before publication.
    SyncStage,
    /// Publish without replacing any destination entry.
    Commit,
    /// Remove an owned failed stage.
    Cleanup,
}
/// Native filesystem facts without requiring a renderer to downcast the source.
#[derive(Debug, Clone)]
pub struct FilesystemDiagnostic {
    phase: FilesystemPhase,
    kind: io::ErrorKind,
    requested: Option<PathBuf>,
    stage: Option<PathBuf>,
}
impl FilesystemDiagnostic {
    /// Boundary that failed.
    pub fn phase(&self) -> FilesystemPhase {
        self.phase
    }
    /// Native error kind.
    pub fn kind(&self) -> io::ErrorKind {
        self.kind
    }
    /// Requested native file path, absent for an anonymous reader.
    pub fn requested_path(&self) -> Option<&Path> {
        self.requested.as_deref()
    }
    /// Owned stage involved, if creation succeeded.
    pub fn stage(&self) -> Option<&Path> {
        self.stage.as_deref()
    }
}
/// Backend-independent playback rejection facts; no CPAL types enter the frontend API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceDiagnostic {
    /// No default output was found.
    NoDefaultOutput,
    /// Native host/server is unavailable.
    HostUnavailable,
    /// Requested device is unavailable.
    DeviceUnavailable,
    /// Output route changed.
    RouteChanged,
    /// Configuration was invalidated.
    ConfigurationInvalidated,
    /// Device is busy.
    Busy,
    /// Permission was denied.
    PermissionDenied,
    /// OS resource limit was reached.
    ResourceExhausted,
    /// Backend rejected an input argument.
    InvalidInput,
    /// Backend does not support the requested configuration.
    UnsupportedConfiguration,
    /// Backend does not support the requested operation.
    UnsupportedOperation,
    /// Native realtime scheduling was refused during a fatal setup operation.
    RealtimeDenied,
    /// A fatal setup operation reported an underrun/overrun.
    BufferUnderrun,
    /// Device rate is outside the supported domain.
    UnsupportedRate {
        /// Rejected hertz.
        hz: u32,
    },
    /// Native representation is unsupported.
    UnsupportedRepresentation {
        /// Backend's representation name.
        representation: String,
    },
    /// Native configuration lacks two ear channels.
    TooFewChannels {
        /// Reported channels.
        channels: u16,
    },
    /// Session and device rates differ.
    RateMismatch {
        /// Session rate.
        session_hz: u32,
        /// Device rate.
        device_hz: u32,
    },
    /// Backend supplied invalid clock data.
    InvalidTiming,
    /// Buffer contained a partial frame.
    MisalignedBuffer,
    /// Watchdog published timeout.
    TimedOut,
    /// Native backend did not expose a recognized category.
    UnknownBackend,
}
/// Authoritative typed diagnostic carried by an application-created immutable error.
#[derive(Debug, Clone)]
pub enum Diagnostic {
    /// Strict input decoding or catalog query rejection.
    Input(InputDiagnostic),
    /// Actual authoritative core validation failure, without an invented semantic span.
    Validation(StimulusError),
    /// Backend-independent playback failure.
    Device(DeviceDiagnostic),
    /// Filesystem boundary failure with native facts.
    Filesystem(FilesystemDiagnostic),
    /// Cooperative cancellation was observed by the operation.
    Cancelled,
    /// Observation configuration failed before audio started.
    Observation(crate::ObservationError),
}
/// Failure to remove an owned stage, retained alongside the primary cause.
#[derive(Debug)]
pub struct CleanupFailure {
    stage: PathBuf,
    error: io::Error,
}
impl CleanupFailure {
    /// Surviving owned staging pathname.
    pub fn stage(&self) -> &Path {
        &self.stage
    }
    /// Native cleanup cause.
    pub fn error(&self) -> &io::Error {
        &self.error
    }
    /// Cleanup is a separate filesystem phase from the primary failure.
    pub fn phase(&self) -> FilesystemPhase {
        FilesystemPhase::Cleanup
    }
}
/// Immutable, transferable application failure with the original technical source.
#[derive(Debug)]
pub struct AppError {
    operation: Operation,
    origin: Option<Origin>,
    destination: Option<PathBuf>,
    diagnostic: Box<Diagnostic>,
    source: Box<dyn Error + Send + Sync>,
    cleanup: Option<Box<CleanupFailure>>,
}
impl AppError {
    fn new(
        operation: Operation,
        origin: Option<Origin>,
        destination: Option<PathBuf>,
        diagnostic: Diagnostic,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            operation,
            origin,
            destination,
            diagnostic: Box::new(diagnostic),
            source: Box::new(source),
            cleanup: None,
        }
    }
    /// Failed application operation.
    pub fn operation(&self) -> Operation {
        self.operation
    }
    /// Original typed input context.
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
    /// Requested native output path.
    pub fn destination(&self) -> Option<&Path> {
        self.destination.as_deref()
    }
    /// Authoritative diagnostic facts, borrowed without mutation.
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
    /// Additional owned-stage cleanup failure, if any.
    pub fn cleanup(&self) -> Option<&CleanupFailure> {
        self.cleanup.as_deref()
    }
    /// Reason derived from diagnostic facts, never independently relabeled.
    pub fn reason(&self) -> Reason {
        match self.diagnostic.as_ref() {
            Diagnostic::Input(InputDiagnostic::UnknownCatalog { .. }) => Reason::UnknownCatalog,
            Diagnostic::Input(InputDiagnostic::NotRegularFile) => Reason::Read,
            Diagnostic::Input(_) => Reason::Preset,
            Diagnostic::Validation(_) => Reason::Validation,
            Diagnostic::Device(_) => Reason::Device,
            Diagnostic::Cancelled => Reason::Cancelled,
            Diagnostic::Observation(_) => Reason::Observation,
            Diagnostic::Filesystem(detail) => match detail.phase {
                FilesystemPhase::OpenInput | FilesystemPhase::ReadInput => Reason::Read,
                FilesystemPhase::Commit if detail.kind == io::ErrorKind::AlreadyExists => {
                    Reason::DestinationExists
                }
                _ => Reason::Publication,
            },
        }
    }
    /// Cleanup failure always makes the result operational, including after cancellation.
    pub fn class(&self) -> ErrorClass {
        if self.cleanup.is_some() {
            ErrorClass::Operational
        } else {
            match self.diagnostic.as_ref() {
                Diagnostic::Input(_) | Diagnostic::Validation(_) => ErrorClass::InvalidInput,
                Diagnostic::Cancelled => ErrorClass::Cancelled,
                _ => ErrorClass::Operational,
            }
        }
    }
    pub(crate) fn validation(operation: Operation, origin: Origin, error: StimulusError) -> Self {
        Self::new(
            operation,
            Some(origin),
            None,
            Diagnostic::Validation(error.clone()),
            error,
        )
    }
    pub(crate) fn observation(origin: Origin, error: crate::ObservationError) -> Self {
        Self::new(
            Operation::Playback,
            Some(origin),
            None,
            Diagnostic::Observation(error.clone()),
            error,
        )
    }
    pub(crate) fn filesystem(
        operation: Operation,
        origin: Option<Origin>,
        destination: Option<&Path>,
        phase: FilesystemPhase,
        requested: Option<&Path>,
        stage: Option<&Path>,
        error: io::Error,
    ) -> Self {
        Self::new(
            operation,
            origin,
            destination.map(Path::to_owned),
            Diagnostic::Filesystem(FilesystemDiagnostic {
                phase,
                kind: error.kind(),
                requested: requested.map(Path::to_owned),
                stage: stage.map(Path::to_owned),
            }),
            error,
        )
    }
    pub(crate) fn cancelled(
        operation: Operation,
        origin: Option<Origin>,
        destination: &Path,
    ) -> Self {
        Self::new(
            operation,
            origin,
            Some(destination.to_owned()),
            Diagnostic::Cancelled,
            io::Error::other("cancelled before completion"),
        )
    }
    pub(crate) fn with_cleanup(mut self, stage: PathBuf, error: io::Error) -> Self {
        self.cleanup = Some(Box::new(CleanupFailure { stage, error }));
        self
    }
    pub(crate) fn not_regular_file(origin: Origin) -> Self {
        Self::new(
            Operation::Load,
            Some(origin),
            None,
            Diagnostic::Input(InputDiagnostic::NotRegularFile),
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "recipe path must refer to a regular file; choose a regular UTF-8 TOML file or use an explicit stream reader (CLI --stdin)",
            ),
        )
    }
    pub(crate) fn unknown_catalog(query: &OsStr) -> Self {
        Self::new(
            Operation::Load,
            query.to_str().map(|name| Origin::BuiltIn(name.to_owned())),
            None,
            Diagnostic::Input(InputDiagnostic::UnknownCatalog {
                query: query.to_owned(),
            }),
            io::Error::other(format!("no built-in preset is named {query:?}")),
        )
    }
    pub(crate) fn input(origin: Origin, bytes: &[u8], error: PresetError) -> Self {
        let detail = match &error {
            PresetError::TooLarge => InputDiagnostic::TooLarge {
                limit: Preset::MAX_BYTES,
                observed_bytes: bytes.len(),
            },
            PresetError::Utf16 => InputDiagnostic::Utf16,
            PresetError::NotUtf8 => InputDiagnostic::InvalidUtf8,
            PresetError::UnsupportedVersion { found } => InputDiagnostic::UnsupportedSchema {
                found: *found,
                supported: Preset::SCHEMA_VERSION,
            },
            PresetError::Format(parser) => {
                // Format failures can only follow the core's bounded UTF-8 check.
                let input: Arc<str> = match std::str::from_utf8(bytes) {
                    Ok(text) => Arc::from(text),
                    Err(_) => unreachable!("core format errors follow successful UTF-8 decoding"),
                };
                InputDiagnostic::Format(FormatDiagnostic::new(
                    parser.message().to_owned(),
                    input,
                    parser.span(),
                ))
            }
        };
        Self::new(
            Operation::Load,
            Some(origin),
            None,
            Diagnostic::Input(detail),
            error,
        )
    }
    pub(crate) fn playback(origin: Origin, error: PlaybackError) -> Self {
        let diagnostic = match &error {
            PlaybackError::Cancelled => Diagnostic::Cancelled,
            PlaybackError::NoDefaultOutput => Diagnostic::Device(DeviceDiagnostic::NoDefaultOutput),
            PlaybackError::UnsupportedSampleFormat(format) => {
                Diagnostic::Device(DeviceDiagnostic::UnsupportedRepresentation {
                    representation: format.to_string(),
                })
            }
            PlaybackError::UnsupportedSampleRate { hz } => {
                Diagnostic::Device(DeviceDiagnostic::UnsupportedRate { hz: *hz })
            }
            PlaybackError::SampleRateMismatch {
                session_hz,
                device_hz,
            } => Diagnostic::Device(DeviceDiagnostic::RateMismatch {
                session_hz: *session_hz,
                device_hz: *device_hz,
            }),
            PlaybackError::TooFewChannels { channels } => {
                Diagnostic::Device(DeviceDiagnostic::TooFewChannels {
                    channels: *channels,
                })
            }
            PlaybackError::InvalidTiming => Diagnostic::Device(DeviceDiagnostic::InvalidTiming),
            PlaybackError::MisalignedBuffer => {
                Diagnostic::Device(DeviceDiagnostic::MisalignedBuffer)
            }
            PlaybackError::TimedOut => Diagnostic::Device(DeviceDiagnostic::TimedOut),
            PlaybackError::Backend(error) => Diagnostic::Device(match error.kind() {
                cpal::ErrorKind::DeviceBusy => DeviceDiagnostic::Busy,
                cpal::ErrorKind::DeviceChanged => DeviceDiagnostic::RouteChanged,
                cpal::ErrorKind::DeviceNotAvailable => DeviceDiagnostic::DeviceUnavailable,
                cpal::ErrorKind::HostUnavailable => DeviceDiagnostic::HostUnavailable,
                cpal::ErrorKind::InvalidInput => DeviceDiagnostic::InvalidInput,
                cpal::ErrorKind::PermissionDenied => DeviceDiagnostic::PermissionDenied,
                cpal::ErrorKind::RealtimeDenied => DeviceDiagnostic::RealtimeDenied,
                cpal::ErrorKind::ResourceExhausted => DeviceDiagnostic::ResourceExhausted,
                cpal::ErrorKind::StreamInvalidated => DeviceDiagnostic::ConfigurationInvalidated,
                cpal::ErrorKind::UnsupportedConfig => DeviceDiagnostic::UnsupportedConfiguration,
                cpal::ErrorKind::UnsupportedOperation => DeviceDiagnostic::UnsupportedOperation,
                cpal::ErrorKind::Xrun => DeviceDiagnostic::BufferUnderrun,
                cpal::ErrorKind::BackendError | cpal::ErrorKind::Other => {
                    DeviceDiagnostic::UnknownBackend
                }
                _ => DeviceDiagnostic::UnknownBackend,
            }),
        };
        Self::new(Operation::Playback, Some(origin), None, diagnostic, error)
    }
}
impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.origin {
            Some(Origin::BuiltIn(name)) => write!(f, "{name:?} (built-in): ")?,
            Some(Origin::File(path)) => write!(f, "{path:?}: ")?,
            Some(Origin::Reader(label)) => write!(f, "{label:?}: ")?,
            None => {}
        }
        if let Some(path) = &self.destination {
            write!(
                f,
                "cannot {} to {path:?}: ",
                if self.operation == Operation::Save {
                    "save"
                } else {
                    "export"
                }
            )?;
        }
        if self.reason() == Reason::Read {
            f.write_str("preset cannot be read: ")?;
        }
        if self.reason() == Reason::DestinationExists {
            f.write_str("the destination already exists and was left untouched")?;
        } else {
            write!(f, "{}", self.source)?;
        }
        if let Some(cleanup) = &self.cleanup {
            write!(
                f,
                "; staging file remains at {:?}: cannot remove it: {}",
                cleanup.stage, cleanup.error
            )?;
        }
        Ok(())
    }
}
impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn origin() -> Origin {
        Origin::Reader("native mapping".into())
    }
    #[test]
    fn every_current_backend_kind_maps_without_losing_technical_cause() {
        for (kind, expected) in [
            (cpal::ErrorKind::DeviceBusy, DeviceDiagnostic::Busy),
            (
                cpal::ErrorKind::DeviceChanged,
                DeviceDiagnostic::RouteChanged,
            ),
            (
                cpal::ErrorKind::DeviceNotAvailable,
                DeviceDiagnostic::DeviceUnavailable,
            ),
            (
                cpal::ErrorKind::HostUnavailable,
                DeviceDiagnostic::HostUnavailable,
            ),
            (
                cpal::ErrorKind::InvalidInput,
                DeviceDiagnostic::InvalidInput,
            ),
            (
                cpal::ErrorKind::PermissionDenied,
                DeviceDiagnostic::PermissionDenied,
            ),
            (
                cpal::ErrorKind::RealtimeDenied,
                DeviceDiagnostic::RealtimeDenied,
            ),
            (
                cpal::ErrorKind::ResourceExhausted,
                DeviceDiagnostic::ResourceExhausted,
            ),
            (
                cpal::ErrorKind::StreamInvalidated,
                DeviceDiagnostic::ConfigurationInvalidated,
            ),
            (
                cpal::ErrorKind::UnsupportedConfig,
                DeviceDiagnostic::UnsupportedConfiguration,
            ),
            (
                cpal::ErrorKind::UnsupportedOperation,
                DeviceDiagnostic::UnsupportedOperation,
            ),
            (cpal::ErrorKind::Xrun, DeviceDiagnostic::BufferUnderrun),
            (
                cpal::ErrorKind::BackendError,
                DeviceDiagnostic::UnknownBackend,
            ),
            (cpal::ErrorKind::Other, DeviceDiagnostic::UnknownBackend),
        ] {
            let native = cpal::Error::with_message(kind, "original backend evidence");
            let error = AppError::playback(origin(), PlaybackError::Backend(native.clone()));
            assert!(matches!(error.diagnostic(),Diagnostic::Device(actual) if actual==&expected));
            assert_eq!(error.reason(), Reason::Device);
            assert_eq!(error.class(), ErrorClass::Operational);
            let retained = error
                .source()
                .unwrap()
                .source()
                .unwrap()
                .downcast_ref::<cpal::Error>()
                .unwrap();
            assert_eq!(retained, &native);
        }
    }
    #[test]
    fn every_playback_variant_preserves_numeric_configuration_and_cancellation_class() {
        for (source, expected) in [
            (
                PlaybackError::NoDefaultOutput,
                DeviceDiagnostic::NoDefaultOutput,
            ),
            (
                PlaybackError::UnsupportedSampleFormat(cpal::SampleFormat::U16),
                DeviceDiagnostic::UnsupportedRepresentation {
                    representation: cpal::SampleFormat::U16.to_string(),
                },
            ),
            (
                PlaybackError::UnsupportedSampleRate { hz: 999999 },
                DeviceDiagnostic::UnsupportedRate { hz: 999999 },
            ),
            (
                PlaybackError::SampleRateMismatch {
                    session_hz: 44100,
                    device_hz: 48000,
                },
                DeviceDiagnostic::RateMismatch {
                    session_hz: 44100,
                    device_hz: 48000,
                },
            ),
            (
                PlaybackError::TooFewChannels { channels: 1 },
                DeviceDiagnostic::TooFewChannels { channels: 1 },
            ),
            (
                PlaybackError::InvalidTiming,
                DeviceDiagnostic::InvalidTiming,
            ),
            (
                PlaybackError::MisalignedBuffer,
                DeviceDiagnostic::MisalignedBuffer,
            ),
            (PlaybackError::TimedOut, DeviceDiagnostic::TimedOut),
        ] {
            let error = AppError::playback(origin(), source);
            assert!(matches!(error.diagnostic(),Diagnostic::Device(actual) if actual==&expected));
            assert_eq!(error.operation(), Operation::Playback);
        }
        let error = AppError::playback(origin(), PlaybackError::Cancelled);
        assert!(matches!(error.diagnostic(), Diagnostic::Cancelled));
        assert_eq!(error.class(), ErrorClass::Cancelled);
        assert_eq!(error.reason(), Reason::Cancelled);
    }
    #[test]
    fn parser_span_absence_and_invalid_multibyte_ranges_are_not_invented() {
        let input: Arc<str> = Arc::from("aΩz");
        for span in [
            None,
            Some(2..3),
            Some(9..10),
            Some(Range { start: 3, end: 2 }),
        ] {
            let detail = FormatDiagnostic::new("parser message".into(), input.clone(), span);
            assert!(detail.span().is_none());
            assert!(detail.snippet().is_none());
            assert_eq!(detail.input(), "aΩz");
        }
        let detail = FormatDiagnostic::new("parser message".into(), input, Some(1..3));
        assert_eq!(detail.span(), Some(1..3));
        assert_eq!(detail.snippet(), Some("Ω"));
    }
}
