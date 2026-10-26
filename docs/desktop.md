# Desktop

Start `auditorylab-gui` without arguments, or open **Auditory Lab.app** on macOS. Windows downloads contain `auditorylab-gui.exe`. `--version` prints the product version, build counter and optional source revision without opening a window or audio device. See [platform requirements](#platforms) and [source setup](../README.md#build-from-source).

## Window and navigation

Library and Workbench share a tab header. **Save sound** always addresses the Workbench draft. The header also contains **Show/Hide monitor**, file-task progress, **Cancel export** and **Task details…**. These controls and the bottom transport stay available while the document scrolls.

The transport identifies the actual playing source, generated description, estimated elapsed time and Play/Stop action. When idle, it identifies the next Play target. Browsing does not retune or rename a playing session. **Details…** opens full audio information; **Task details…** opens the current task status there. File offers **Stop current & play recipe** for explicit session replacement.

About is available from the macOS application menu or Windows/Linux Help menu. It shows version/build and Slint attribution; the source revision is available through `--version` and packaged `SOURCE.txt`. **Project website** opens the repository; **Licenses** shows software/artwork terms; **Third-party licenses…** opens the folder containing packaged notices and source information. macOS legal resources stay inside the app when it moves to Applications. Source builds embed the root license; full dependency notices require packaging. Missing packaged resources are reported in the license window. Escape closes About and license windows.

## Library

Wide windows show collection/rhythm navigation, a result list and selected-sound details. Compact windows use list/detail navigation with **Back to recipes**, preserving selection and scroll. Selection never plays audio. Result rows include nominal sound facts, duration and the relevant sort date; details disclose research, sources, dates and technical information.

| Collection | Contents |
| --- | --- |
| All | Active Built-in and User sounds. |
| Built-in | Immutable shipped recipes. |
| User | Active managed local recipes. |
| Open files | Temporary recipe inputs, cleared on restart. |
| Trash | Managed recipes available for restore or permanent deletion. |

Search requires every Unicode-normalized term; bare numeric terms match whole values. Band counts refer to unique sounds, and one sound can have several bands or rhythms. Nominal classification describes authored structure independently of playback validity. See the [reference convention](catalog.md#nominal-rhythm-references).

Search, Method and Sort apply immediately. In compact layouts, Method is in **Filters** and Sort is in **Library options…**. **More filters and grouping** provides study, population, condition role, analysis coverage, grouping and Created/Modified/Added here ranges. Advanced criteria commit through Apply; invalid ranges retain the prior query and show an error. Close, Escape or clicking outside dismisses Filters. Band/method must match one process; study/role/population must match one current evidence record. Ancestor evidence does not establish a current-study match.

Calendar bounds are inclusive local YYYY-MM-DD dates, ending at the next local midnight; the timezone is shown. Unknown dates fail bounded ranges and sort last in either direction. Automatic sorting follows the collection; explicit sorts survive collection changes. **Clear all filters** preserves collection and sort; **Reset sort** restores automatic ordering. **Query details…** shows the complete committed query as selectable text. Details show local timestamps with explicit offsets; **Exact UTC dates** shows canonical UTC values. Created/Modified belong to the portable recipe, while Added here/Trashed here belong to this library.

**Library options…** provides Open Recipe, Refresh, Show saved, sorting/filter resets and storage recovery even when results are empty. **Show saved** reveals a confirmed saved entry and clears restrictions; saving itself preserves filters. Incomplete inventories keep healthy and last-observed rows with a warning. Refresh runs explicitly and on native focus regain. **Library storage and recovery** shows the actual root, diagnostics and Recheck. A missing default library stays uncreated until first publication. An absolute `AUDITORYLAB_LIBRARY_DIR` requires explicit initialization; unavailable storage leaves Built-ins usable. See [library configuration and lifecycle](cli.md#root-and-lifecycle).

**Listen** starts finite playback after checking the selected managed version against storage and validating at the actual device rate. A changed or trashed record is refused. **Preview waveform** opens a labeled idle preview. **Export WAV…** validates captured content at the export rate and opens the native Save dialog. Study URLs open only through an explicit action.

**Open Recipe…** admits a regular TOML file into Open files. **Add to User Library** publishes its portable content without changing the external original. A reserved built-in UUID or same-ID conflict requires an independent copy. **Close open file** removes only the temporary entry. Portable recipe export preserves identity/history and excludes local membership.

**Edit a copy** creates a fresh UUID with notes, bibliography and bounded ancestry; retained research evidence becomes ancestor evidence. **Edit saved User recipe** keeps its managed identity and observed version. **Edit external file** keeps its path and observed file identity/content. Dirty drafts receive Save/Discard/Cancel protection before replacement. A copied-from snapshot is inspectable during the session; saved lineage does not reconstruct an ancestor waveform.

User recipes support Rename and Trash with captured identities/versions. Trashing a dirty managed edit receives Save/Discard/Cancel protection; a failed operation retains the draft. Restore preserves portable history and Added here. Permanent deletion is offered only in Trash, requires confirmation naming the recipe, and rechecks its version. These operations do not cascade to copies, ancestors or external originals. Built-ins are read only. Keyboard arrows/Home/End navigate results; Enter/Space opens details without playback.

## Workbench

**New** starts an empty, silent sound. **Add component…** offers steady tones, clicks, binaural pairs and monaural pairs. The sound supports up to 16 authored components and 16 expanded voices; a pair uses two voices. New components start at −30 dBFS in both ears without modulation. Pair offsets are signed. See [recipe fields and limits](recipes.md#fields-and-identity).

Component controls expose frequency/rate, width, level and applicable modulation. Ears, carrier, modulation and Remove are in a disclosure; active effects and errors remain apparent when it is closed. **Undo remove** restores the most recently removed component and its staged text until another removal or document replacement, subject to the component/voice limits. Exact fields use Hz, milliseconds, dBFS and percent. Sliders use three significant digits for frequency/rate/width, 0.1 dB for level and whole-percent depth; compact layouts keep exact fields while hiding sliders.

Typing stages text without changing audio. Enter, leaving a field or an action commits the whole candidate. Unparseable or nonfinite values preserve all staged entries for correction. Finite entries commit together even when acoustically invalid, allowing a draft to be saved and repaired. Escape restores a field's committed value. Changing carrier/modulation discards staged values only for fields it removes. Notes use a bounded multiline editor: Return inserts a line break, leaving the field or Save commits, and Escape restores committed notes. Other updates preserve staged text, focus and caret.

Live playback follows committed values valid at the device rate; an invalid commit keeps the last valid sound. Play prepares the device before acoustic validation, independently of the export rate. Metadata edits do not restart audio transitions. **Draft classification** reflects committed settings; pending text is labeled **Last committed classification**. **Saved version** reflects the exact confirmed save candidate, which can differ from a newer draft.

Sound settings include custom notes, source attribution, saved duration/fades and export rate. Duration/fades govern finite playback and export; finite Stop fades over up to 30 ms. Workbench live playback continues until Stop or its frame limit. Level/routing/depth changes ramp over 30 ms; tone/AM rates glide, and source/modulation kind or pulse rate/width changes crossfade. Live Stop can finish an existing transition before a 30 ms fade, leaving almost 60 ms of generation plus device buffering. Live lifetime is bounded to 2^36 frames, about 16 days at 48 kHz.

**Pattern explained** shows a static timing/envelope model for the selected recipe, committed draft or accepted live settings, with source, rate and revision. It distinguishes click/gate/AM schedules and pair relationships from actual output. Projections aim for four cycles within 250 ms–4 s, bounded to 32 lanes × 512 columns; slower processes label incomplete cycles. Technical component IDs appear here.

## Actual output and transport

**Show monitor** opens the Output monitor without starting audio; **Preview waveform** also reveals it. The drawer sits above transport and can be resized by dragging, arrow keys or accessibility slider actions. When labels/controls cannot fit within its available bounds, a focused monitor window provides the content. That window scrolls, reveals keyboard focus and supports Page Up/Down and Ctrl+Home/End (Command on macOS). Closing it returns focus to its opener; hiding an unfocused monitor preserves editor focus.

The monitor shows mixed stereo samples after generator ramps, fades and clamping, before device conversion, or an explicitly labeled seekable preview. Display states **Live**, **Held**, **Stale**, **Waiting**, **Unavailable** and **Last capture** are independent of transport status. Receipt age follows changed publications; a periodic trace can remain stationary while live.

Choose a 2/10/40/100 ms window and **Fit signal** or **Fixed ±1**. Fit uses one symmetric stereo limit; silence uses ±1. Scaling changes geometry without changing samples or meters. Extrema retain brief pulses and both polarities. Peak/RMS readings describe the captured window; raw history is bounded to 100 ms.

**Hold display** retains samples, source/session, rate, frame span, observation, scale and meters together across editing, navigation and sessions. Audio and warnings continue. **Return to live** selects current coverage. Ended sessions show Last capture with the retained levels. Current stale/unavailable/incomplete coverage produces warnings; skipped publications stay in diagnostics. Monitor gaps and backend underruns are distinct. Audio details includes exact rates, revisions, counters and explicit **Refresh diagnostics snapshot** without continually replacing selected text. Output elapsed uses backend timestamps; generator observations are not a latency-aligned acoustic playhead.

Reduce Motion defaults to a held display when enabled and permits live drawing when disabled. An unavailable preference source gives a diagnosed live fallback. Hold/Return to live overrides the preference; **Follow system motion** restores it. Read-only native preference queries use AppKit on macOS, client-area animation on Windows, and GNOME GSettings or KDE's settings portal on Linux, outside audio processing.

## Saving, file tasks and quit

**Save sound** commits and captures an immutable draft. New drafts save to User Library; managed edits target their stable key/version, and external edits target the observed file. **File → Save independent copy** creates a new User identity with scoped ancestry. Portable export does not fork or mark the draft saved. The location label distinguishes Unsaved, User Library and External file.

Save accepts serializable finite-invalid sounds. Unparseable/nonfinite fields and documents over 64 KiB are rejected; playback/export validate separately at their respective rates. A later edit remains dirty after an earlier snapshot publishes, and an older document's result cannot mark a replacement saved. Save preserves live control and held captures. No-op saves preserve dates and bytes; meaningful edits record the observed modification time without inventing missing creation history.

Managed and external saves check observed versions even for no-op. A conflict keeps the draft; Refresh and Edit saved User recipe provide reload through Save/Discard/Cancel, while Save independent copy preserves a separate identity. External Save rechecks the opened identity and bytes immediately before replacement; another writer can still race after that check.

Publication stages and synchronizes a private sibling file before publishing. Ambiguous I/O results are verified before reporting; confirmed durability/cleanup warnings remain saved and advance the captured baseline. Errors retain the draft and name the path. Scans report abandoned owned stages; a locked write removes them without promoting them. Forced termination can leave `.auditorylab-UUID.partial` beside an interrupted destination.

One audio session and one file task can run independently. Stop addresses audio; **Cancel export** addresses export. WAV export requires idle audio. Import occupies the file slot and populates Open files. Native Save confirms replacement. During native file selection, macOS exposes **Stop audio** inside the panel; Windows/Linux expose modeless **Audio controls**. Ordinary transport stays available during the file job itself.

New/copy/close/Quit protect dirty content through Save/Discard/Cancel; repeated Quit shares one decision. Resolve an open dialog before closing/replacing. The app closes after audio/file tasks finish. Cancel, failed saves and newer unsaved edits keep it open; Save-and-close does not discard newer edits.

## Platforms

macOS requires version 15 or later and the matching Apple silicon or Intel download. Packages are ad-hoc signed without notarization. A blocked first launch may require **System Settings → Privacy & Security → Open Anyway** after attempting to open the app; see [Apple's instructions](https://support.apple.com/en-us/102445).

Linux downloads require glibc 2.35 or later, ALSA, X11 or Wayland, fontconfig, xkbcommon, fonts, OpenGL and an xdg-desktop-portal with a desktop backend. X11 also needs its xkbcommon library (`libxkbcommon-x11-0` on Debian/Ubuntu). GNOME Wayland text scaling uses GLib's `gsettings`. Keep the extracted folder and legal materials together and add it to PATH. Install `auditorylab-gui.desktop` under `~/.local/share/applications/` and merge `hicolor/` into `~/.local/share/icons/hicolor/`, preserving the `org.resoltico.auditorylab` icon names. The executable can also run directly from its extracted folder.

Windows requires Windows 10 or later on x86_64, Windows 11 on ARM64, and an OpenGL 2.0 or newer driver. x86_64 requires SSE3, CMPXCHG16B and LAHF/SAHF. Fatal application errors, including graphics-context creation failure, appear in a native dialog.

Typography uses platform SystemUi, AppKit system size, Windows message metrics at 96 DPI and toolkit DPI scaling on Linux. X11 uses Xft DPI without an extra text multiplier; GNOME Wayland reads its text scale on a bounded worker. Slint/OpenGL renders the UI; synthesis runs on the CPU. Turn device volume down before playback and use headphones for separate-ear sounds.
