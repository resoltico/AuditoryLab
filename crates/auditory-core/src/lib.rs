// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Auditory stimuli: the preset format, the built-in catalog, and sample
//! generation.
//!
//! A [`Preset`] is a TOML document describing a [`Stimulus`] in physical
//! units (hertz, seconds, dBFS) and, optionally, [`About`] text. A
//! stimulus is a sum of [`Component`]s, each routed to the left ear, the
//! right ear or both. [`Stimulus::session`] checks it against a
//! [`SampleRate`] and returns a [`Session`], whose [`Renderer`] produces
//! stereo frames: left ear, then right ear.
//!
//! Every rule about stimuli lives here, so a front end only passes input in
//! and presents what comes back. The crate is `no_std` with `alloc`:
//! production code performs no I/O, spawns no threads and takes no locks.
//! `no_std` removes implicit access to `std`; it does not forbid explicitly
//! linking `std` or using a dependency that does so. Sample arithmetic uses
//! `libm` to avoid the platform mathematics library, and pinned digests
//! check identical rendered bits on the supported platforms.
//!
//! ```
//! use auditory_core::{Preset, SampleRate};
//!
//! let preset = Preset::parse(
//!     r#"
//!     schema_version = 2
//!     duration_seconds = 1.0
//!     fade_seconds = 0.01
//!
//!     [[component]]
//!     ears = "left"
//!     kind = "tone"
//!     frequency_hz = 250.0
//!     peak_dbfs = -20.0
//!
//!     [[component]]
//!     ears = "right"
//!     kind = "tone"
//!     frequency_hz = 256.0
//!     peak_dbfs = -20.0
//!     "#,
//! )?;
//! let session = preset.stimulus.session(SampleRate::DEFAULT)?;
//! assert_eq!(session.frames(), 48_000);
//! let mut renderer = session.into_renderer();
//! let mut frames = [[1.0; 2]; 48_010];
//! assert_eq!(renderer.render(&mut frames), 48_000);
//! assert!(renderer.is_complete());
//! assert_eq!(frames[0], [0.0, 0.0]);
//! assert!(frames.iter().all(|[l, r]| l.abs() <= 0.1 && r.abs() <= 0.1));
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod catalog;
mod generator;
mod numeric;
mod preset;
mod rate;
mod session;
mod stimulus;

pub use preset::{About, Preset, PresetError};
pub use rate::{SampleRate, SampleRateError};
pub use session::{Renderer, Session};
pub use stimulus::{Component, ComponentError, Ear, Ears, Modulation, Stimulus, StimulusError};
