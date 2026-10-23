# Desktop

Start `auditorylab-gui` without arguments, or open **Auditory Lab.app** on macOS. Choose the separate Apple silicon or Intel download for your processor; macOS 15 or later is required. Windows downloads contain `auditorylab-gui.exe`. `--version` prints product version, build counter and optional source revision without opening a device.

About is available from the macOS application menu or Windows/Linux Help menu. It contains selectable product/source identity, licenses and toolkit attribution. **Source and issue tracker** opens the repository.

## Window and navigation

Library and Workbench are the two destinations. Navigation, **Save sound…** and **Show/Hide monitor** remain outside the scrolling document, as do **Task details…**, which opens the current status in Audio details, and an export's progress and **Cancel export**. Save always addresses the Workbench draft. The compact bottom transport retains the playing source, concise generated description, estimated elapsed time and Stop while the document scrolls or the monitor is hidden. Its Play/Stop button keeps one width for all of its labels, and the elapsed clock (`m:ss` or `h:mm:ss`, with the total for finite recipes) has a fixed column, so changing state or time never moves the controls. An ordinary Stop or completion is shown there, not repeated in the document status. Idle transport identifies the next Play target; full source and generator information is available through **Details…**.

The optional **Output monitor** is a bounded lower drawer above transport, with its own title, separator and Hide control. Drag its divider, use arrow keys or its accessibility slider actions to adjust its height. Its lower bound follows the measured labels and controls; if they cannot fit within the drawer bound and available document space, the focused monitor window supplies them. Drawer choice, capture controls and document focus survive refresh/navigation. This also preserves document space at larger text sizes or small window heights. The separate focused monitor has one scroll region for content that exceeds its viewport. Tab reveals each control; Page Up/Down scroll vertically, and Ctrl+Home/End (Command+Home/End on macOS) reach the top-left or bottom-right, including warning text. When a focused monitor window disappears, focus returns to the monitor opener. Hiding an unfocused monitor preserves editor focus and pending text. Detailed diagnostics open separately, with selectable text and explicit **Refresh diagnostics snapshot**; they do not scroll inside the drawer or continually replace a selected reading.

## Library

Search titles, IDs, sound parameters and research text. Wrapped results show sound structure, research topic and duration. Clear search restores the catalog. **Open Recipe…** imports a regular TOML file; imported entries retain their origin. Reopening a path replaces that Library entry with the newly read content; a copy made earlier keeps the recipe it was copied from as its source. A failed import preserves the list and selection. An imported recipe is selected and scrolled into view. Browsing and importing do not rebind active playback. Drag the divider beside the list, or focus it and use the arrow keys, to change the list's width. Compact layouts provide **Back to recipes**.

Selection does not play audio. Details show the actual summary, duration and study context. The research disclosure offers **Open study source** when the recipe links one, and a separate reader with selectable citations, species, limitations and recipe adaptations. Claims remain attributed to their source sound.

**Listen** starts finite playback on the default device. **Stop current & play recipe** explicitly fades/drains the old session before starting its replacement. **Preview waveform** explicitly selects a seekable preview when audio is idle.

**Edit a copy** opens the sound in Workbench with blank custom notes and an edited title. Its attributed source remains available under Sound settings. Existing dirty work receives Save/Discard/Cancel protection. Editing a different sound does not silently update an older live session. Recipes above the 16-layer limit remain readable in Library and must be reduced before editing.

## Workbench

An empty sound is silence. **New**, **Add steady tone** and **Add click rhythm** are in the document. Up to 16 layers retain stable technical component IDs. New layers start at −30 dBFS in both ears without modulation.

Each layer leads with its role, physical context and directly editable frequency/rate, width, level and applicable modulation controls. Routing/source/modulation/removal use an explicit disclosure; active effects and validation errors remain apparent when it is closed. Technical IDs are in Pattern explained. Exact fields use Hz, milliseconds, dBFS and percent. Sliders use three significant digits for frequency/rate/width, 0.1 dB for level and whole-percent depth. Compact layouts retain exact fields while hiding sliders.

Typing stages text: diagnostics and action availability follow it, but the sound does not. Enter, leaving the field or an action commits the whole sound at once. If any typed entry does not parse or any value is not a finite number, nothing is applied and every typed entry stays for correction. Otherwise all entries apply together, including values the sound cannot play, so the draft can be saved and repaired. Escape restores the field's committed value. Changing a carrier or modulation discards typed entries only for the fields it removes. Pending text survives monitor, font, status and other layers' updates. A live session plays committed values that are valid at the device's rate; an invalid commit keeps the last valid sound. Before the device is opened, Play needs only complete finite values: the device's actual rate decides validity, independently of the export rate. Parser/validation messages identify affected settings, with coupled rate/width or sideband constraints naming both inputs. Aggregate peak errors describe the mix. Metadata changes do not restart audio transitions.

**Pattern explained** expands beside a layer within the document. It is a static, origin-zero core timing/envelope model of the selected recipe, the committed draft or, while it plays live, the session's accepted settings, with explicit source, rate and revision. It keeps click/gate/AM processes and interaural relationships distinct. It is independent of the actual-output drawer and live phase history. Projections aim for four cycles in 250 ms–4 s, bounded to 32 lanes × 512 columns; very slow processes identify an incomplete cycle.

