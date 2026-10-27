//! Typed portable-document contract failures, independent of presentation.
use crate::DocumentUuid;
use alloc::string::String;
use core::fmt;
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
impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Core's Error implementation exposes technical rule identifiers.
        // Human sentences are owned by application presentation.
        let code = match self {
            Self::EmptyTitle => "empty_title",
            Self::SourceBudget { .. } => "source_budget",
            Self::EvidenceBudget { .. } => "evidence_budget",
            Self::LineageBudget { .. } => "lineage_budget",
            Self::EmptySourceKey { .. } => "empty_source_key",
            Self::DuplicateSourceKey { .. } => "duplicate_source_key",
            Self::EmptySourceLabel { .. } => "empty_source_label",
            Self::EmptySourceCitation { .. } => "empty_source_citation",
            Self::MissingLocator { .. } => "missing_source_locator",
            Self::UrlScheme { .. } => "source_url_scheme",
            Self::InvalidUrl { .. } => "invalid_source_url",
            Self::InvalidDoi { .. } => "invalid_doi",
            Self::DanglingReference { .. } => "dangling_bibliography_reference",
            Self::EmptyLineageTitle { .. } => "empty_lineage_title",
            Self::EmptyLineageNote { .. } => "empty_lineage_note",
            Self::ReusedCopyIdentity { .. } => "reused_copy_identity",
            Self::CopyLineageFull { .. } => "copy_lineage_full",
        };
        f.write_str(code)
    }
}
impl core::error::Error for ContractError {}
