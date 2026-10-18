// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Export of an `auditorylab-core` session to a 32-bit float WAV stream.

mod error;
mod export;
mod header;

pub(crate) use error::ExportError;
pub use export::Exported;
pub(crate) use export::encode;
pub use header::Container;

#[cfg(test)]
mod tests;
