//! Terminal usage and reporting wording.
/// The interface operation that failed; backend error detail remains attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterfaceOperation {
    StatusRenderer,
    StructuredFailure,
}
impl InterfaceOperation {
    /// Error explanation shared by native windows and terminal renderers.
    #[must_use]
    pub(crate) fn failure(self, error: impl std::fmt::Display) -> String {
        match self {
            Self::StatusRenderer => "status renderer failed".into(),
            Self::StructuredFailure => format!("cannot report structured failure: {error}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandProblem {
    /// ID output cannot represent grouped rows.
    GroupedIds,
    /// A temporary entry has no reusable persistent selector.
    TemporarySelector,
    /// Raw recipe output requires a persistent selector or file input.
    TemporaryRecipeSelector,
    /// Temporary inputs require the inclusive collection.
    RestrictedTemporaryInput,
    /// ID output cannot represent temporary entries.
    TemporaryIds,
    /// Only known nominal coverage values are accepted.
    AnalysisCoverage,
    /// A selected trashed entry cannot be played.
    TrashedPlayback,
    /// Purging requires explicit confirmation and an observed version.
    PurgeConfirmation,
    /// The root must be explicit rather than silently falling back.
    RelativeLibraryRoot,
    /// A catalog adapter returned an entry with the wrong capability.
    CatalogOrigin,
}
impl CommandProblem {
    /// User-facing explanation for the rejected request.
    #[must_use]
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::GroupedIds => "--format ids cannot be combined with grouping",
            Self::TemporarySelector => {
                "temporary selectors are not reusable; use --file or --stdin"
            }
            Self::TemporaryRecipeSelector => "temporary selectors are not reusable; use --file",
            Self::RestrictedTemporaryInput => {
                "temporary --file inputs cannot be combined with a restrictive --collection"
            }
            Self::TemporaryIds => "--format ids cannot emit temporary inputs; omit --file",
            Self::AnalysisCoverage => "expected complete or invalid",
            Self::TrashedPlayback => {
                "trashed recipes can be inspected/exported; restore before playback"
            }
            Self::PurgeConfirmation => "permanent deletion requires --yes",
            Self::RelativeLibraryRoot => "--library-dir must be absolute; no fallback is used",
            Self::CatalogOrigin => "catalog returned a non-builtin origin",
        }
    }
}

/// A successfully completed operation whose notification could not be printed.
#[must_use]
pub(crate) fn completion_reporting_failed(error: impl std::fmt::Display) -> String {
    format!("warning: operation completed; cannot report completion: {error}")
}

pub(crate) enum Caption {
    StopKeyboardHint,
    PasteRecipe,
    StandardInput,
    ConfiguredRoot,
    MonitorAlreadyInUse,
}
impl Caption {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::StopKeyboardHint => "Press Ctrl-C to stop.",
            Self::PasteRecipe => {
                "Paste a UTF-8 TOML recipe, then end input with Ctrl-D (Unix) or Ctrl-Z followed by Enter (Windows)."
            }
            Self::StandardInput => "standard input",
            Self::ConfiguredRoot => "configured root",
            Self::MonitorAlreadyInUse => "Output monitoring is already in use.",
        }
    }
}
