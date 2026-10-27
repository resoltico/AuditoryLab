//! Playback preparation shares captured-source and Trash policy across frontends.
use super::{Resolved, library::Library};
pub use crate::audio::PlaybackError;
pub use crate::audio::device::{DeviceError, PreparedLive, PreparedPlayback};
pub use crate::audio::session::{
    EndReason, GeneratedStatus, LiveControl, LiveUpdateError, Meter, MonitorDetail, MonitorReader,
    ObservationFreshness, OutputInfo, PlaybackControl, PlaybackMonitor, PlaybackPhase,
    PlaybackReport, Preparation, SampleFormat, Session, SessionDescriptor, SessionId, StopRequest,
};
use {
    crate::document::editing::Draft, crate::document::recipe::Origin,
    crate::document::recipe::RecipeSnapshot, crate::storage::library::LibraryError,
};
/// Prepare one resolved source at the actual output rate, refusing Trash before device access.
///
/// # Errors
/// Preserves captured-library revalidation, acoustic and native-device failures.
pub fn prepare_playback(
    session: Session,
    resolved: &Resolved,
) -> Result<Preparation<PreparedPlayback>, PlaybackError> {
    if let Some(report) = crate::audio::device::stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    if resolved.summary.trashed {
        let error = PlaybackError::Trashed {
            key: resolved.summary.key.clone(),
        };
        return Err(crate::audio::device::failed(&session, error));
    }
    prepare_captured(session, &resolved.snapshot, resolved.managed.as_ref())
}
/// Revalidate a captured User version using the library that admitted it.
///
/// A fresh load is required even when its content hash is unchanged: Trash
/// membership is separate from portable bytes.
pub(crate) fn revalidate_managed(
    snapshot: &RecipeSnapshot,
    library: &Library,
) -> Result<RecipeSnapshot, LibraryError> {
    let Origin::User { id, version } = snapshot.origin() else {
        return Err(LibraryError::ExpectedUser);
    };
    let record = library.load(&crate::document::origin::LibraryKey::User(*id))?;
    if record.trashed || &record.version_token != version {
        return Err(LibraryError::StalePlayback);
    }
    Ok(record.snapshot)
}
pub(crate) fn prepare_captured(
    session: Session,
    snapshot: &RecipeSnapshot,
    library: Option<&Library>,
) -> Result<Preparation<PreparedPlayback>, PlaybackError> {
    if let Some(report) = crate::audio::device::stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    let snapshot = resolve_captured(snapshot, library)
        .map_err(|error| crate::audio::device::failed(&session, error.into()))?;
    crate::audio::device::prepare_playback(session, &snapshot)
}
// The admitted selection supplies managed authority. Origin is provenance:
// retained User snapshots can be temporary content without a library capability.
pub(crate) fn resolve_captured(
    snapshot: &RecipeSnapshot,
    library: Option<&Library>,
) -> Result<RecipeSnapshot, LibraryError> {
    library.map_or_else(
        || Ok(snapshot.clone()),
        |library| revalidate_managed(snapshot, library),
    )
}

/// Prepare a committed live document through the same native preparation boundary.
///
/// # Errors
/// Preserves acoustic, session and native-device failures.
pub fn prepare_live(
    session: Session,
    draft: &Draft,
) -> Result<Preparation<PreparedLive>, PlaybackError> {
    crate::audio::device::prepare_live(session, draft)
}
