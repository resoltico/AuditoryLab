// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Export of an `auditory-core` session to a 32-bit float WAV stream.

mod error;
mod export;
mod header;

pub use error::ExportError;
pub use export::{Exported, encode};
pub use header::Container;
