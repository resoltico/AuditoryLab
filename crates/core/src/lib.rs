//! Auditory recipes, validation and pure seekable and live stereo synthesis.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(test)]
extern crate std;
pub mod catalog;
pub mod live;
mod numeric;
mod preset;
mod rate;
mod signal;
mod stimulus;
mod synthesis;
pub mod waveform;
pub use preset::{About, Preset, PresetError};
pub use rate::{SampleRate, SampleRateError};
pub use signal::Signal;
pub use stimulus::{
    Carrier, Component, ComponentError, Ear, Ears, Modulation, Parameter, Stimulus, StimulusError,
};
pub use waveform::Extent;
