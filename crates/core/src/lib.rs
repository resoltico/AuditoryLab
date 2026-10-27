//! Auditory recipes, validation and pure seekable and live stereo synthesis.
//!
//! A [`Recipe`] is a portable document containing a [`Sound`] and attributed
//! research. A Sound contains identified [`Component`]s, each a [`ComponentForm`] built
//! from a [`Voice`] or an explicit pair. [`Sound::check`] retains diagnostics at
//! one [`SampleRate`]; [`Sound::target`] returns a validated [`Target`]. A
//! [`Signal`] adds the finite envelope. [`live::Engine`] renders targets as an
//! open-ended stream, and an [`Observation`] states the facts of a generated frame.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(test)]
extern crate std;
pub mod authored;
pub mod catalog;
mod compiled;
mod contract;
pub use contract::{ContractError, ReferenceOwner};
mod facts;
pub mod finite;
mod history;
pub mod live;
pub mod rhythm;
mod validity;
pub use validity::{ValueIssue, ValueRule};
mod numeric;
mod parameter;
pub mod provenance;
mod rate;
mod recipe;
mod signal;
mod synthesis;
pub mod timing;
mod voice;
pub mod waveform;
pub use authored::{
    Component, ComponentForm, Pair, PairError, PairIssue, PairKind, Sound, StructureError,
};
pub use compiled::{Target, TargetCheck};
pub use facts::{
    CarrierFacts, ComponentFacts, ModulationFacts, Observation, PairFacts, PhaseRate, PulseFacts,
    SlotRole, VoiceObservation,
};
pub use history::{DocumentIdentity, DocumentUuid, Timestamp};
pub use parameter::{
    CarrierKind, ComponentId, ComponentRole, Lane, ModulationKind, ParameterDescriptor,
    ParameterId, Unit, VoiceId,
};
pub use provenance::{
    BibliographySource, ConditionRole, Description, Evidence, EvidenceScope, Lineage, Measure,
    Population,
};
pub use rate::{SampleRate, SampleRateError};
pub use recipe::{Recipe, RecipeBuilder, RecipeError};
pub use signal::Signal;
pub use voice::{Carrier, Ear, Ears, Modulation, SoundError, Voice, VoiceError};
pub use waveform::Extent;

#[cfg(test)]
mod test_support;
