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
#[path = "model/component_schema.rs"]
mod component_schema;
pub mod diagnostic;
pub use diagnostic::{Code, ParamValue, Params};
#[cfg(test)]
extern crate std;
#[path = "model/authored.rs"]
pub mod authored;
#[path = "model/catalog.rs"]
pub mod catalog;
#[path = "model/catalog_id.rs"]
mod catalog_id;
#[path = "render/compiled.rs"]
mod compiled;
#[path = "model/contract.rs"]
mod contract;
#[path = "analysis/query.rs"]
pub mod query;
#[path = "model/vocabulary.rs"]
pub mod vocabulary;
pub use contract::{ContractError, ReferenceOwner};
#[path = "analysis/facts.rs"]
mod facts;
#[path = "render/finite_source.rs"]
pub mod finite_source;
#[path = "model/history.rs"]
mod history;
#[path = "render/live.rs"]
pub mod live;
#[path = "analysis/rhythm.rs"]
pub mod rhythm;
#[path = "model/validity.rs"]
mod validity;
pub use validity::{ValueIssue, ValueRule};
#[path = "model/finite.rs"]
mod finite;
#[path = "render/modulation.rs"]
mod modulation;
pub use modulation::Modulation;
#[path = "render/conversion.rs"]
mod conversion;
pub use finite::Finite;
#[path = "model/parameter.rs"]
mod parameter;
#[path = "model/provenance.rs"]
pub mod provenance;
#[path = "render/rate.rs"]
mod rate;
#[path = "model/recipe.rs"]
mod recipe;
#[path = "render/signal.rs"]
mod signal;
#[path = "render/synthesis.rs"]
mod synthesis;
#[path = "analysis/timing.rs"]
pub mod timing;
#[path = "render/voice.rs"]
mod voice;
#[path = "render/waveform.rs"]
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
    CarrierKind, ComponentId, Lane, ModulationKind, ParameterDescriptor, ParameterId, Unit, VoiceId,
};
pub use provenance::{
    BibliographySource, ConditionRole, Description, Evidence, EvidenceScope, Lineage, Measure,
    Population,
};
pub use rate::{SampleRate, SampleRateError};
pub use recipe::{Recipe, RecipeBuilder, RecipeContent, RecipeError};
pub use signal::Signal;
pub use voice::{Carrier, Ear, Ears, SoundError, VoiceError};
pub use waveform::Extent;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub(crate) use voice::Voice;
