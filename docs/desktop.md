# Desktop

Open **Auditory Lab.app**, `auditorylab-gui.exe`, or `auditorylab-gui`. Start without arguments; `--version` prints identity without opening a window or device. [Source setup](../README.md#build-from-source) and [platform requirements](#platforms) cover installation.

## Browse the Library

Selecting a row shows its sound description, study context, dates and preview. **Play selected sound** starts the selected captured recipe. The persistent transport keeps naming the actual playing session while you browse. **Stop current & play recipe** deliberately replaces it.

Collections separate active **All**, read-only **Built-in**, managed **User**, temporary **Open files** and recoverable **Trash**. Wide windows show navigation, results and details together. Compact windows use list/detail navigation and retain scroll and selection when you return.

Search, Method and Sort apply immediately. Filters also offer study, population, condition role, analysis coverage, grouping and date ranges. Press Apply to commit advanced criteria; invalid ranges retain the previous query. Close, Escape and clicking outside dismiss the popup. Band counts refer to unique sounds, including sounds with several rhythms. [Query rules](formats.md#queries-and-classification) define correlation and ordering.

Desktop date bounds are inclusive local days. The timezone is shown; **Exact UTC dates** provides canonical values. Created/Modified describe the recipe, while Added here/Trashed here describe this library. Unknown dates stay visible as Unknown and sort last.

**Library options…** provides Open, Refresh, Show saved, filter resets and storage details, including when results are empty. **Show saved** reveals a confirmed saved entry and clears filters. Saving itself preserves the current query. Incomplete discovery keeps healthy and last-observed rows with a warning. Use Refresh and inspect the named files before editing a saved version.

The native per-user library can be overridden with an absolute `AUDITORYLAB_LIBRARY_DIR`. Initialize an override through Library storage controls or `auditorylab --library-dir PATH library init`. Built-ins and independent external exports remain available when persistent storage is unavailable. See [library configuration](cli.md#root-and-lifecycle) and [unsupported-file recovery](formats.md#unsupported-and-unreadable-recipes).

## Open, copy and manage sounds

**Open Recipe…** admits a regular portable file into Open files. **Close open file** removes its temporary entry. **Add to User Library** imports its content while keeping the original file unchanged.

**Edit a copy** creates a new UUID and retains the source’s notes, bibliography and bounded ancestry. Its first User Library save sets Created and Modified. Source evidence becomes ancestor evidence. The copied-from snapshot remains inspectable during the session; saved lineage retains attribution after reopening.

**Edit saved User recipe** retains its managed identity and observed content token. **Edit external file** retains its opened path, identity and bytes. Save/Discard/Cancel protects dirty content before replacement.

Rename and Trash use the key/version captured by their controls. Trashing a dirty managed edit asks how to handle its draft. Restore preserves portable history and Added here. Permanent deletion is offered in Trash and asks for confirmation naming the entry. Copies, ancestors and external originals remain independent.

## Build and edit a sound

**New Sound** opens an empty, silent Workbench. **Add component…** offers tones, clicks and explicit pairs. Each raw component uses one voice; a pair uses two. [Fields and limits](formats.md#recipe-schema) define the budgets.

Component names use their authored IDs; removing/restoring rows keeps those references stable. A pair's secondary lane is labeled as its second tone.

Enter frequency/rate, width, level and modulation values. The UI uses Hz, milliseconds, dBFS and percent. Carrier, modulation, ears and Remove are in each component’s disclosure. **Undo remove** restores the latest removed component and its staged text when capacity permits.

Typing stages text. Enter, leaving a field or a commit action applies the complete finite candidate. Unparseable or nonfinite values preserve staged text for correction. Escape restores the committed value. Notes support multiple lines; Return inserts a line break. Sliders use comfortable ranges and gesture rounding; numeric fields accept values outside those spans without clamping. Display-unit conversion uses binary64 and can round or overflow for extreme finite model values. An unedited field callback preserves the exact committed bits; explicitly edited text is parsed in the displayed unit.

Set device volume low before Play. Workbench playback follows committed settings valid at the actual device rate. Invalid edits retain the last valid playing settings. Pending text is labeled; metadata-only edits leave audio transitions unchanged. Library and Workbench playback remain distinct captured sources.

Saved duration and endpoint fades govern finite playback and WAV export. Live Workbench playback continues until Stop or its frame limit. **Draft classification** follows committed settings, and **Saved version** follows the exact confirmed save candidate.

## Save or export

**Save sound** captures the current complete draft. New drafts go to User Library, managed edits use their captured key/token, and external edits use the observed file. Later edits remain dirty after an earlier captured save completes.

**Save independent copy** creates a separate User identity. Portable recipe export preserves identity/history and does not mark a draft saved. **Export WAV…** validates at the export rate and opens the native Save dialog; stop audio first. Native Save dialogs confirm replacement.

Finite but acoustically invalid documents can be saved for repair. File errors keep the draft and identify the affected path. A content conflict requires Refresh/reload, retaining the draft, or saving an independent copy. A confirmed durability/cleanup warning remains a successful save. [Publication and recovery](formats.md#publication-and-recovery) explains these outcomes.

One audio slot and one file slot run independently. Stop targets audio; **Cancel export** targets WAV export. During native path selection, macOS offers Stop in the file panel; Windows/Linux offer modeless Audio controls. A worker slot remains busy until its thread has joined, including after displayed progress reaches completion.

New/copy/close/Quit protect unsaved content through Save/Discard/Cancel. Repeated Quit shares one decision. Failed saves and newer unsaved edits keep the application open.

## View timing and output

**Pattern explained** shows a static timing model for the selected recipe, committed draft or accepted live target. It names its source, rate and revision. **Show monitor** opens the Output monitor; **Preview waveform** shows a labeled seekable preview while idle.

The monitor displays generated stereo samples before device conversion, with peak/RMS values and coverage status. Choose a 2/10/40/100 ms window. **Fit signal** changes the common stereo axis, while **Fixed ±1** uses digital full scale. Neither changes samples or levels.

**Hold display** retains samples, source, rate, frame span, scale and meters together across navigation and sessions. **Return to live** selects current coverage. Monitor gaps, stale coverage and backend underruns remain distinct. Details opens selectable quantities; **Refresh diagnostics snapshot** updates them explicitly.

The drawer can be resized by pointer, keyboard or accessibility slider actions. When it cannot fit, a focused monitor window provides scrollable content. Closing that window returns focus to its opener. Reduce Motion defaults to a held display; Hold/Return to live overrides it, and **Follow system motion** restores the native preference.

## Platforms

macOS requires version 15 or later and the matching CPU archive. Packages are ad-hoc signed. Keep the app’s Contents intact when moving it.

Linux downloads require glibc 2.35 or later, ALSA, X11 or Wayland, fonts, fontconfig, xkbcommon, OpenGL and an xdg-desktop-portal backend. X11 also needs `libxkbcommon-x11-0` on Debian/Ubuntu. GNOME Wayland text scaling uses GLib’s `gsettings`. Keep the extracted executable and legal files together and add that directory to PATH. For a per-user launcher, install `auditorylab-gui.desktop` in `~/.local/share/applications/` and merge the packaged `hicolor/` contents into `~/.local/share/icons/hicolor/`. Keep the `org.resoltico.auditorylab` icon names unchanged.

Windows requires Windows 10 on x86_64 or Windows 11 on ARM64 and an OpenGL 2.0 or newer driver. x86_64 requires SSE3, CMPXCHG16B and LAHF/SAHF. Fatal graphics/application failures use a native dialog.

About is in the macOS application menu or the Windows/Linux Help menu. It shows product identity and links to source and licenses. Packaged dependency notices stay with the download and inside the macOS app. [Packaging](releasing.md) defines the complete delivered contents.
