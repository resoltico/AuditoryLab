//! Export of an `auditorylab-core` session to a 32-bit float WAV stream.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

mod error;
pub(crate) use error::CODES as ERROR_CODES;
mod export;
mod header;

pub(crate) use self::error::ExportError;
pub use export::Exported;
pub(crate) use export::encode;
pub use header::Container;

#[cfg(test)]
mod tests;
