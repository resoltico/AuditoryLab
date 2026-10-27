//! Shared application capabilities for Auditory Lab frontends.
//!
//! [`core`] owns pure documents and synthesis. [`document`] owns editable
//! snapshots, [`discovery`] owns queries, [`services`] exposes operations,
//! [`workspace`] coordinates interaction, and [`presentation`] supplies shared text.
#![forbid(unsafe_code)]
pub use auditorylab_core as core;
pub(crate) mod audio;
pub mod clock;
pub mod discovery;
pub mod document;
pub mod identity;
pub mod job;
pub mod outcome;
pub mod presentation;
pub mod services;
pub(crate) mod storage;
pub mod workspace;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_recipes;
