//! Shared application capabilities for Auditory Lab frontends.
//!
//! Core documents and synthesis live in [`core`]. [`recipe`] owns snapshots
//! and renditions, [`editing`] owns editor sessions, [`workflow`] publishes files,
//! and [`workspace`] coordinates interactive use.
#![forbid(unsafe_code)]
pub use auditorylab_core as core;
pub mod calendar;
pub mod collection;
pub mod controls;
#[cfg(feature = "device-playback")]
pub mod device;
pub mod editing;
pub mod error;
pub mod external;
pub mod identity;
mod input_admission;
pub mod library;
pub mod presentation;
mod publish;
pub mod query;
pub mod recipe;
pub mod reports;
pub mod session;
pub mod vocabulary;
pub mod wav;
pub mod waveform_view;
pub mod workflow;
pub mod workspace;
