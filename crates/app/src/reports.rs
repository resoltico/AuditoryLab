//! Typed machine-readable application reports.
use crate::{
    collection::{
        CatalogQuery, Collection, Direction, EntrySummary, EvidenceFacet, QueryResult, ResultGroup,
        SortBy,
    },
    core::{ComponentFacts, DocumentUuid, Lane, SampleRate, Timestamp, nominal},
    error::Error,
    library::{
        Initialization, Inventory, LibraryDiagnostic, LibraryKey, LibraryRoot, MutationStatus,
        RootInfo, RootSource, VersionToken, WriteOutcome,
    },
    recipe::RecipeSnapshot,
};
use serde::Serialize;
use std::path::PathBuf;

/// Version of the command-line JSON report envelope.
pub const OUTPUT_SCHEMA_VERSION: u32 = 1;

macro_rules! report_struct {
    ($(#[$attr:meta])* $name:ident { $($(#[$field_attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("Machine-readable ", stringify!($name), " fields.")]
        #[derive(Debug, Clone, Serialize)]
        $(#[$attr])*
        pub struct $name {
            $(#[doc = concat!("The ", stringify!($field), " value.")] $(#[$field_attr])* pub $field: $ty,)*
        }
    }
}
report_struct!(DescriptorReport {
    owner: u32, related_owner: Option<u32>, method: nominal::Method,
    hz: f64, band: Option<nominal::Band>, derivation: Derivation,
});
/// Attribution of a nominal rate.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Derivation {
    /// An authored schedule or explicit pair.
    Explicit,
    /// An unambiguous whole-document two-tone relationship.
    NarrowInference,
}
report_struct!(NominalReport {
    descriptors: Vec<DescriptorReport>, bands: Vec<nominal::Band>, methods: Vec<nominal::Method>,
    completeness: nominal::Completeness, multiple_rhythms: bool, outside_bands: bool,
    no_rhythm: bool, diagnostics: Vec<String>,
});
impl From<&nominal::Analysis> for NominalReport {
    fn from(value: &nominal::Analysis) -> Self {
        Self {
            descriptors: value
                .rhythms
                .iter()
                .map(|r| DescriptorReport {
                    owner: r.owner.get(),
                    related_owner: r.related.map(|id| id.get()),
                    method: r.method,
                    hz: r.hz,
                    band: r.band,
                    derivation: match r.derivation {
                        nominal::Derivation::Explicit => Derivation::Explicit,
                        nominal::Derivation::NarrowInference => Derivation::NarrowInference,
                    },
                })
                .collect(),
            bands: value.bands.clone(),
            methods: value.methods.clone(),
            completeness: value.completeness,
            multiple_rhythms: value.multiple(),
            outside_bands: value.outside_bands(),
            no_rhythm: value.no_rhythm(),
            diagnostics: value.diagnostics.clone(),
        }
    }
}
report_struct!(ComponentReport {
    owner: u32,
    lane: Lane,
    facts: ComponentFacts
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
        components: Vec<ComponentReport>,
    },
    /// The document remains inspectable, but rendering failed at this rate.
    Invalid {
        /// Frames per second checked.
        sample_rate_hz: u32,
        /// Explanation of the invalid quantities.
        diagnostics: Vec<String>,
    },
}
report_struct!(EntryReport {
    key: LibraryKey, collection: Collection, title: String, document_id: DocumentUuid,
    duration_seconds: f64, created_at: Option<Timestamp>, modified_at: Option<Timestamp>,
    added_at: Option<Timestamp>, trashed_at: Option<Timestamp>, trashed: bool, version_token: Option<VersionToken>,
    nominal: NominalReport, evidence: Vec<EvidenceFacet>, validation: Validation,
    #[serde(skip_serializing_if = "Option::is_none")] history_warnings: Option<Vec<String>>,
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
    Library(LibraryDiagnostic),
    /// A root inspection error.
    Root {
        /// Stable diagnostic class.
        code: &'static str,
        /// User-facing explanation.
        message: String,
    },
    /// A direct operation diagnostic.
    Message(String),
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
    Initialized(Initialization),
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
    output_schema_version: u32, operation: String, complete: bool, convention: &'static str,
    query: Option<CatalogQuery>, unique_count: usize, entries: Vec<EntryReport>, groups: Vec<ResultGroup>,
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
        format!("Operation confirmed; completion output failed: {error}")
    }
    fn new(operation: &str, complete: bool) -> Self {
        Self {
            output_schema_version: OUTPUT_SCHEMA_VERSION,
            operation: operation.into(),
            complete,
            convention: nominal::CONVENTION,
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
    pub fn listing(
        operation: &str,
        query: &CatalogQuery,
        result: &QueryResult<'_>,
        complete: bool,
        diagnostics: &[LibraryDiagnostic],
        stages: &[String],
    ) -> Self {
        let mut report = Self::new(operation, complete).with_entries(
            result
                .entries
                .iter()
                .map(|e| EntryReport::from(*e))
                .collect(),
        );
        report.query = Some(query.clone());
        report.effective_sort = Some(query.effective_sort());
        report.classifier_revision = Some(nominal::CLASSIFIER_REVISION);
        report.groups = result.groups.clone();
        report.diagnostics = diagnostics
            .iter()
            .cloned()
            .map(Diagnostic::Library)
            .collect();
        report.stages = Some(stages.to_vec());
        report
    }
    /// Full portable content and declared-history diagnostics.
    ///
    /// # Errors
    /// Returns a recipe serialization error for invalid portable content.
    pub fn describe(summary: &EntrySummary, snapshot: &RecipeSnapshot) -> Result<Self, Error> {
        let mut entry = EntryReport::from(summary);
        entry.history_warnings = Some(crate::library::history_warnings(snapshot.recipe()));
        entry.recipe_toml = Some(snapshot.recipe().to_toml().map_err(|e| {
            crate::error::Error::caused(crate::error::ErrorKind::InvalidRecipe, e.to_string(), e)
        })?);
        Ok(Self::new("describe", true).with_entries(vec![entry]))
    }
    /// Compile the captured document at the requested rate and retain invalid-content diagnostics.
    #[must_use]
    pub fn inspect(
        summary: &EntrySummary,
        snapshot: &RecipeSnapshot,
        rate: SampleRate,
    ) -> (Self, u8) {
        let mut entry = EntryReport::from(summary);
        let mut code = 0;
        entry.validation = match snapshot.at(rate) {
            Ok(sound) => Validation::Valid {
                sample_rate_hz: rate.hz(),
                frames: sound.signal().frames(),
                peak_dbfs: sound.signal().peak_dbfs(),
                components: sound
                    .signal()
                    .target()
                    .components()
                    .map(|(id, facts)| ComponentReport {
                        owner: id.get(),
                        lane: id.lane(),
                        facts: *facts,
                    })
                    .collect(),
            },
            Err(error) => {
                code = error.kind().exit_code();
                Validation::Invalid {
                    sample_rate_hz: rate.hz(),
                    diagnostics: vec![error.to_string()],
                }
            }
        };
        (Self::new("inspect", true).with_entries(vec![entry]), code)
    }
    /// A successful root inspection.
    #[must_use]
    pub fn path(operation: &str, info: RootInfo) -> Self {
        let mut report = Self::new(operation, true);
        report.root = Some(RootReport::Inspected(info));
        report
    }
    /// A root inspection failure without claiming the directory's state is known.
    #[must_use]
    pub fn unavailable_path(
        operation: &str,
        root: &LibraryRoot,
        exists: Option<bool>,
        error: &Error,
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
            message: error.to_string(),
        });
        report
    }
    /// Explicit library initialization.
    #[must_use]
    pub fn initialized(operation: &str, initialized: &Initialization) -> Self {
        let mut report = Self::new(operation, true);
        report.status = Some(ReportStatus::Mutation(if initialized.created {
            MutationStatus::Published
        } else {
            MutationStatus::Unchanged
        }));
        report.root = Some(RootReport::Initialized(initialized.clone()));
        report.diagnostics = initialized
            .warnings
            .iter()
            .cloned()
            .map(Diagnostic::Message)
            .collect();
        report
    }
    /// Bounded inventory and storage diagnostics.
    #[must_use]
    pub fn inventory(operation: &str, inventory: &Inventory) -> Self {
        let mut report = Self::new(operation, inventory.complete)
            .with_entries(inventory.entries.iter().map(EntryReport::from).collect());
        report.diagnostics = inventory
            .diagnostics
            .iter()
            .cloned()
            .map(Diagnostic::Library)
            .collect();
        report.stages = Some(inventory.stages.clone());
        report
    }
    /// The library's authoritative publication outcome.
    #[must_use]
    pub fn mutation(operation: &str, outcome: &WriteOutcome) -> Self {
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
            .cloned()
            .map(Diagnostic::Message)
            .collect();
        report
    }
    /// A failure classified by its public exit outcome.
    #[must_use]
    pub fn error(operation: &str, error: &dyn std::error::Error, code: u8) -> Self {
        let mut report = Self::new(operation, false);
        report.status = Some(ReportStatus::Failure(match code {
            3 => FailureStatus::Conflict,
            4 => FailureStatus::Busy,
            5 => FailureStatus::IncompleteInventory,
            130 => FailureStatus::Cancelled,
            2 => FailureStatus::Rejected,
            _ => FailureStatus::Failed,
        }));
        report
            .diagnostics
            .push(Diagnostic::Message(error.to_string()));
        report
    }
}
