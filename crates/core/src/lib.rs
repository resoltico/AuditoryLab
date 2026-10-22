//! Auditory recipes, validation and pure seekable and live stereo synthesis.
//!
//! A [`Recipe`] is the raw document: a [`Stimulus`] and optional [`About`]
//! text. A [`TargetCheck`] compiles a stimulus's components at one
//! [`SampleRate`] into a validated [`Target`]; a [`Signal`] adds the finite
//! envelope. [`live::Engine`] renders targets as an open-ended stream, and an
//! [`Observation`] states the facts of a generated frame.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(test)]
extern crate std;
pub mod catalog;
mod compiled;
mod facts;
pub mod interpretation;
pub mod live;
mod numeric;
mod parameter;
mod rate;
mod recipe;
mod signal;
mod stimulus;
mod synthesis;
pub mod timing;
pub mod waveform;
pub use compiled::{Target, TargetCheck};
pub use facts::{
    CarrierFacts, ComponentFacts, ModulationFacts, Observation, PhaseRate, PulseFacts, SlotRole,
    VoiceObservation,
};
pub use parameter::{
    CarrierKind, ComponentId, ComponentRole, ModulationKind, ParameterDescriptor, ParameterId, Unit,
};
pub use rate::{SampleRate, SampleRateError};
pub use recipe::{About, Recipe, RecipeError};
pub use signal::Signal;
pub use stimulus::{
    Carrier, Component, ComponentError, Ear, Ears, Modulation, Stimulus, StimulusError,
};
pub use waveform::Extent;
