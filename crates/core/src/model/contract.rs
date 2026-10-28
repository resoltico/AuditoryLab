//! Typed portable-document contract failures, independent of presentation.
use crate::DocumentUuid;
use alloc::string::{String, ToString};
/// The metadata record owning a bibliography reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceOwner {
    /// Zero-based evidence record index.
    Evidence(usize),
    /// Zero-based lineage record index.
    Lineage(usize),
}
/// Why authored content cannot become a portable recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum ContractError {
    /// A present description has an empty title.
    EmptyTitle,
    /// Bibliography capacity exceeded.
    SourceBudget {
        /// Requested source count.
        count: usize,
    },
    /// Evidence capacity exceeded.
    EvidenceBudget {
        /// Requested evidence count.
        count: usize,
    },
    /// Flat ancestry capacity exceeded.
    LineageBudget {
        /// Requested ancestor count.
        count: usize,
    },
    /// A bibliography record has an empty key.
    EmptySourceKey {
        /// Zero-based bibliography record index.
        index: usize,
    },
    /// A bibliography key is repeated.
    DuplicateSourceKey {
        /// Repeated bibliography key.
        key: String,
    },
    /// A bibliography label is empty.
    EmptySourceLabel {
        /// Bibliography key.
        key: String,
    },
    /// A bibliography citation is empty.
    EmptySourceCitation {
        /// Bibliography key.
        key: String,
    },
    /// A source has neither a DOI nor an explicit URL.
    MissingLocator {
        /// Bibliography key.
        key: String,
    },
    /// A source URL does not use explicit HTTP or HTTPS.
    UrlScheme {
        /// Bibliography key.
        key: String,
        /// Requested URL.
        url: String,
    },
    /// A URL lacks a host or contains whitespace/control characters.
    InvalidUrl {
        /// Bibliography key.
        key: String,
        /// Requested URL.
        url: String,
    },
    /// A DOI lacks the required prefix or suffix.
    InvalidDoi {
        /// Bibliography key.
        key: String,
        /// Requested DOI.
        doi: String,
    },
    /// A metadata reference names no bibliography record.
    DanglingReference {
        /// Referring metadata record.
        owner: ReferenceOwner,
        /// Missing source key.
        key: String,
    },
    /// An ancestor title is empty.
    EmptyLineageTitle {
        /// Zero-based ancestor index.
        index: usize,
    },
    /// An ancestor relationship note is empty.
    EmptyLineageNote {
        /// Zero-based ancestor index.
        index: usize,
    },
    /// A copy would reuse the original identity.
    ReusedCopyIdentity {
        /// Reused UUID.
        id: DocumentUuid,
    },
    /// A copy has no room for its immediate ancestor.
    CopyLineageFull {
        /// Current ancestor count.
        count: usize,
    },
}
impl core::fmt::Display for ContractError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for ContractError {}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    ContractError {
        codes {
            Self::EmptyTitle => "contract.empty_title",
            Self::SourceBudget { .. } => "contract.source_budget",
            Self::EvidenceBudget { .. } => "contract.evidence_budget",
            Self::LineageBudget { .. } => "contract.lineage_budget",
            Self::EmptySourceKey { .. } => "contract.empty_source_key",
            Self::DuplicateSourceKey { .. } => "contract.duplicate_source_key",
            Self::EmptySourceLabel { .. } => "contract.empty_source_label",
            Self::EmptySourceCitation { .. } => "contract.empty_source_citation",
            Self::MissingLocator { .. } => "contract.missing_source_locator",
            Self::UrlScheme { .. } => "contract.source_url_scheme",
            Self::InvalidUrl { .. } => "contract.invalid_source_url",
            Self::InvalidDoi { .. } => "contract.invalid_doi",
            Self::DanglingReference { .. } => "contract.dangling_bibliography_reference",
            Self::EmptyLineageTitle { .. } => "contract.empty_lineage_title",
            Self::EmptyLineageNote { .. } => "contract.empty_lineage_note",
            Self::ReusedCopyIdentity { .. } => "contract.reused_copy_identity",
            Self::CopyLineageFull { .. } => "contract.copy_lineage_full",
        }
        delegates {}
        params(this) {
            match this {
                Self::EmptyTitle => crate::Params::new(),
                Self::SourceBudget { count }
                | Self::EvidenceBudget { count }
                | Self::LineageBudget { count }
                | Self::CopyLineageFull { count } => crate::diagnostic_params! {"count"=>*count},
                Self::EmptySourceKey { index }
                | Self::EmptyLineageTitle { index }
                | Self::EmptyLineageNote { index } => crate::diagnostic_params! {"index"=>*index},
                Self::DuplicateSourceKey { key }
                | Self::EmptySourceLabel { key }
                | Self::EmptySourceCitation { key }
                | Self::MissingLocator { key } => crate::diagnostic_params! {
                    "source_key"=>key.clone()},
                Self::UrlScheme { key, url } | Self::InvalidUrl { key, url } => {
                    crate::diagnostic_params! {
                        "source_key"=>key.clone(),
                        "url"=>url.clone(),
                    }
                }
                Self::InvalidDoi { key, doi } => {
                    crate::diagnostic_params! {
                        "source_key"=>key.clone(),
                        "doi"=>doi.clone(),
                    }
                }
                Self::DanglingReference { owner, key } => {
                    let (kind, index) = match owner {
                        ReferenceOwner::Evidence(index) => ("evidence", *index),
                        ReferenceOwner::Lineage(index) => ("lineage", *index),
                    };
                    crate::diagnostic_params! {
                        "record_kind"=>kind,
                        "record_index"=>index,
                        "source_key"=>key.clone(),
                    }
                }
                Self::ReusedCopyIdentity { id } => {
                    crate::diagnostic_params! {"document_id"=>id.to_string()}
                }
            }
        }
    }
}
