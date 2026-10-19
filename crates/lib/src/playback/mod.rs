//! Default output discovery and callback playback.
mod callback;
mod error;
pub(crate) mod output;
mod shared;
pub(crate) mod source;
pub(crate) use error::PlaybackError;
pub(crate) use output::Output;
