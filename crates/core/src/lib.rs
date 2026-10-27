//! Auditory recipes, validation and pure seekable and live stereo synthesis.
//!
//! A [`Recipe`] is a portable document containing a [`Sound`] and attributed
//! research. A Sound contains identified [`Component`]s, each a [`ComponentForm`] built
//! from a primitive form or an explicit pair. [`Sound::check`] retains diagnostics at
//! one [`SampleRate`]; [`Sound::target`] returns a validated [`Target`]. A
//! [`Signal`] adds the finite envelope. [`live::Engine`] renders targets as an
//! open-ended stream, and an [`Observation`] states the facts of a generated frame.
#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
extern crate alloc;
#[cfg(test)]
extern crate std;
pub mod authored;
pub mod catalog;
mod catalog_id;
mod compiled;
mod contract;
pub mod query;
pub mod vocabulary;
pub use contract::{ContractError, ReferenceOwner};
mod facts;
pub mod finite;
mod history;
pub mod live;
pub mod rhythm;
mod validity;
pub use validity::{ValueIssue, ValueRule};
mod modulation;
mod number;
pub use modulation::Modulation;
mod numeric;
pub use number::Finite;
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
    CarrierFacts, ModulationFacts, Observation, PairFacts, PhaseRate, PulseFacts, SlotRole,
    VoiceFacts, VoiceObservation,
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
pub use recipe::{Recipe, RecipeBuilder, RecipeContent, RecipeError, UNTITLED_SOUND, copy_title};
pub use signal::Signal;
pub use voice::{Carrier, Ear, Ears, SoundError, VoiceError};
pub use waveform::Extent;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub(crate) use voice::Voice;
