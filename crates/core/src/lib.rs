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
mod model;
use model::{
    authored, catalog_id, component_schema, contract, finite, history, parameter, recipe, validity,
};
pub use model::{catalog, provenance, vocabulary};
mod render;
use render::{compiled, conversion, modulation, rate, signal, synthesis, voice};
pub use render::{finite_source, live, waveform};
mod analysis;
use analysis::facts;
pub use analysis::{rhythm, timing};

pub mod diagnostic;
use diagnostic::{Code, Params};
#[cfg(test)]
extern crate std;
pub use authored::{
    Component, ComponentForm, Pair, PairError, PairIssue, PairKind, Sound, StructureError,
};
pub use compiled::{Target, TargetCheck};
pub use contract::{ContractError, ReferenceOwner};
pub use facts::{
    CarrierFacts, ModulationFacts, Observation, PairFacts, PhaseRate, PulseFacts, SlotRole,
    VoiceFacts, VoiceObservation,
};
pub use finite::Finite;
pub use history::{DocumentIdentity, DocumentUuid, Timestamp};
pub use modulation::Modulation;
pub use parameter::{
    CarrierKind, ComponentId, Lane, ModulationKind, ParameterDescriptor, ParameterId, Unit, VoiceId,
};
use provenance::{
    BibliographySource, ConditionRole, Description, Evidence, EvidenceScope, Lineage, Measure,
    Population,
};
pub use rate::{SampleRate, SampleRateError};
pub use recipe::{Recipe, RecipeBuilder, RecipeContent, RecipeError};
pub use signal::Signal;
pub use validity::{ValueIssue, ValueRule};
pub use voice::{Carrier, Ear, Ears, SoundError, VoiceError};
use waveform::Extent;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub(crate) use voice::Voice;