Sound settings contain custom notes, attributed source context, saved duration/fades and export rate. Duration/fades affect finite playback and WAV export; live playback continues until Stop or its frame limit. The live device rate remains separate from the preview/export rate.

## Actual output and transport

The drawer displays actual mixed stereo samples after generator ramps, fades and clamping, before device conversion. It can also show a clearly labeled seekable preview. **Live**, **Held**, **Stale**, **Waiting**, **Unavailable** and **Last capture** describe the display independently of Playing/Stopping/Stopped/Failed transport. Receipt age (“received samples just now”, “12 s ago”) advances from genuinely changed publications, not redraws. A stationary periodic trace can be live; no artificial motion is added.

Choose a visibly selected 2/10/40/100 ms window and **Fit signal** (the default) or **Fixed ±1**. Fit uses one symmetric stereo limit and preserves channel balance; silence uses ±1. Scaling changes geometry, not samples or meters. Extrema retain brief pulses and both polarities. Time/amplitude references and sample peak/RMS describe the actual captured window. Raw history stays bounded to 100 ms; static models do not enlarge it.

**Hold display** retains source/session, rate, frame span, sequence, observation, scale and meters together. Audio, editing, transport and warnings continue. A held older source stays explicitly identified after browsing, replacement or Stop. **Return to live** takes the latest available coverage. Ended sessions show Last capture and historical levels. Historical skipped monitor publications remain in diagnostics; only current stale/unavailable/incomplete coverage produces a monitor warning. Backend-reported output underruns remain distinct and visible.

Native motion preferences distinguish Reduce Motion enabled, disabled and unavailable. Enabled defaults to a held display; disabled permits live drawing. Unavailable uses an explicitly diagnosed live fallback. Hold/Return to live is an explicit override; **Follow system motion** restores native preference control. macOS uses AppKit's accessibility preference, Windows queries client-area animation, and Linux uses GNOME GSettings or KDE's settings portal; unavailable preference sources retain the diagnosed fallback. Preference I/O stays outside audio processing.

Generated descriptions retain independent rhythms, steady companions, interaural/monaural pair conditions and outgoing/incoming crossfade roles. Exact rates, source/session, accepted/active/settled revisions and counters are in Audio details. Output elapsed uses backend timestamps; generator observations are not a latency-aligned acoustic playhead.

Live level/routing/depth changes ramp over 30 ms. Tone/AM rates glide continuously; source/modulation kind and pulse rate/width changes crossfade. Finite Stop fades over up to 30 ms. Live Stop can finish a current transition before its 30 ms fade, so almost 60 ms of generation plus device buffering can remain. Live lifetime is bounded to 2^36 frames, about 16 days at 48 kHz.

## Saving, file tasks and quit

**Save sound…** commits an immutable draft snapshot before opening the native panel and writes schema-2 TOML while audio continues. One audio session and at most one file operation have independent cancellation/progress/completion. A later edit remains dirty after an earlier snapshot saves. A result from an older draft cannot mark a replacement draft saved. Saving cannot detach live control or clear its monitor.

Save accepts serializable finite-invalid values for correction, reported as **Saved with validation errors**. Unparseable/nonfinite fields and recipes over 64 KiB are rejected. Play and Export validate at their respective rates. Native Save confirms replacement; library staged publication syncs and atomically publishes the captured contents. Cancellation/failure preserves the draft and reports cleanup leftovers. The atomic save step is not cancellable through Stop.

During macOS file selection an in-panel **Stop audio** button addresses only audio, leaving the panel open. Windows/Linux expose independent modeless **Audio controls** while the owner has a native file dialog. Ordinary transport remains available during the file job itself. **Cancel export** is separately named; WAV export requires idle audio to avoid competing render workloads. Import shares the single file slot and only populates Library.

New/copy/close/File Quit/native macOS Quit protect dirty content with Save/Discard/Cancel, through one shared decision; repeated Quit asks once. Resolve an already-open dialog before closing/replacing. The application closes only after audio and file tasks have stopped and finished; Cancel, a failed save or newer unsaved edits keep it open. Save-and-close will not discard newer edits. Cancellation/completion do not depend on monitoring. Forced process termination can leave a hidden `.NAME.RANDOM.partial` file beside an interrupted destination.

## Platforms

A downloaded macOS app may require **System Settings → Privacy & Security → Open Anyway** after its first launch attempt; see [Apple’s instructions](https://support.apple.com/en-us/102445).

Linux needs X11 or Wayland, fontconfig, xkbcommon, fonts, OpenGL and an xdg-desktop-portal with a desktop backend for native dialogs. X11 also needs `libxkbcommon-x11-0` on Debian/Ubuntu. Windows needs the matching architecture and an OpenGL 2.0 or newer driver. Fatal Windows application errors appear in a native dialog, including failure to create a graphics context. See [source prerequisites](../README.md#build-from-source).

Typography uses San Francisco on macOS, Windows message font/text scale (Segoe UI by default), and the configured Linux desktop family/size. GNOME uses its settings portal or native GSettings; KDE uses its QFont portal preference; other GTK desktops use XSettings. Pango/GTK/GIO support Linux parsing/fallbacks. Missing preference sources/families are diagnosed. Updates coalesce off audio/UI processing and reflow app-owned windows without submitting audio targets. Menu/context-menu routing retains ordinary backend handling.

The UI renders with Slint/OpenGL; synthesis runs on the CPU. Keep device volume low before playback and use headphones for separate-ear sounds.
