//! Desktop chrome, owned by the native frontend and its build-time interface.
macro_rules! chrome {
    ($($category:ident { $($variant:ident => ($property:literal, $label:expr)),* $(,)? })*) => {
        $(#[derive(Clone, Copy)] pub(crate) enum $category { $($variant,)* }
        impl $category { pub(crate) const fn label(self) -> &'static str { match self { $(Self::$variant => $label,)* } } })*
        pub(crate) const UI_TERMS: &[(&str, &str)] = &[$($(($property, $category::$variant.label()),)*)*];
    }
}
chrome! {
    Action {
        ShowMonitor => ("action-show-monitor", "Show monitor"),
        HideMonitor => ("action-hide-monitor", "Hide monitor"),
        Quit => ("action-quit", "Quit"),
        Licenses => ("action-licenses", "Licenses"),
        PurgeConfirmation => ("action-purge-confirmation", "Permanently delete saved recipe?"),
        StopInFilePanel => ("action-stop-in-file-panel", "Stop audio · file dialog stays open"),
        Listen => ("action-listen", "Listen"),
        Save => ("action-save", "Save"),
        Discard => ("action-discard", "Discard"),
        Cancel => ("action-cancel", "Cancel"),
        PermanentlyDelete => ("action-permanently-delete", "Permanently delete"),
        Edit => ("action-edit", "Edit"),
        Copy => ("action-copy", "Copy"),
        SelectAll => ("action-select-all", "Select All"),
        ThirdPartyLicensesDialog => ("action-third-party-licenses-dialog", "Third-party licenses…"),
        Hide => ("action-hide", "Hide"),
        HideOutputMonitor => ("action-hide-output-monitor", "Hide output monitor"),
        ReturnToLive => ("action-return-to-live", "Return to live"),
        HoldDisplay => ("action-hold-display", "Hold display"),
        FollowSystem => ("action-follow-system", "Follow system"),
        FollowSystemMotionPreference => ("action-follow-system-motion-preference", "Follow system motion preference"),
        RefreshDiagnosticsSnapshot => ("action-refresh-diagnostics-snapshot", "Refresh diagnostics snapshot"),
        Close => ("action-close", "Close"),
        CloseFilters => ("action-close-filters", "Close filters"),
        QueryDetails => ("action-query-details", "Query details…"),
        AddComponent => ("action-add-component", "Add component…"),
        NewSound => ("action-new-sound", "New Sound"),
        OpenRecipeDialog => ("action-open-recipe-dialog", "Open Recipe…"),
        SaveSoundDialog => ("action-save-sound-dialog", "Save sound…"),
        SaveIndependentCopy => ("action-save-independent-copy", "Save independent copy"),
        ExportPortableRecipeDialog => ("action-export-portable-recipe-dialog", "Export portable recipe…"),
        ExportWavDialog => ("action-export-wav-dialog", "Export WAV…"),
        CancelExport => ("action-cancel-export", "Cancel export"),
        SaveSound => ("action-save-sound", "Save sound"),
        ShowOutputMonitor => ("action-show-output-monitor", "Show output monitor"),
        ClearRhythmFilters => ("action-clear-rhythm-filters", "Clear rhythm filters"),
        ClearSearch => ("action-clear-search", "Clear search"),
        Refresh => ("action-refresh", "Refresh"),
        ClearAllFilters => ("action-clear-all-filters", "Clear all filters"),
        BackToRecipes => ("action-back-to-recipes", "Back to recipes"),
        ShowSaved => ("action-show-saved", "Show saved"),
        ResetSort => ("action-reset-sort", "Reset sort"),
        ApplyFilters => ("action-apply-filters", "Apply filters"),
        InitializeConfiguredLibrary => ("action-initialize-configured-library", "Initialize configured library"),
        PreviewWaveform => ("action-preview-waveform", "Preview waveform"),
        EditACopy => ("action-edit-acopy", "Edit a copy"),
        EditSavedUserRecipe => ("action-edit-saved-user-recipe", "Edit saved User recipe"),
        CloseOpenFile => ("action-close-open-file", "Close open file"),
        EditExternalFile => ("action-edit-external-file", "Edit external file"),
        AddToUserLibrary => ("action-add-to-user-library", "Add to User Library"),
        RenameSavedRecipe => ("action-rename-saved-recipe", "Rename saved recipe"),
        Rename => ("action-rename", "rename"),
        RestoreFromTrash => ("action-restore-from-trash", "Restore from Trash"),
        MoveSavedEntryToTrash => ("action-move-saved-entry-to-trash", "Move saved entry to Trash"),
        Restore => ("action-restore", "restore"),
        Trash => ("action-trash", "trash"),
        PermanentlyDeleteDialog => ("action-permanently-delete-dialog", "Permanently delete…"),
        Purge => ("action-purge", "purge"),
        OpenStudySource => ("action-open-study-source", "Open study source"),
        ReadSelectableResearchAndRecipeDetailsDialog => ("action-read-selectable-research-and-recipe-details-dialog", "Read selectable research and recipe details…"),
        ExportSettings => ("action-export-settings", "Export settings"),
        New => ("action-new", "New"),
        AddBinauralPair => ("action-add-binaural-pair", "Add binaural pair"),
        AddMonauralPair => ("action-add-monaural-pair", "Add monaural pair"),
        UndoRemove => ("action-undo-remove", "Undo remove"),
        Remove => ("action-remove", "Remove"),
        RemovePrefix => ("action-remove-prefix", "Remove "),
        ExportPortableDraftRecipeDialog => ("action-export-portable-draft-recipe-dialog", "Export portable draft recipe…"),
        ShowCopiedFromSnapshotInLibrary => ("action-show-copied-from-snapshot-in-library", "Show copied-from snapshot in Library"),
        SavedAudioDurationFadesAndExportRate => ("action-saved-audio-duration-fades-and-export-rate", "Saved audio · duration, fades and export rate"),
        SavedDuration => ("action-saved-duration", "Saved duration"),
        SavedFade => ("action-saved-fade", "Saved fade"),
    }
    Collection {
        User => ("collection-user", "User"),
        Library => ("collection-library", "Library"),
        AllBuiltInUser => ("collection-all-built-in-user", "All · Built-in + User"),
        BuiltInReadOnly => ("collection-built-in-read-only", "Built-in · Read only"),
        OpenFilesTemporary => ("collection-open-files-temporary", "Open files · Temporary"),
        Trash => ("collection-trash", "Trash"),
    }
    Facet {
        AddedHereThroughInclusiveLocalCalendarDate => ("facet-added-here-through-inclusive-local-calendar-date", "Added here through inclusive local calendar date"),
        AddedHereThroughYyyyMmDd => ("facet-added-here-through-yyyy-mm-dd", "Added here through YYYY-MM-DD"),
        AddedHereFromLocalCalendarDate => ("facet-added-here-from-local-calendar-date", "Added here from local calendar date"),
        AddedHereFromYyyyMmDd => ("facet-added-here-from-yyyy-mm-dd", "Added here from YYYY-MM-DD"),
        AddedHere => ("facet-added-here", "Added here"),
        ExportSampleRate => ("facet-export-sample-rate", "Export sample rate"),
        SortLibrary => ("facet-sort-library", "Sort library"),
        FiltersRecoveryDialog => ("facet-filters-recovery-dialog", "Filters / recovery…"),
        AppliedLibraryFiltersPrefix => ("facet-applied-library-filters-prefix", "Applied library filters · "),
        AppliedLibraryFilters => ("facet-applied-library-filters", "Applied library filters"),
        MoreFiltersAndGrouping => ("facet-more-filters-and-grouping", "More filters and grouping"),
        StudyEvidenceFilter => ("facet-study-evidence-filter", "Study evidence filter"),
        AnyPopulation => ("facet-any-population", "Any population"),
        PopulationEvidenceFilter => ("facet-population-evidence-filter", "Population evidence filter"),
        AnyConditionRole => ("facet-any-condition-role", "Any condition role"),
        ConditionRoleEvidenceFilter => ("facet-condition-role-evidence-filter", "Condition role evidence filter"),
        AnyAnalysisCoverage => ("facet-any-analysis-coverage", "Any analysis coverage"),
        NominalAnalysisCoverage => ("facet-nominal-analysis-coverage", "Nominal analysis coverage"),
        SortDirection => ("facet-sort-direction", "Sort direction"),
        NoGrouping => ("facet-no-grouping", "No grouping"),
        LibraryResultGrouping => ("facet-library-result-grouping", "Library result grouping"),
        CreatedFromLocalCalendarDate => ("facet-created-from-local-calendar-date", "Created from local calendar date"),
        CreatedThroughInclusiveLocalCalendarDate => ("facet-created-through-inclusive-local-calendar-date", "Created through inclusive local calendar date"),
        ModifiedFromLocalCalendarDate => ("facet-modified-from-local-calendar-date", "Modified from local calendar date"),
        ModifiedThroughInclusiveLocalCalendarDate => ("facet-modified-through-inclusive-local-calendar-date", "Modified through inclusive local calendar date"),
        ExactUtcDates => ("facet-exact-utc-dates", "Exact UTC dates"),
        ExactUtcRecipeAndMembershipDates => ("facet-exact-utc-recipe-and-membership-dates", "Exact UTC recipe and membership dates"),
    }
    Status {
        StopInFilePanelRequested => ("status-stop-in-file-panel-requested", "Stop requested · file dialog stays open"),
        FilePanelAudioStopped => ("status-file-panel-audio-stopped", "Audio already stopped · file dialog stays open"),
        UnsavedSound => ("status-unsaved-sound", "Unsaved sound"),
        EmptyLibrary => ("status-empty-library", "No recipes in this view. Open a recipe or change the applied filters or collection."),
        Partial => ("status-partial", "Partial"),
        UnsavedChanges => ("status-unsaved-changes", "Unsaved changes"),
        UnmodifiedDraft => ("status-unmodified-draft", "Unmodified draft"),
    }
    Caption {
        AudioControls => ("caption-audio-controls", "Audio controls"),
        LibraryFilters => ("caption-library-filters", "Filters"),
        LibraryOptions => ("caption-library-options", "Library options…"),
        PlaybackTransport => ("caption-playback-transport", "Playback transport"),
        CommittedLibraryQuery => ("caption-committed-library-query", "Committed library query"),
        WorkspaceNavigation => ("caption-workspace-navigation", "Workspace navigation"),
        SourceDatesDetails => ("caption-source-dates-details", "Source, dates and technical details"),
        ResearchSummary => ("caption-research-summary", "Research summary and limitations"),
        Collections => ("caption-collections", "Collections"),
        BrowseRhythm => ("caption-browse-rhythm", "Browse by rhythm"),
        Classification => ("caption-classification", "Classification"),
        DatesIdentity => ("caption-dates-identity", "Dates and identity"),
        SoundFacts => ("caption-sound-facts", "Sound"),
        NativeFilePanelUnavailable => ("caption-native-file-panel-unavailable", "Cannot display a native file panel."),
        NativeWindowUnavailable => ("caption-native-window-unavailable", "The document has no native window"),
        NativeFilePathUnavailable => ("caption-native-file-path-unavailable", "The native panel returned no local file path"),
        ExecutableDirectoryUnavailable => ("caption-executable-directory-unavailable", "Executable directory is missing"),
        BundleResourcesUnavailable => ("caption-bundle-resources-unavailable", "Bundle resources are missing"),
        WavAudio => ("caption-wav-audio", "WAV audio"),
        TomlSound => ("caption-toml-sound", "TOML sound"),
        AudioSnapshot => ("caption-audio-snapshot", "Audio details · snapshot"),
        NoticesPathError => ("caption-notices-path-error", "Cannot open a non-UTF8 notices path."),
        CalendarRangeError => ("caption-calendar-range-error", "Invalid calendar range. Use ordered YYYY-MM-DD dates with a unique local midnight; prior filters retained."),
        NewTitle => ("caption-new-title", "New title"),
        BundledNoticesHelp => ("caption-bundled-notices-help", "Open bundled component notices and license files."),
        Copyright => ("caption-copyright", auditorylab_app::presentation::build_info::BUILD_IDENTITY.copyright),
        AboutProduct => ("caption-about-product", auditorylab_app::presentation::build_info::ABOUT_TITLE),
        ApplicationIcon => ("caption-application-icon", "Application icon"),
        ProductName => ("caption-product-name", auditorylab_app::presentation::build_info::BUILD_IDENTITY.name),
        ProductDescription => ("caption-product-description", "Play and export auditory stimuli.\nEdit sounds as TOML recipes."),
        ProjectWebsite => ("caption-project-website", "Project website"),
        SoftwareAndArtworkLicenses => ("caption-software-and-artwork-licenses", "Software and Artwork Licenses"),
        LicenseTermsHelp => ("caption-license-terms-help", "Complete software and owner artwork license terms"),
        SamplesCommonSymmetricScale => ("caption-samples-common-symmetric-scale", " samples, common symmetric scale ±"),
        OutputMonitor => ("caption-output-monitor", "Output monitor"),
        DetailsDialog => ("caption-details-dialog", "Details…"),
        FullAudioDiagnosticsAndCaptureSource => ("caption-full-audio-diagnostics-and-capture-source", "Full audio diagnostics and capture source"),
        Left => ("caption-left", "Left"),
        Right => ("caption-right", "Right"),
        CapturedEnd => ("caption-captured-end", "Captured end"),
        WaveformTimeWindow => ("caption-waveform-time-window", "Waveform time window"),
        WaveformAmplitudeScale => ("caption-waveform-amplitude-scale", "Waveform amplitude scale"),
        Expanded => ("caption-expanded", " · expanded"),
        Collapsed => ("caption-collapsed", " · collapsed"),
        Slider => ("caption-slider", " slider"),
        File => ("caption-file", "File"),
        Help => ("caption-help", "Help"),
        WavExportProgress => ("caption-wav-export-progress", "WAV export progress"),
        TaskDetailsDialog => ("caption-task-details-dialog", "Task details…"),
        LibraryCollection => ("caption-library-collection", "Library collection"),
        AutomaticOrder => ("caption-automatic-order", "Automatic order"),
        NominalRate => ("caption-nominal-rate", "Nominal rate"),
        Title => ("caption-title", "Title"),
        Created => ("caption-created", "Created"),
        Modified => ("caption-modified", "Modified"),
        Study => ("caption-study", "Study"),
        Collection => ("caption-collection", "Collection"),
        TitleIdSoundOrStudyTopic => ("caption-title-id-sound-or-study-topic", "Title, ID, sound or study topic"),
        FindARecipe => ("caption-find-arecipe", "Find a recipe"),
        NominalRhythmReferenceBand => ("caption-nominal-rhythm-reference-band", "Nominal rhythm reference band"),
        StructuralMethod => ("caption-structural-method", "Structural method"),
        LibraryResults => ("caption-library-results", "Library results"),
        LineBreak => ("caption-line-break", "\n"),
        CreateSound => ("caption-create-sound", "Create sound"),
        LibraryPaneWidth => ("caption-library-pane-width", "Library pane width"),
        StudyDoiOrKeySource => ("caption-study-doi-or-key-source", "Study DOI or key:source"),
        AutomaticDirection => ("caption-automatic-direction", "Automatic direction"),
        Ascending => ("caption-ascending", "Ascending"),
        Descending => ("caption-descending", "Descending"),
        BandGroups => ("caption-band-groups", "Band groups"),
        MethodGroups => ("caption-method-groups", "Method groups"),
        StudyGroups => ("caption-study-groups", "Study groups"),
        CollectionGroups => ("caption-collection-groups", "Collection groups"),
        CreatedFromYyyyMmDd => ("caption-created-from-yyyy-mm-dd", "Created from YYYY-MM-DD"),
        CreatedThroughYyyyMmDd => ("caption-created-through-yyyy-mm-dd", "Created through YYYY-MM-DD"),
        ModifiedFromYyyyMmDd => ("caption-modified-from-yyyy-mm-dd", "Modified from YYYY-MM-DD"),
        ModifiedThroughYyyyMmDd => ("caption-modified-through-yyyy-mm-dd", "Modified through YYYY-MM-DD"),
        LibraryStorageAndRecovery => ("caption-library-storage-and-recovery", "Library storage and recovery"),
        ResearchCitationAndSoundAdaptations => ("caption-research-citation-and-sound-adaptations", "Research, citation and sound adaptations"),
        PatternExplainedStaticCoreModel => ("caption-pattern-explained-static-core-model", "Pattern explained · static core model"),
        StaticModel => ("caption-static-model", " static model"),
        Workbench => ("caption-workbench", "Workbench"),
        SoundTitle => ("caption-sound-title", "Sound title"),
        StaticModelOriginZero => ("caption-static-model-origin-zero", " static model, origin zero"),
        RoutingModulationAndComponentOptions => ("caption-routing-modulation-and-component-options", "Routing, modulation and component options"),
        Ears => ("caption-ears", "Ears"),
        EarsSuffix => ("caption-ears-suffix", ": ears"),
        Carrier => ("caption-carrier", "Carrier"),
        CarrierSuffix => ("caption-carrier-suffix", ": carrier"),
        Modulation => ("caption-modulation", "Modulation"),
        ModulationSuffix => ("caption-modulation-suffix", ": modulation"),
        SoundSettingsNotesAndSavedExportedAudio => ("caption-sound-settings-notes-and-saved-exported-audio", "Sound settings · notes and saved/exported audio"),
        SoundNotes => ("caption-sound-notes", "Sound notes"),
        YourNotesAboutThisSound => ("caption-your-notes-about-this-sound", "Your notes about this sound"),
        CustomRecipeNotes => ("caption-custom-recipe-notes", "Custom recipe notes"),
        SavedAudioHelp => ("caption-saved-audio-help", "Live playback continues until Stop. These settings affect saved recipes and WAV export."),
        OutputMonitorHeight => ("caption-output-monitor-height", "Output monitor height"),
        NextPlayPrefix => ("caption-next-play-prefix", " · Next Play: "),
        AudioDetails => ("caption-audio-details", "Audio details"),
    }
}

impl Caption {
    /// Title of a recipe details window.
    #[must_use]
    pub(crate) fn sound_details(title: &str) -> String {
        format!("Sound details · {title}")
    }
    /// Caption of a typography diagnostic.
    #[must_use]
    pub(crate) fn typography(family: &str, size: f32, source: &str) -> String {
        format!(
            "Typography: {}",
            Self::typography_values(family, size, source)
        )
    }
}
impl Caption {
    /// Prefix used for a graphical application error dialog.
    #[must_use]
    pub(crate) fn application_error_title() -> String {
        format!(
            "{} error",
            auditorylab_app::presentation::build_info::BUILD_IDENTITY.name
        )
    }
    /// A monitor's native typography values, without duplicating the diagnostic heading.
    #[must_use]
    pub(crate) fn typography_values(family: &str, size: f32, source: &str) -> String {
        format!("{family} · {size} logical px · {source}")
    }
    /// Explain an incomplete desktop package without depending on the process cwd.
    #[must_use]
    pub(crate) fn missing_notices(path: &std::path::Path) -> String {
        format!(
            "Bundled notices are missing or empty: {}. Obtain a complete desktop download from {}",
            path.display(),
            auditorylab_app::presentation::build_info::SOURCE_URL
        )
    }
}
impl Caption {
    /// Default portable filename for a file panel whose destination is not yet chosen.
    #[must_use]
    pub(crate) fn suggested_filename(extension: &str) -> String {
        format!("sound.{extension}")
    }
}
/// The interface operation that failed; backend error detail remains attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterfaceOperation {
    /// Show the About window.
    About,
    /// Show bundled licenses.
    Licenses,
    /// Complete a native Quit request.
    Quit,
    /// Shut down application jobs.
    Shutdown,
    /// Show a details window.
    Details,
    /// Open a web or installed notices link.
    OpenLink,
    /// Show the output monitor.
    ShowMonitor,
    /// Hide the output monitor.
    HideMonitor,
    /// Present audio controls beside a native dialog.
    AudioControls,
    /// Continue the graphical application after an error.
    Application,
}
impl InterfaceOperation {
    /// Error explanation shared by native windows and terminal renderers.
    #[must_use]
    pub(crate) fn failure(self, error: impl std::fmt::Display) -> String {
        match self {
            Self::About => format!("About: {error}"),
            Self::Licenses => format!("Licenses: {error}"),
            Self::Quit => format!("Quit: {error}"),
            Self::Shutdown => format!("Shutdown: {error}"),
            Self::Details => format!("Details: {error}"),
            Self::OpenLink => format!("Cannot open link: {error}"),
            Self::ShowMonitor => format!("Cannot show Output monitor: {error}"),
            Self::HideMonitor => format!("Cannot hide Output monitor: {error}"),
            Self::AudioControls => format!("Cannot present Audio controls: {error}"),
            Self::Application => format!(
                "{} could not continue.\n\n{error}",
                auditorylab_app::presentation::build_info::BUILD_IDENTITY.name
            ),
        }
    }
}

pub(crate) enum CommandProblem {
    DesktopArguments,
}
impl CommandProblem {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::DesktopArguments => "Use auditorylab-gui without arguments or with --version.",
        }
    }
}
