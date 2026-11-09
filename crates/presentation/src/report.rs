//! Typed machine-readable application reports.
use crate::Explain;
mod facts;
mod query;
use auditorylab_app::discovery::query::Direction;
use auditorylab_app::discovery::query::SortBy;
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::document::origin::VersionToken;
use auditorylab_app::vocabulary::Operation;
use auditorylab_core::{diagnostic::Code, diagnostic::Params};
use serde::Serialize;
use std::path::PathBuf;
use {
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
    auditorylab_core::Timestamp, auditorylab_core::VoiceId, auditorylab_core::rhythm,
};

/// Version of the command-line JSON report envelope.
pub const OUTPUT_SCHEMA_VERSION: u32 = 4;

macro_rules! report_struct {
    ($(#[$attr:meta])* $name:ident { $($(#[$field_attr:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("Machine-readable ", stringify!($name), " fields.")]
        #[derive(Debug, Clone, Serialize)]
        $(#[$attr])*
        struct $name {
            $($(#[$field_attr])* pub $field: $ty,)*
        }
    }
}
/// A fixed machine identifier, human explanation and captured values.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticMessage {
    /// Fixed identifier owned by the typed cause.
    pub code: &'static str,
    /// Human explanation rendered by presentation.
    pub message: String,
    /// Captured values, omitted when the rule needs no parameters.
    #[serde(skip_serializing_if = "Params::is_empty")]
    pub params: Params,
}
impl DiagnosticMessage {
    /// Render one typed cause without parsing its Display output.
    #[must_use]
    pub fn new(cause: &impl Code, message: impl Into<String>) -> Self {
        Self {
            code: cause.code(),
            message: message.into(),
            params: cause.params(),
        }
    }
}
impl From<&auditorylab_app::services::StorageWarning> for DiagnosticMessage {
    fn from(value: &auditorylab_app::services::StorageWarning) -> Self {
        Self::new(value, crate::errors::publication::warning(value))
    }
}
impl<E: Code + super::Explain> From<&LibraryDiagnostic<E>> for DiagnosticMessage {
    fn from(value: &LibraryDiagnostic<E>) -> Self {
        Self::new(value, crate::errors::library_diagnostic_message(value))
    }
}
report_struct!(InitializationReport {
    created: bool,
    path: PathBuf,
    warnings: Vec<DiagnosticMessage>,
});
/// Machine group membership with a separately rendered caption.
#[derive(Debug, Clone, Serialize)]
pub struct GroupReport {
    /// Semantic group identifier.
    pub key: String,
    /// Shared human caption.
    pub label: String,
    /// Flat entry membership.
    pub keys: Vec<LibraryKey>,
}
impl From<&ResultGroup> for GroupReport {
    fn from(group: &ResultGroup) -> Self {
        Self {
            key: group.key.clone(),
            // Display captions remain separate from independent machine keys.
            // Keep its wording owned by the same renderer as the human library views.
            label: crate::library::group_label(&group.label),
            keys: group.keys.clone(),
        }
    }
}
report_struct!(DescriptorReport {
    owner: VoiceId,
    method: rhythm::Method,
    hz: f64,
    band: Option<rhythm::Band>,
});
report_struct!(NominalReport {
    descriptors: Vec<DescriptorReport>,
    bands: Vec<rhythm::Band>,
    methods: Vec<rhythm::Method>,
    completeness: rhythm::Completeness,
    multiple_rhythms: bool,
    outside_bands: bool,
    no_rhythm: bool,
    diagnostics: Vec<DiagnosticMessage>,
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
                .map(|cause| DiagnosticMessage::new(cause, crate::rhythm_diagnostic(cause)))
                .collect(),
        }
    }
}
report_struct!(VoiceReport {
    id: VoiceId,
    facts: facts::Facts,
});
/// One component's own compilation result, independent of aggregate validity.
#[derive(Debug, Clone, Serialize)]
struct ComponentValidation {
    /// Authored component identity.
    pub id: auditorylab_core::ComponentId,
    /// Empty on success, otherwise this component's own failure.
    pub diagnostics: Vec<DiagnosticMessage>,
}
/// A checked generator lane retained even when another component failed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum VoiceValidation {
    /// This lane compiled successfully; this does not validate the whole mix.
    Valid {
        /// Lane identity.
        id: VoiceId,
        /// Independent resolved lane facts.
        facts: facts::Facts,
    },
    /// This lane failed.
    Invalid {
        /// Lane identity.
        id: VoiceId,
        /// Independent lane failure.
        diagnostic: DiagnosticMessage,
    },
}
/// Validation at the explicitly requested output rate.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Validation {
    /// Discovery did not compile an audio target.
    NotChecked,
    /// The entire sound can be rendered at the stated rate.
    Valid {
        /// Frames per second.
        sample_rate_hz: u32,
        /// Finite envelope length.
        frames: u64,
        /// Conservative per-channel peak bounds.
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
        /// Every authored component's own diagnostics, in source order.
        components: Vec<ComponentValidation>,
        /// Available lane facts and failures; pre-lowering failures live on components.
        voices: Vec<VoiceValidation>,
    },
}
#[derive(Debug, Clone, Serialize)]
struct EvidenceReport {
    source: String,
    doi: Option<String>,
    scope: auditorylab_core::provenance::EvidenceScope,
    population: auditorylab_core::provenance::Population,
    role: auditorylab_core::provenance::ConditionRole,
    measures: Vec<auditorylab_core::provenance::Measure>,
}
impl From<&EvidenceFacet> for EvidenceReport {
    fn from(value: &EvidenceFacet) -> Self {
        Self {
            source: value.source.clone(),
            doi: value.doi.clone(),
            scope: value.scope,
            population: value.population,
            role: value.role,
            measures: value.measures.clone(),
        }
    }
}
report_struct!(EntryReport {
    key: LibraryKey,
    collection: Collection,
    title: String,
    document_id: DocumentUuid,
    duration_seconds: f64,
    created_at: Option<Timestamp>,
    modified_at: Option<Timestamp>,
    added_at: Option<Timestamp>,
    trashed_at: Option<Timestamp>,
    trashed: bool,
    version_token: Option<VersionToken>,
    nominal: NominalReport,
    evidence: Vec<EvidenceReport>,
    validation: Validation,
    #[serde(skip_serializing_if = "Option::is_none")]
    history_warnings: Option<Vec<DiagnosticMessage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recipe_toml: Option<String>,
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
            evidence: e.evidence.iter().map(EvidenceReport::from).collect(),
            validation: Validation::NotChecked,
            history_warnings: None,
            recipe_toml: None,
        }
    }
}
report_struct!(UnavailableRoot {
    path: PathBuf,
    source: RootSource,
    exists: Option<bool>,
    layout: Option<u32>,
    initialized: Option<bool>,
});
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
    /// Existing input could not be read or decoded.
    Unreadable,
    /// A requested publication failed; the expected mutation was not confirmed.
    PublicationFailed,
    /// Post-mutation observation cannot establish the requested publication.
    PublicationUnconfirmed,
    /// Another operation failed; inspect its typed diagnostic.
    Failed,
}
// Wire fields are explicit: internal domain derives do not define schema 4.
#[derive(Debug, Clone, Serialize)]
struct WindowReport {
    start_seconds: f64,
    end_seconds: f64,
}
#[derive(Debug, Clone, Serialize)]
struct ToneReport {
    channel: usize,
    hz: f64,
    dbfs: f64,
    relative_db: f64,
}
#[derive(Debug, Clone, Serialize)]
struct EarDifferenceReport {
    left_hz: f64,
    right_hz: f64,
    difference_hz: f64,
    band: Option<rhythm::Band>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PulsingCauseReport {
    Beating {
        first_hz: f64,
        second_hz: f64,
        harmonic: u32,
    },
    Modulation,
}
#[derive(Debug, Clone, Serialize)]
struct PulsingReport {
    channel: usize,
    envelope_band: String,
    hz: f64,
    band: Option<rhythm::Band>,
    cause: PulsingCauseReport,
}
#[derive(Debug, Clone, Serialize)]
struct EnergyShareReport {
    channel: usize,
    below_hz: f64,
    fraction: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
struct StereoReport {
    peak_dbfs: Vec<Option<f64>>,
    rms_dbfs: Vec<Option<f64>>,
    correlation: Option<f64>,
    side_to_mid_db: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
struct SpectrumPointReport {
    hz: f64,
    dbfs_per_hz: f64,
}
#[derive(Debug, Clone, Serialize)]
struct AnalysisReport {
    frames: u64,
    source_frames: Option<u64>,
    start_frame: u64,
    end_frame: u64,
    codec: String,
    lossy: bool,
    duration_seconds: Option<f64>,
    sample_rate: u32,
    channels: usize,
    window: WindowReport,
    envelope_window: WindowReport,
    resolution_hz: f64,
    envelope_resolution_hz: f64,
    minimum_resolved_envelope_hz: f64,
    minimum_envelope_relative_power: f64,
    tones: Vec<ToneReport>,
    ear_differences: Vec<EarDifferenceReport>,
    coincident_tones: bool,
    pulsing: Vec<PulsingReport>,
    low_energy: Vec<EnergyShareReport>,
    stereo: StereoReport,
    spectrum: Vec<Vec<SpectrumPointReport>>,
    limits: Vec<String>,
}
impl From<&auditorylab_core::measurement::Pulsing> for PulsingReport {
    fn from(t: &auditorylab_core::measurement::Pulsing) -> Self {
        use auditorylab_core::measurement::PulsingCause;
        PulsingReport {
            channel: t.channel,
            envelope_band: t.envelope_band.key().into(),
            hz: t.hz,
            band: t.band,
            cause: match t.cause {
                PulsingCause::Beating {
                    first_hz,
                    second_hz,
                    harmonic,
                } => PulsingCauseReport::Beating {
                    first_hz,
                    second_hz,
                    harmonic,
                },
                PulsingCause::Modulation => PulsingCauseReport::Modulation,
            },
        }
    }
}
impl From<&auditorylab_app::recording::Analysis> for AnalysisReport {
    fn from(a: &auditorylab_app::recording::Analysis) -> Self {
        Self {
            frames: a.frames,
            source_frames: a.source_frames,
            start_frame: a.start_frame,
            end_frame: a.end_frame,
            codec: a.codec.clone(),
            lossy: a.lossy,
            duration_seconds: a.duration_seconds,
            sample_rate: a.sample_rate,
            channels: a.channels,
            window: WindowReport {
                start_seconds: a.window.start_seconds,
                end_seconds: a.window.end_seconds,
            },
            envelope_window: WindowReport {
                start_seconds: a.envelope_window.start_seconds,
                end_seconds: a.envelope_window.end_seconds,
            },
            resolution_hz: a.resolution_hz,
            envelope_resolution_hz: a.envelope_resolution_hz,
            minimum_resolved_envelope_hz: a.minimum_resolved_envelope_hz,
            minimum_envelope_relative_power: a.minimum_envelope_relative_power,
            tones: a
                .tones
                .iter()
                .map(|t| ToneReport {
                    channel: t.channel,
                    hz: t.hz,
                    dbfs: t.dbfs,
                    relative_db: t.relative_db,
                })
                .collect(),
            ear_differences: a
                .ear_differences
                .iter()
                .map(|t| EarDifferenceReport {
                    left_hz: t.left_hz,
                    right_hz: t.right_hz,
                    difference_hz: t.difference_hz,
                    band: t.band,
                })
                .collect(),
            coincident_tones: a.coincident_tones,
            pulsing: a.pulsing.iter().map(PulsingReport::from).collect(),
            low_energy: a
                .low_energy
                .iter()
                .map(|t| EnergyShareReport {
                    channel: t.channel,
                    below_hz: t.below_hz,
                    fraction: t.fraction,
                })
                .collect(),
            stereo: StereoReport {
                peak_dbfs: a.stereo.peak_dbfs.clone(),
                rms_dbfs: a.stereo.rms_dbfs.clone(),
                correlation: a.stereo.correlation,
                side_to_mid_db: a.stereo.side_to_mid_db,
            },
            spectrum: a
                .spectrum
                .iter()
                .map(|c| {
                    c.iter()
                        .map(|t| SpectrumPointReport {
                            hz: t.hz,
                            dbfs_per_hz: t.dbfs_per_hz,
                        })
                        .collect()
                })
                .collect(),
            limits: a.limits.iter().map(|value| value.key().into()).collect(),
        }
    }
}
/// Listing commands with a supported successful JSON payload.
#[derive(Debug, Clone, Copy)]
pub enum ListingOperation {
    /// Ordinary discovery.
    List,
    /// Trash discovery.
    ListTrash,
}
impl TryFrom<Operation> for ListingOperation {
    type Error = &'static str;
    fn try_from(value: Operation) -> Result<Self, Self::Error> {
        match value {
            Operation::List => Ok(Self::List),
            Operation::ListTrash => Ok(Self::ListTrash),
            _ => Err("operation does not produce a listing"),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
struct Entries {
    complete: bool,
    entries: Vec<EntryReport>,
}
#[derive(Debug, Clone, Serialize)]
struct Listing {
    complete: bool,
    convention: &'static str,
    query: query::Query,
    unique_count: usize,
    entries: Vec<EntryReport>,
    groups: Vec<GroupReport>,
    diagnostics: Vec<DiagnosticMessage>,
    classifier_revision: u32,
    effective_sort: (SortBy, Direction),
    stages: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
struct InventoryReport {
    complete: bool,
    entries: Vec<EntryReport>,
    diagnostics: Vec<DiagnosticMessage>,
    stages: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
struct Publication {
    status: MutationStatus,
    selector: LibraryKey,
    version_token: Option<VersionToken>,
    entries: Vec<EntryReport>,
    diagnostics: Vec<DiagnosticMessage>,
}
#[derive(Debug, Clone, Serialize)]
struct InitializationPayload {
    status: MutationStatus,
    root: InitializationReport,
    diagnostics: Vec<DiagnosticMessage>,
}
#[derive(Debug, Clone, Serialize)]
struct RootPayload {
    root: query::Root,
}
#[derive(Debug, Clone, Serialize)]
struct RecordingPayload {
    complete: bool,
    recording: RecordingReport,
}
#[derive(Debug, Clone, Serialize)]
struct RecordingReport {
    path: PathBuf,
    bytes: u64,
    format: auditorylab_app::recording::ContainerFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    analysis: Option<AnalysisReport>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum Success {
    List(Listing),
    ListTrash(Listing),
    Describe(Entries),
    Inspect(Entries),
    LibraryPath(RootPayload),
    LibraryInit(InitializationPayload),
    LibraryCheck(InventoryReport),
    LibraryAdd(Publication),
    LibraryUpdate(Publication),
    LibraryRename(Publication),
    LibraryTrash(Publication),
    LibraryRestore(Publication),
    LibraryPurge(Publication),
    AnalyzeRecording(RecordingPayload),
}
#[derive(Debug, Clone, Serialize)]
struct Failure {
    operation: Operation,
    status: FailureStatus,
    diagnostics: Vec<DiagnosticMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    root: Option<UnavailableRoot>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum Body {
    Success(Box<Success>),
    Failure(Failure),
}
/// An operation-specific report. Its private payload prevents contradictory combinations.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    output_schema_version: u32,
    #[serde(flatten)]
    body: Body,
}
impl Report {
    fn success(body: Success) -> Self {
        Self {
            output_schema_version: OUTPUT_SCHEMA_VERSION,
            body: Body::Success(Box::new(body)),
        }
    }
    /// Whether this report carries a confirmed publication or confirmed no-change result.
    /// Read/inspection completeness is independent and never implies publication.
    #[must_use]
    pub fn publication_confirmed(&self) -> bool {
        match &self.body {
            Body::Failure(_) => false,
            Body::Success(result) => match result.as_ref() {
                Success::LibraryInit(_)
                | Success::LibraryAdd(_)
                | Success::LibraryUpdate(_)
                | Success::LibraryRename(_)
                | Success::LibraryTrash(_)
                | Success::LibraryRestore(_)
                | Success::LibraryPurge(_) => true,
                Success::List(_)
                | Success::ListTrash(_)
                | Success::Describe(_)
                | Success::Inspect(_)
                | Success::LibraryPath(_)
                | Success::LibraryCheck(_)
                | Success::AnalyzeRecording(_) => false,
            },
        }
    }
    /// An opened recording's identity, without invented sample findings.
    #[must_use]
    pub fn recording(recording: &auditorylab_app::recording::RecordingFile) -> Self {
        Self::success(Success::AnalyzeRecording(RecordingPayload {
            complete: false,
            recording: RecordingReport {
                path: recording.path.clone(),
                bytes: recording.bytes,
                format: recording.format,
                analysis: None,
            },
        }))
    }
    /// Complete measurements and the identity of their admitted source.
    #[must_use]
    pub fn analyzed_recording(recording: &auditorylab_app::recording::AnalyzedRecording) -> Self {
        Self::success(Success::AnalyzeRecording(RecordingPayload {
            complete: true,
            recording: RecordingReport {
                path: recording.file.path.clone(),
                bytes: recording.file.bytes,
                format: recording.file.format,
                analysis: Some(AnalysisReport::from(&recording.analysis)),
            },
        }))
    }
    /// Query results retain useful entries even when the inventory is incomplete.
    #[must_use]
    pub fn listing(
        operation: ListingOperation,
        listing: &auditorylab_app::services::Listing,
    ) -> Self {
        let query = listing.query();
        let payload = Listing {
            complete: listing.complete(),
            convention: rhythm::CONVENTION,
            query: query.into(),
            unique_count: listing.entries().len(),
            entries: listing.entries().iter().map(EntryReport::from).collect(),
            groups: listing.groups().iter().map(GroupReport::from).collect(),
            diagnostics: listing
                .diagnostics()
                .iter()
                .map(DiagnosticMessage::from)
                .collect(),
            classifier_revision: rhythm::CLASSIFIER_REVISION,
            effective_sort: query.effective_sort(),
            stages: listing
                .stages()
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        };
        Self::success(match operation {
            ListingOperation::List => Success::List(payload),
            ListingOperation::ListTrash => Success::ListTrash(payload),
        })
    }
    /// Full portable content and declared-history diagnostics.
    pub fn describe(description: &auditorylab_app::services::Description<'_>) -> Self {
        let source = description.source();
        let text = description.portable_text();
        let history = description.history_warnings();
        let mut entry = EntryReport::from(source.summary());
        entry.history_warnings = Some(history.iter().map(DiagnosticMessage::from).collect());
        entry.recipe_toml = Some(text.to_string());
        Self::success(Success::Describe(Entries {
            complete: true,
            entries: vec![entry],
        }))
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
                    .map(|(id, facts)| VoiceReport {
                        id,
                        facts: (*facts).into(),
                    })
                    .collect(),
            },
            Err(error) => Validation::Invalid {
                sample_rate_hz: rate.hz(),
                diagnostics: vec![DiagnosticMessage::new(error, error.explain())],
                components: inspection
                    .component_check()
                    .into_iter()
                    .flat_map(auditorylab_core::TargetCheck::components)
                    .map(|(id, result)| ComponentValidation {
                        id: *id,
                        diagnostics: result
                            .as_ref()
                            .err()
                            .map(|e| DiagnosticMessage::new(e, e.explain()))
                            .into_iter()
                            .collect(),
                    })
                    .collect(),
                voices: inspection
                    .component_check()
                    .into_iter()
                    .flat_map(auditorylab_core::TargetCheck::voices)
                    .map(|(id, result)| match result {
                        Ok(facts) => VoiceValidation::Valid {
                            id: *id,
                            facts: (*facts).into(),
                        },
                        Err(error) => VoiceValidation::Invalid {
                            id: *id,
                            diagnostic: DiagnosticMessage::new(
                                error,
                                crate::errors::acoustic::voice_error(error),
                            ),
                        },
                    })
                    .collect(),
            },
        };
        Self::success(Success::Inspect(Entries {
            complete: true,
            entries: vec![entry],
        }))
    }
    /// A successful root inspection.
    #[must_use]
    pub fn path(info: RootInfo) -> Self {
        Self::success(Success::LibraryPath(RootPayload { root: info.into() }))
    }
    /// A root failure preserves only facts actually observed.
    #[must_use]
    pub fn unavailable_path(
        root: &auditorylab_app::services::library::Location,
        exists: Option<bool>,
        error: &auditorylab_app::services::library::LibraryError,
    ) -> Self {
        let mut report = Self::error(
            Operation::LibraryPath,
            error,
            error.explain(),
            error.outcome(),
            None,
        );
        if let Body::Failure(payload) = &mut report.body {
            payload.root = Some(UnavailableRoot {
                path: root.path().into(),
                source: root.source(),
                exists,
                layout: None,
                initialized: None,
            });
        }
        report
    }
    /// Explicit initialization with its publication notices.
    #[must_use]
    pub fn initialized(initialized: &Initialization) -> Self {
        let status = if !initialized.created {
            MutationStatus::Unchanged
        } else if initialized.warnings.is_empty() {
            MutationStatus::Published
        } else {
            MutationStatus::PublishedWithWarning
        };
        Self::success(Success::LibraryInit(InitializationPayload {
            status,
            root: InitializationReport {
                created: initialized.created,
                path: initialized.path.clone(),
                warnings: initialized
                    .warnings
                    .iter()
                    .map(DiagnosticMessage::from)
                    .collect(),
            },
            diagnostics: initialized
                .warnings
                .iter()
                .map(DiagnosticMessage::from)
                .collect(),
        }))
    }
    /// Bounded inventory with useful partial entries.
    #[must_use]
    pub fn inventory(inventory: &Inventory) -> Self {
        Self::success(Success::LibraryCheck(InventoryReport {
            complete: inventory.complete,
            entries: inventory.entries.iter().map(EntryReport::from).collect(),
            diagnostics: inventory
                .diagnostics
                .iter()
                .map(DiagnosticMessage::from)
                .collect(),
            stages: inventory
                .stages
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }))
    }
    /// Confirmed publication; null token explicitly means no current record after purge.
    #[must_use]
    pub fn mutation(outcome: &WriteOutcome) -> Self {
        let payload = Publication {
            status: outcome.status,
            selector: outcome.key.clone(),
            version_token: outcome.record.as_ref().map(|r| r.version_token),
            entries: outcome
                .record
                .as_ref()
                .map(|r| EntryReport::from(&r.summary()))
                .into_iter()
                .collect(),
            diagnostics: outcome
                .diagnostics
                .iter()
                .map(DiagnosticMessage::from)
                .collect(),
        };
        Self::success(match outcome.kind {
            auditorylab_app::services::library::MutationKind::Create
            | auditorylab_app::services::library::MutationKind::Add => Success::LibraryAdd(payload),
            auditorylab_app::services::library::MutationKind::Update => {
                Success::LibraryUpdate(payload)
            }
            auditorylab_app::services::library::MutationKind::Rename => {
                Success::LibraryRename(payload)
            }
            auditorylab_app::services::library::MutationKind::Trash => {
                Success::LibraryTrash(payload)
            }
            auditorylab_app::services::library::MutationKind::Restore => {
                Success::LibraryRestore(payload)
            }
            auditorylab_app::services::library::MutationKind::Purge => {
                Success::LibraryPurge(payload)
            }
        })
    }
    /// A failure classified by its public exit outcome.
    #[must_use]
    pub fn error(
        operation: Operation,
        error: &impl Code,
        message: impl Into<String>,
        outcome: Outcome,
        confirmation: Option<&auditorylab_app::services::Confirmation>,
    ) -> Self {
        let status = match outcome {
            Outcome::Conflict | Outcome::HistoryConflict => FailureStatus::Conflict,
            Outcome::Busy | Outcome::LockUnavailable => FailureStatus::Busy,
            Outcome::IncompleteInventory => FailureStatus::IncompleteInventory,
            Outcome::Cancelled => FailureStatus::Cancelled,
            Outcome::UnknownRecipe
            | Outcome::InvalidRecipe
            | Outcome::InvalidSound
            | Outcome::InvalidArgument
            | Outcome::ReadOnlyBuiltIn => FailureStatus::Rejected,
            Outcome::Unreadable => FailureStatus::Unreadable,
            Outcome::Write
                if matches!(
                    confirmation,
                    Some(
                        auditorylab_app::services::Confirmation::Different
                            | auditorylab_app::services::Confirmation::Unavailable(_)
                    )
                ) =>
            {
                FailureStatus::PublicationUnconfirmed
            }
            Outcome::Write
                if matches!(
                    operation,
                    Operation::LibraryInit
                        | Operation::LibraryAdd
                        | Operation::LibraryUpdate
                        | Operation::LibraryRename
                        | Operation::LibraryTrash
                        | Operation::LibraryRestore
                        | Operation::LibraryPurge
                ) =>
            {
                FailureStatus::PublicationFailed
            }
            Outcome::Write => FailureStatus::Failed,
            Outcome::Identity | Outcome::Device | Outcome::DestinationExists => {
                FailureStatus::Failed
            }
        };
        Self {
            output_schema_version: OUTPUT_SCHEMA_VERSION,
            body: Body::Failure(Failure {
                operation,
                status,
                diagnostics: vec![DiagnosticMessage::new(error, message)],
                root: None,
            }),
        }
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
        if matches!(c,
                '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
                | '\u{2028}'
                | '\u{2029}')
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn diagnostic_values_serialize_explicitly_and_empty_params_are_omitted() {
        let issue = auditorylab_core::ValueIssue {
            parameter: auditorylab_core::ParameterId::ToneFrequency,
            value: f64::INFINITY,
            rule: auditorylab_core::ValueRule::Positive,
        };
        let value =
            serde_json::to_value(DiagnosticMessage::new(&issue, "invalid frequency")).unwrap();
        assert_eq!(value["code"], "value.positive");
        assert_eq!(value["params"]["value"], "inf");
        assert_eq!(value["params"]["parameter"], "tone_frequency");
        assert_eq!(value.as_object().unwrap().len(), 3);
        let empty = serde_json::to_value(DiagnosticMessage::new(
            &auditorylab_app::document::editing::InputProblem::Unparseable,
            "invalid input",
        ))
        .unwrap();
        assert!(empty.get("params").is_none());
        let negative_zero = auditorylab_core::ValueIssue {
            value: -0.0,
            ..issue
        };
        let value =
            serde_json::to_value(DiagnosticMessage::new(&negative_zero, "invalid frequency"))
                .unwrap();
        assert_eq!(
            value["params"]["value"].as_f64().unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
    }
}
