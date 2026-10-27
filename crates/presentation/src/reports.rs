//! Typed machine-readable application reports.
use crate::Explain;
use crate::*;
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::document::origin::VersionToken;
use auditorylab_core::query::Direction;
use auditorylab_core::query::SortBy;
use auditorylab_core::vocabulary::Operation;
use serde::Serialize;
use std::path::PathBuf;
use {
    auditorylab_app::discovery::collection::CatalogQuery,
    auditorylab_app::discovery::collection::Collection,
    auditorylab_app::discovery::collection::EntrySummary,
    auditorylab_app::discovery::collection::EvidenceFacet,
    auditorylab_app::discovery::collection::ResultGroup, auditorylab_app::outcome::Outcome,
    auditorylab_app::services::library::Initialization,
    auditorylab_app::services::library::Inventory,
    auditorylab_app::services::library::LibraryDiagnostic,
    auditorylab_app::services::library::MutationStatus,
    auditorylab_app::services::library::RootInfo, auditorylab_app::services::library::RootSource,
    auditorylab_app::services::library::WriteOutcome, auditorylab_core::DocumentUuid,
    auditorylab_core::Timestamp, auditorylab_core::VoiceFacts, auditorylab_core::VoiceId,
    auditorylab_core::rhythm,
};

/// Version of the command-line JSON report envelope.
pub const OUTPUT_SCHEMA_VERSION: u32 = 2;

