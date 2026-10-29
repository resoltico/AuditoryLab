//! Editable recipe documents, staged input and candidate evaluation.
//!
//! An [`EditorSession`] owns one document: its committed [`Draft`], the text a
//! person has typed but not committed, the saved-content baseline and the
//! document's identity. Staging never changes committed values. Committing
//! parses the whole candidate at once: if any staged field does not parse, or
//! any numeric value of the complete candidate is not finite, nothing is
//! committed and every staged field is kept. A fully parsed finite candidate
//! is committed even when it is not acoustically valid, so it can be saved and
//! repaired. Audio sessions own accepted and generated targets; the editor
//! only supplies candidates.
use crate::document::recipe::{Origin, RecipeSnapshot};
use auditorylab_core::{
    ChannelRouting, ComponentId, Finite, ParameterId, Recipe, RecipeBuilder, SampleRate, Signal,
    Sound, SoundError, StructureError, Target, TargetCheck,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use {
    crate::document::error::EditorError as Error, crate::identity::IdentityError,
    crate::identity::IdentityScope,
};

mod bindings;
mod candidate;
mod draft;
mod session;
pub use super::input::InputProblem;
pub use bindings::{DocumentId, EditingField, FieldHandle, MetadataField, RowKey};
use candidate::AnalysisKey;
pub use candidate::{Analysis, Candidate};
pub use draft::Draft;
pub(crate) use draft::serialize;
pub use session::EditorSession;

/// Why an editing command was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusal {
    /// Width conversion cannot preserve a finite quantity in the destination domain.
    PulseConversion,
    /// A requested pulse window cannot initialize finite shaped edges.
    PulseWindow,
    /// The command addresses a document that is no longer open.
    StaleDocument,
    /// The authored component exists, but this handle belongs to an older row lifetime.
    StaleRow,
    /// The component no longer exists.
    UnknownComponent,
    /// The component's carrier or modulation does not have the quantity.
    Inapplicable,
    /// Adding one component would exceed the weighted generator-lane budget.
    BudgetExceeded {
        /// Current authored component count.
        components: usize,
        /// Current expanded voice count.
        voices: usize,
        /// Expanded voices required by the component being added or restored.
        additional_voices: usize,
    },
    /// No unused component or row-lifetime identity remains in this document.
    IdentitiesExhausted,
    /// No removed component is waiting to be restored.
    NothingRemoved,
}

/// The candidate could not be committed; every staged field was kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRefusal {
    /// Every field that prevented the commit.
    pub problems: Vec<(EditingField, InputProblem)>,
}

/// Why a candidate cannot be saved.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveRefusal {
    /// Staged input does not parse or numbers are not finite.
    Input(Vec<(EditingField, InputProblem)>),
    /// The document does not serialize within the reader's limits.
    Document(Error),
}

impl std::fmt::Display for EditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

impl std::fmt::Display for SaveRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

impl std::fmt::Display for CommitRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for CommitRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.problems
            .first()
            .map(|(_, cause)| cause as &(dyn std::error::Error + 'static))
    }
}
impl std::error::Error for EditRefusal {}
impl std::error::Error for SaveRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
            Self::Input(problems) => problems
                .first()
                .map(|(_, cause)| cause as &(dyn std::error::Error + 'static)),
        }
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    EditRefusal {
        codes {
            Self::StaleDocument => "edit.stale_document",
            Self::StaleRow => "edit.stale_row",
            Self::UnknownComponent => "edit.unknown_component",
            Self::Inapplicable => "edit.inapplicable",
            Self::PulseConversion => "edit.pulse_conversion",
            Self::PulseWindow => "edit.pulse_window",
            Self::BudgetExceeded { .. } => "edit.budget_exceeded",
            Self::IdentitiesExhausted => "edit.identities_exhausted",
            Self::NothingRemoved => "edit.nothing_removed",
        }
        delegates {}
        params(this) {
            match this {
                Self::BudgetExceeded {
                    components,
                    voices,
                    additional_voices,
                } => {
                    crate::core::diagnostic_params! {
                        "components"=>*components,
                        "voices"=>*voices,
                        "additional\
                        _voices"=>*additional_voices,
                    }
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
    SaveRefusal {
        codes {
            Self::Input(..) => "save_refusal.input",
        }
        delegates {
            Self::Document(error) => error,
        }
        params(this) {
            match this {
                Self::Input(problems) => {
                    crate::core::diagnostic_params! {"problem_count"=>problems.len()}
                }
                Self::Document(_) => crate::core::diagnostic::Params::new(),
            }
        }
    }

    CommitRefusal {
        codes {
            Self { .. } => "edit.commit",
        }
        delegates {}
        params(this) {
            crate::core::diagnostic_params! {"problem_count"=>this.problems.len()}
        }
    }
}

#[cfg(test)]
mod tests;
