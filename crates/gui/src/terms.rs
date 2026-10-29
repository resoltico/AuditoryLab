//! Desktop chrome, owned by the native frontend and its build-time interface.
macro_rules! chrome {
    ($($category:ident { $($variant:ident => ($property:literal, $label:expr)),* $(,)? })*) => {
        $(#[derive(Clone, Copy)] pub(crate) enum $category { $($variant,)* }
        impl $category { pub(crate) const fn label(self) -> &'static str { match self {
            $(Self::$variant => $label,)* } } })*
        pub(crate) const UI_TERMS: &[(&str, &str)] = &[$($(($property, $category::$variant.label()),
            )*)*];
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
        Edit => ("action-edit", "Edit"),
        Undo => ("action-undo", "Undo"),
        Cut => ("action-cut", "Cut"),
        Copy => ("action-copy", "Copy"),
        Paste => ("action-paste", "Paste"),
        DesktopGuide => ("action-desktop-guide", "Auditory Lab desktop guide"),
        SelectAll => ("action-select-all", "Select All"),
        ThirdPartyLicensesDialog => ("action-third-party-licenses-dialog", "Third-party licenses…"),
        Hide => ("action-hide", "Hide"),
        HideOutputMonitor => ("action-hide-output-monitor", "Hide output monitor"),
        ReturnToLive => ("action-return-to-live", "Return to live"),
        HoldDisplay => ("action-hold-display", "Hold display"),
        FollowSystem => ("action-follow-system", "Follow system"),
        FollowSystemMotionPreference => ("action-follow-system-motion-preference", "Follow system \
            motion preference"),
        RefreshDiagnosticsSnapshot => ("action-refresh-diagnostics-snapshot", "Refresh \
            diagnostics snapshot"),
        Close => ("action-close", "Close"),
        CloseFilters => ("action-close-filters", "Close filters"),
        ActiveFilters => ("action-active-filters", "Active filters…"),
        ShowOutputMonitor => ("action-show-output-monitor", "Show output monitor"),
        ClearRhythmFilters => ("action-clear-rhythm-filters", "Clear rhythm filters"),
        ClearSearch => ("action-clear-search", "Clear search"),
        ClearAllFilters => ("action-clear-all-filters", "Clear all filters"),
        BackToRecipes => ("action-back-to-recipes", "Back to recipes"),
        ResetSort => ("action-reset-sort", "Reset sort"),
        ApplyFilters => ("action-apply-filters", "Apply filters"),
        PreviewWaveform => ("action-preview-waveform", "Preview waveform"),
        OpenStudySource => ("action-open-study-source", "Open study source"),
        ReadSelectableResearchAndRecipeDetailsDialog => ("action-read-selectable-research-and-reci\
            pe-details-dialog", "Read \
            selectable research and recipe details…"),
        ExportSettings => ("action-export-settings", "Export settings"),
        ShowCopiedFromSnapshotInLibrary => ("action-show-copied-from-snapshot-in-library", "Show \
            copied-from snapshot in Library"),
        SavedAudioDurationFadesAndExportRate => ("action-saved-audio-duration-fades-and-export-rat\
            e", "Saved \
            audio · duration, fades and export rate"),
        SavedDuration => ("action-saved-duration", "Saved duration"),
        SavedFade => ("action-saved-fade", "Saved fade"),
    }

    Facet {
        SortLibrary => ("facet-sort-library", "Sort library"),
        FiltersRecoveryDialog => ("facet-filters-recovery-dialog", "Filters / recovery…"),
        MoreFiltersAndGrouping => ("facet-more-filters-and-grouping", "More filters and grouping"),

    }
    Status {
        StopInFilePanelRequested => ("status-stop-in-file-panel-requested", "Stop requested · \
            file dialog stays open"),
        FilePanelAudioStopped => ("status-file-panel-audio-stopped", "Audio already stopped · \
            file dialog stays open"),
        UnsavedSound => ("status-unsaved-sound", "Unsaved sound"),
        EmptyLibrary => ("status-empty-library", "No recipes in this view. Open a recipe or \
            change the applied filters or collection."),
        Partial => ("status-partial", "Partial"),
    }
    Caption {
        AudioControls => ("caption-audio-controls", "Audio controls"),
        LibraryFilters => ("caption-library-filters", "Filters"),
        LibraryOptions => ("caption-library-options", "Library options…"),
        WorkspaceNavigation => ("caption-workspace-navigation", "Workspace navigation"),
        SourceDatesDetails => ("caption-source-dates-details", "Source, dates and technical \
            details"),
        ResearchSummary => ("caption-research-summary", "Research summary and limitations"),
        BrowseRhythm => ("caption-browse-rhythm", "Browse by rhythm"),
        Classification => ("caption-classification", "Classification"),
        NativeFilePanelUnavailable => ("caption-native-file-panel-unavailable", "Cannot display a \
            native file panel."),
        NativeWindowUnavailable => ("caption-native-window-unavailable", "The document has no \
            native window"),
        NativeFilePathUnavailable => ("caption-native-file-path-unavailable", "The native panel \
            returned no local file path"),
        ExecutableDirectoryUnavailable => ("caption-executable-directory-unavailable", "Executable\
             directory is missing"),
        BundleResourcesUnavailable => ("caption-bundle-resources-unavailable", "Bundle resources \
            are missing"),
        WavAudio => ("caption-wav-audio", "WAV audio"),
        TomlSound => ("caption-toml-sound", "TOML sound"),
        AudioSnapshot => ("caption-audio-snapshot", "Audio details · snapshot"),
        NoticesPathError => ("caption-notices-path-error", "Cannot open a non-UTF8 notices path."),
        CalendarRangeError => ("caption-calendar-range-error", "Invalid calendar range. Use \
            ordered YYYY-MM-DD dates with a unique local midnight; prior filters \
            retained."),
        NewTitle => ("caption-new-title", "New title"),
        BundledNoticesHelp => ("caption-bundled-notices-help", "Open bundled component notices \
            and license files."),

        ApplicationIcon => ("caption-application-icon", "Application icon"),

        ProductDescription => ("caption-product-description", "Play and export auditory \
            stimuli.\nEdit sounds as TOML recipes."),
        ProjectWebsite => ("caption-project-website", "Project website"),
        SoftwareAndArtworkLicenses => ("caption-software-and-artwork-licenses", "Software and \
            Artwork Licenses"),
        LicenseTermsHelp => ("caption-license-terms-help", "Complete software and owner artwork \
            license terms"),
        SamplesCommonSymmetricScale => ("caption-samples-common-symmetric-scale", " samples, \
            common symmetric scale ±"),
        DetailsDialog => ("caption-details-dialog", "Details…"),
        FullAudioDiagnosticsAndCaptureSource => ("caption-full-audio-diagnostics-and-capture-sourc\
            e", "Full \
            audio diagnostics and capture source"),
        Expanded => ("caption-expanded", " · expanded"),
        Collapsed => ("caption-collapsed", " · collapsed"),
        Slider => ("caption-slider", " slider"),
        File => ("caption-file", "File"),
        Help => ("caption-help", "Help"),
        TaskDetailsDialog => ("caption-task-details-dialog", "Task details…"),
        SearchRecipes => ("caption-search-recipes", "Title, ID or what it was studied for"),
        FindByPurpose => ("caption-find-by-purpose", "To find sounds by what they were studied \
            for, search for words such as memory or relaxation."),
        FindARecipe => ("caption-find-arecipe", "Find a recipe"),
        LibraryResults => ("caption-library-results", "Library results"),
        LineBreak => ("caption-line-break", "\n"),
        CreateSound => ("caption-create-sound", "Create sound"),
        LibraryPaneWidth => ("caption-library-pane-width", "Library pane width"),
        LibraryStorageAndRecovery => ("caption-library-storage-and-recovery", "Library storage \
            and recovery"),
        ResearchCitationAndSoundAdaptations => ("caption-research-citation-and-sound-adaptations",
            "Research, \
            citation and sound adaptations"),
        WaveformPattern => ("caption-waveform-pattern", "Waveform pattern"),
        CalculatedPattern => ("caption-calculated-pattern", " calculated pattern"),
        RoutingModulationAndComponentOptions => ("caption-routing-modulation-and-component-options",
            "Routing, \
            modulation and component options"),
        EarsSuffix => ("caption-ears-suffix", ": ears"),
        CarrierSuffix => ("caption-carrier-suffix", ": carrier"),
        ModulationSuffix => ("caption-modulation-suffix", ": modulation"),
        SoundSettingsNotesAndSavedExportedAudio => ("caption-sound-settings-notes-and-saved-export\
            ed-audio", "Sound \
            settings · notes and saved/exported audio"),
        YourNotesAboutThisSound => ("caption-your-notes-about-this-sound", "Your notes about this \
            sound"),
        CustomRecipeNotes => ("caption-custom-recipe-notes", "Custom recipe notes"),
        SavedAudioHelp => ("caption-saved-audio-help", "Live playback continues until Stop. These \
            settings affect saved recipes and WAV export."),
        OutputMonitorHeight => ("caption-output-monitor-height", "Output monitor height"),
        NextPlayPrefix => ("caption-next-play-prefix", " · Next Play: "),
    }
}