macro_rules! report_struct {
    ($(#[$attr:meta])* $name:ident { $($(#[$field_attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("Machine-readable ", stringify!($name), " fields.")]
        #[derive(Debug, Clone, Serialize)]
        $(#[$attr])*
        pub struct $name {
            $($(#[$field_attr])* pub $field: $ty,)*
        }
    }
}
/// A stable diagnostic code and its human explanation.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticMessage {
    /// Stable machine identifier.
    pub code: String,
    /// Human explanation rendered by presentation.
    pub message: String,
}
impl From<&auditorylab_app::services::StorageWarning> for DiagnosticMessage {
    fn from(value: &auditorylab_app::services::StorageWarning) -> Self {
        Self {
            code: value.to_string(),
            message: super::publication_text::warning(value),
        }
    }
}
/// Inventory context paired with its rendered diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryDiagnosticReport {
    /// Relative storage name or configured root.
    pub storage_id: String,
    /// Stable inventory diagnostic category.
    pub code: String,
    /// Human explanation from the retained typed cause.
    pub message: String,
}
impl<E: super::Explain> From<&LibraryDiagnostic<E>> for LibraryDiagnosticReport {
    fn from(value: &LibraryDiagnostic<E>) -> Self {
        Self {
            storage_id: value.storage_id.clone(),
            code: value.code.clone(),
            message: value.message(),
        }
    }
}
report_struct!(InitializationReport { created: bool, path: PathBuf, warnings: Vec<DiagnosticMessage> });
report_struct!(GroupReport {key:String,label:String,keys:Vec<LibraryKey>});
impl From<&ResultGroup> for GroupReport {
    fn from(group: &ResultGroup) -> Self {
        Self {
            key: group.key.clone(),
            label: super::library_text::group_label(&group.label),
            keys: group.keys.clone(),
        }
    }
}
report_struct!(DescriptorReport {
    owner: VoiceId, method: rhythm::Method,
    hz: f64, band: Option<rhythm::Band>,
});
report_struct!(NominalReport {
    descriptors: Vec<DescriptorReport>, bands: Vec<rhythm::Band>, methods: Vec<rhythm::Method>,
    completeness: rhythm::Completeness, multiple_rhythms: bool, outside_bands: bool,
    no_rhythm: bool, diagnostics: Vec<DiagnosticMessage>,
});
impl From<&rhythm::Analysis> for NominalReport {
    fn from(value: &rhythm::Analysis) -> Self {
        Self {
            descriptors: value
                .rhythms
                .iter()
                .map(|r| DescriptorReport {
                    owner: r.owner,
                    method: r.method,
                    hz: r.hz,
                    band: r.band,
                })
                .collect(),
            bands: value.bands.clone(),
            methods: value.methods.clone(),
            completeness: value.completeness,
            multiple_rhythms: value.multiple(),
            outside_bands: value.outside_bands(),
            no_rhythm: value.no_rhythm(),
            diagnostics: value
                .diagnostics
                .iter()
                .map(|cause| DiagnosticMessage {
                    code: cause.to_string(),
                    message: crate::rhythm_diagnostic(cause),
                })
                .collect(),
        }
    }
}
report_struct!(VoiceReport {
    id: VoiceId,
    facts: VoiceFacts
});
/// Validation at the explicitly requested output rate.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Validation {
    /// Discovery did not compile an audio target.
    NotChecked,
    /// The entire sound can be rendered at the stated rate.
    Valid {
        /// Frames per second.
        sample_rate_hz: u32,
        /// Finite envelope length.
        frames: u64,
        /// Conservative per-ear peak bounds.
        peak_dbfs: [f64; 2],
        /// Compiled voice facts.
        voices: Vec<VoiceReport>,
    },
    /// The document remains inspectable, but rendering failed at this rate.
    Invalid {
        /// Frames per second checked.
        sample_rate_hz: u32,
        /// Explanation of the invalid quantities.
        diagnostics: Vec<DiagnosticMessage>,
    },
}
report_struct!(EntryReport {
    key: LibraryKey, collection: Collection, title: String, document_id: DocumentUuid,
    duration_seconds: f64, created_at: Option<Timestamp>, modified_at: Option<Timestamp>,
    added_at: Option<Timestamp>, trashed_at: Option<Timestamp>, trashed: bool, version_token: Option<VersionToken>,
    nominal: NominalReport, evidence: Vec<EvidenceFacet>, validation: Validation,
    #[serde(skip_serializing_if = "Option::is_none")] history_warnings: Option<Vec<DiagnosticMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")] recipe_toml: Option<String>,
});
impl From<&EntrySummary> for EntryReport {
    fn from(e: &EntrySummary) -> Self {
        Self {
            key: e.key.clone(),
            collection: e.collection(),
            title: e.title.clone(),
            document_id: e.document_id,
            duration_seconds: e.duration_seconds,
            created_at: e.created_at,
            modified_at: e.modified_at,
            added_at: e.added_at,
            trashed_at: e.trashed_at,
            trashed: e.trashed,
            version_token: e.version_token,
            nominal: (&e.nominal).into(),
            evidence: e.evidence.clone(),
            validation: Validation::NotChecked,
            history_warnings: None,
            recipe_toml: None,
        }
    }
}
/// A report diagnostic retains either storage context or a direct operation message.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Diagnostic {
    /// A bounded inventory diagnostic.
    Library(LibraryDiagnosticReport),
    /// A root inspection error.
    Root {
        /// Stable diagnostic class.
        code: &'static str,
        /// User-facing explanation.
        message: String,
    },
    /// A direct operation diagnostic.
    Message(DiagnosticMessage),
}
report_struct!(UnavailableRoot {
    path: PathBuf, source: RootSource, exists: Option<bool>, layout: Option<u32>, initialized: bool,
});
/// Root information from inspection, initialization, or an inspection failure.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RootReport {
    /// Fully inspected configuration.
    Inspected(RootInfo),
    /// Explicit initialization result.
    Initialized(InitializationReport),
    /// Path configuration whose on-disk state could not be verified.
    Unavailable(UnavailableRoot),
}
/// Stable failure statuses associated with process exit outcomes.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureStatus {
    /// The expected content token was stale.
    Conflict,
    /// Another operation owns the cooperative lock.
    Busy,
    /// Inventory results are incomplete.
    IncompleteInventory,
    /// Cancellation was requested.
    Cancelled,
    /// Invalid input or arguments.
    Rejected,
    /// A file operation failed; inspect the path named by its diagnostic.
    Failed,
}
/// Publication or failure state, independent of terminal output success.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(untagged)]
pub enum ReportStatus {
    /// Library publication state.
    Mutation(MutationStatus),
    /// Failure classification.
    Failure(FailureStatus),
}
report_struct!(Report {
    output_schema_version: u32, operation: Operation, complete: bool, convention: &'static str,
    query: Option<CatalogQuery>, unique_count: usize, entries: Vec<EntryReport>, groups: Vec<GroupReport>,
    diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")] classifier_revision: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")] effective_sort: Option<(SortBy, Direction)>,
    #[serde(skip_serializing_if = "Option::is_none")] stages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] root: Option<RootReport>,
    #[serde(skip_serializing_if = "Option::is_none")] status: Option<ReportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")] selector: Option<LibraryKey>,
    #[serde(skip_serializing_if = "Option::is_none")] version_token: Option<Option<VersionToken>>,
});
impl Report {
    /// A completed operation remains confirmed if rendering or writing its report fails.
    #[must_use]
    pub fn completion_failure(&self, error: impl std::fmt::Display) -> String {
        let truth = if self.complete {
            auditorylab_core::vocabulary::Status::OperationConfirmed
        } else {
            auditorylab_core::vocabulary::Status::OperationIncomplete
        }
        .label();
        format!("{truth}; completion output failed: {error}")
    }
    fn new(operation: Operation, complete: bool) -> Self {
        Self {
            output_schema_version: OUTPUT_SCHEMA_VERSION,
            operation,
            complete,
            convention: rhythm::CONVENTION,
            query: None,
            unique_count: 0,
            entries: Vec::new(),
            groups: Vec::new(),
            diagnostics: Vec::new(),
            classifier_revision: None,
            effective_sort: None,
            stages: None,
            root: None,
            status: None,
            selector: None,
            version_token: None,
        }
    }
    fn with_entries(mut self, entries: Vec<EntryReport>) -> Self {
        self.unique_count = entries.len();
        self.entries = entries;
        self
    }
    /// Query results with the exact typed query and storage diagnostics.
    #[must_use]
    pub fn listing(operation: Operation, listing: &auditorylab_app::services::Listing) -> Self {
        let query = listing.query();
        let mut report = Self::new(operation, listing.complete())
            .with_entries(listing.entries().iter().map(EntryReport::from).collect());
        report.query = Some(query.clone());
        report.effective_sort = Some(query.effective_sort());
        report.classifier_revision = Some(rhythm::CLASSIFIER_REVISION);
        report.groups = listing.groups().iter().map(GroupReport::from).collect();
        report.diagnostics = listing
            .diagnostics()
            .iter()
            .map(|diagnostic| Diagnostic::Library(diagnostic.into()))
            .collect();
        report.stages = Some(
            listing
                .stages()
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        );
        report
    }
    /// Full portable content and declared-history diagnostics.
    pub fn describe(description: auditorylab_app::services::Description<'_>) -> Self {
        let source = description.source();
        let text = description.portable_text();
        let history = description.history_warnings();
        let mut entry = EntryReport::from(source.summary());
        entry.history_warnings = Some(history.iter().map(DiagnosticMessage::from).collect());
        entry.recipe_toml = Some(text.to_string());
        Self::new(Operation::Describe, true).with_entries(vec![entry])
    }
    /// Render the service’s captured rate validation, including invalid-content diagnostics.
    #[must_use]
    pub fn inspect(inspection: &auditorylab_app::services::Inspection<'_>) -> Self {
        let rate = inspection.rate();
        let mut entry = EntryReport::from(inspection.source().summary());
        entry.validation = match inspection.sound() {
            Ok(sound) => Validation::Valid {
                sample_rate_hz: rate.hz(),
                frames: sound.signal().frames(),
                peak_dbfs: sound.signal().peak_dbfs(),
                voices: sound
                    .signal()
                    .target()
                    .voices()
                    .map(|(id, facts)| VoiceReport { id, facts: *facts })
                    .collect(),
            },
            Err(error) => Validation::Invalid {
                sample_rate_hz: rate.hz(),
                diagnostics: vec![DiagnosticMessage {
                    code: error.to_string(),
                    message: error.explain(),
                }],
            },
        };
        Self::new(Operation::Inspect, true).with_entries(vec![entry])
    }
    /// A successful root inspection.
    #[must_use]
    pub fn path(operation: Operation, info: RootInfo) -> Self {
        let mut report = Self::new(operation, true);
        report.root = Some(RootReport::Inspected(info));
        report
    }
    /// A root inspection failure without claiming the directory's state is known.
    #[must_use]
    pub fn unavailable_path(
        operation: Operation,
        root: &auditorylab_app::services::library::Location,
        exists: Option<bool>,
        error: &auditorylab_app::services::library::LibraryError,
    ) -> Self {
        let mut report = Self::new(operation, false);
        report.root = Some(RootReport::Unavailable(UnavailableRoot {
            path: root.path().into(),
            source: root.source(),
            exists,
            layout: None,
            initialized: false,
        }));
        report.diagnostics.push(Diagnostic::Root {
            code: "root_inspection",
            message: error.explain(),
        });
        report
    }
    /// Explicit library initialization.
    #[must_use]
    pub fn initialized(operation: Operation, initialized: &Initialization) -> Self {
        let mut report = Self::new(operation, true);
        report.status = Some(ReportStatus::Mutation(if initialized.created {
            MutationStatus::Published
        } else {
            MutationStatus::Unchanged
        }));
        report.root = Some(RootReport::Initialized(InitializationReport {
            created: initialized.created,
            path: initialized.path.clone(),
            warnings: initialized
                .warnings
                .iter()
                .map(DiagnosticMessage::from)
                .collect(),
        }));
        report.diagnostics = initialized
            .warnings
            .iter()
            .map(|warning| Diagnostic::Message(warning.into()))
            .collect();
        report
    }
    /// Bounded inventory and storage diagnostics.
    #[must_use]
    pub fn inventory(operation: Operation, inventory: &Inventory) -> Self {
        let mut report = Self::new(operation, inventory.complete)
            .with_entries(inventory.entries.iter().map(EntryReport::from).collect());
        report.diagnostics = inventory
            .diagnostics
            .iter()
            .map(|diagnostic| Diagnostic::Library(diagnostic.into()))
            .collect();
        report.stages = Some(
            inventory
                .stages
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        );
        report
    }
    /// The library's authoritative publication outcome.
    #[must_use]
    pub fn mutation(operation: Operation, outcome: &WriteOutcome) -> Self {
        let entries = outcome
            .record
            .as_ref()
            .map(|record| EntryReport::from(&record.summary()))
            .into_iter()
            .collect();
        let mut report = Self::new(operation, true).with_entries(entries);
        report.status = Some(ReportStatus::Mutation(outcome.status));
        report.selector = Some(outcome.key.clone());
        report.version_token = Some(outcome.record.as_ref().map(|record| record.version_token));
        report.diagnostics = outcome
            .diagnostics
            .iter()
            .map(|warning| Diagnostic::Message(warning.into()))
            .collect();
        report
    }
    /// A failure classified by its public exit outcome.
    #[must_use]
    pub fn error(
        operation: Operation,
        code: impl Into<String>,
        message: impl Into<String>,
        outcome: Outcome,
    ) -> Self {
        let mut report = Self::new(operation, false);
        report.status = Some(ReportStatus::Failure(match outcome {
            Outcome::Conflict | Outcome::HistoryConflict => FailureStatus::Conflict,
            Outcome::Busy | Outcome::LockUnavailable => FailureStatus::Busy,
            Outcome::IncompleteInventory => FailureStatus::IncompleteInventory,
            Outcome::Cancelled => FailureStatus::Cancelled,
            Outcome::UnknownRecipe
            | Outcome::InvalidRecipe
            | Outcome::InvalidSound
            | Outcome::InvalidArgument
            | Outcome::ReadOnlyBuiltIn => FailureStatus::Rejected,
            Outcome::Unreadable
            | Outcome::Identity
            | Outcome::Device
            | Outcome::DestinationExists
            | Outcome::Write => FailureStatus::Failed,
        }));
        report
            .diagnostics
            .push(Diagnostic::Message(DiagnosticMessage {
                code: code.into(),
                message: message.into(),
            }));
        report
    }
}

/// Serialize one report with control and bidirectional characters escaped.
///
/// # Errors
/// Returns the serializer failure before any output is written.
pub fn json_bytes(value: &Report) -> Result<Vec<u8>, serde_json::Error> {
    let text = serde_json::to_string(value)?;
    let mut output = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{2028}'|'\u{2029}')
        {
            use std::fmt::Write;
            write!(&mut output, "\\u{:04x}", u32::from(c))
                .unwrap_or_else(|_| unreachable!("writing into String cannot fail"));
        } else {
            output.push(c);
        }
    }
    Ok(output.into_bytes())
}
