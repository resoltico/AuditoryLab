# Desktop

Open **Auditory Lab.app**, `auditorylab-gui.exe`, or `auditorylab-gui`. Start without arguments; `--version` prints identity without opening a window or device. [Source setup](../README.md#build-from-source) and [platform requirements](#platforms) cover installation.

## Start here

**Start here** is an optional guide. Fresh profiles open it. After the first deliberate entry to work is successfully recorded, later launches start in Library. Reopen it from workspace navigation or Help. The guide retains its selected lesson, construction and band during the session. Selecting another lesson starts at its heading; selecting the current destination preserves the reading position.

| Lesson | What it provides |
| --- | --- |
| Overview | Routes to the real Library or a pulsed-tone starter in Workbench. |
| Library guide | Explains built-ins, saved sounds and research context. |
| Pitch & rhythm | Compares a carrier, its pulse-modulation gain and the resulting sound, with actual example rates and labeled time windows. |
| Sound types | Six constructions shown as choices at once, and **Open this example in Workbench**. |
| Rhythm ranges | A combined five-band spectrum, selected-band detail, observed EEG/MEG contexts and source links. |

Choose **Library** or **Workbench** to leave. Reading preserves drafts, pending text, Library selection and running tasks. Play, Save and Export require a working destination; Stop and Cancel export still address the actual running work. Guide entry focuses the selected lesson.

**Create a pulsed tone** opens the starter; **Open this example in Workbench** opens the selected construction. Both capture a complete fresh example through Save/Discard/Cancel. Successful replacement focuses its title. Cancel or failed Save retains the guide and current draft. Nothing plays or saves automatically; **New sound** starts empty and silent.

Sound diagrams use actual compiled data. Signed waveforms share one fitted scale across channels, so height does not show digital full scale or listening volume. Identical channels share one labeled trace; different channels are drawn together, left in blue and right in orange, so their difference is visible. Level plots run from silent at the bottom to the tone's full level at the top; pulse-source on/off timing is distinct. Each lesson ends with **Details and sources**, which gives the precise quantities.

Band illustrations are one-second, zero-phase normalized sine schematics, with native labels and light/dark colors. They are not recorded EEG, rendered audio or pulse envelopes. Observed EEG/MEG associations overlap and depend on region, task and measurement; matching an audio rate to a band does not establish a brain state or promise an effect. Direct references open the [IFCN glossary](https://pmc.ncbi.nlm.nih.gov/articles/PMC6123891/), [memory-study record](https://pubmed.ncbi.nlm.nih.gov/16837600/) and [NIH sleep-stage overview](https://www.nhlbi.nih.gov/health/sleep/stages-of-sleep), independently of application-manual version links. The memory study used 4.5–8.5 and 60–90 Hz, rather than the app’s exact reference intervals.

The `onboarding-seen` flag lives in the native application configuration directory, independently of User Library and `AUDITORYLAB_LIBRARY_DIR`. Closing before departure leaves the guide unseen. A malformed or unwritable preference produces a compact **Preference notice** with selectable path, cause and recovery details; Stop and Cancel remain available in that popup. Unknown bytes are never replaced automatically. Successful publication clears its notice and closes the popup. Repair the named file or directory and restart if the notice remains. Only the seen flag persists; lesson progress and workspace restoration are not stored.

## Browse the Library

Each row shows what its study was about. Selecting a row shows its sound description, study context, dates and preview. **Play selected sound** starts the selected captured recipe. The persistent transport keeps naming the actual playing session while you browse. **Stop current & play recipe** deliberately replaces it.

Collections separate active **All**, read-only **Built-in**, managed **User**, temporary **Open files** and recoverable **Trash**. Wide windows show navigation, results and details together. Compact windows use list/detail navigation and retain scroll and selection when you return.

Search, Method and Sort apply immediately. Filters also offer study, population, study role, rhythm analysis, grouping and date ranges. Search also matches what a sound was studied for, such as memory or relaxation. Press Apply to commit advanced criteria; invalid ranges retain the previous query. Close, Escape and clicking outside dismiss the popup. Band counts refer to unique sounds, including sounds with several rhythms. [Query rules](formats.md#queries-and-classification) define correlation and ordering.

Desktop date bounds are inclusive local days. The timezone is shown; the selected sound's **Source and dates** section also shows the exact canonical UTC values. Created/Modified describe the recipe, while Added here/Trashed here describe this library. Unknown dates stay visible as Unknown and sort last.

**Library options…** provides Open, Refresh, Show saved, filter resets and storage details, including when results are empty. Saving itself preserves the current query. **Show saved** selects the confirmed entry in User or Trash and restores an unrestricted query with automatic ordering and no grouping. Incomplete discovery keeps healthy and last-observed rows with a warning. Use Refresh and inspect the named files before editing a saved version.

Filter resets have distinct scopes:

| Action | Result |
| --- | --- |
| Clear rhythm filters | Clears band, method and special rhythm restrictions; retains every other query setting. |
| Clear all filters | Clears search, rhythm, evidence and date restrictions; retains collection, sort, direction and grouping. |
| Reset sort | Restores automatic sort/direction; retains filters, collection and grouping. |

Clearing rhythm filters is available with the compact/list filter controls. Other resets are available through Library options or the filter popup. Unapplied advanced input survives unrelated selection and display updates; an explicit full reset or reveal discards it.

The native per-user library can be overridden with an absolute `AUDITORYLAB_LIBRARY_DIR`. Initialize an override through Library storage controls or `auditorylab --library-dir PATH library init`. Built-ins and independent external exports remain available when persistent storage is unavailable. See [library configuration](cli.md#root-and-lifecycle) and [unsupported-file recovery](formats.md#unsupported-and-unreadable-recipes).

## Open, copy and manage sounds

**Open recipe…** admits a regular portable file into Open files. **Close open file** removes its temporary entry. **Add to User Library** imports its content while keeping the original file unchanged.

**Edit a copy** creates a new UUID and retains the source’s notes, bibliography and bounded ancestry. Its first User Library save sets Created and Modified. Source evidence becomes ancestor evidence. The copied-from snapshot remains inspectable during the session; saved lineage retains attribution after reopening.

**Edit saved User recipe** retains its managed identity and observed content token. **Edit external file** retains its opened path, identity and bytes. Save/Discard/Cancel protects dirty content before replacement.

Library actions retain an app-issued handle for the rendered listing and captured version. A superseded listing refuses the action; activate it again on the current entry. Rename and Trash use that captured entry. Trashing a dirty managed edit asks how to handle its draft. Restore preserves portable history and Added here. Permanent deletion is offered in Trash and asks for confirmation naming the entry. Copies, ancestors and external originals remain independent.

## Build and edit a sound

**New sound** opens an empty, silent Workbench. Its **Start with** choices, and **Add a component** below existing components, offer tones, pulse trains, both explicit pair kinds and pulsed tones as visible buttons. Tone/pulse sources cost one generator lane; pairs cost two within one 16-lane budget. [Fields and limits](formats.md#recipe-schema) define the budgets.

**Pulsed tone (isochronic)** adds a single tone with shared pulsed modulation. The [recipe guide](recipes.md#sinusoidal-and-pulsed-modulation) explains its waveform and defaults. Pulse trains and pulsed pairs have distinct names.

Component names use their authored IDs; removing/restoring rows keeps those references stable. A pair's secondary lane is labeled as its second tone.

Enter frequency/rate, width, level and modulation values. The UI uses Hz, milliseconds, dBFS and percent. Each component card shows its calculated pattern, values, source, shared modulation and digital channel routing together, with Remove in its header. **Saved audio and export**, **Sound notes** and **Location** sections follow the components. **Undo remove** restores the latest removed component and its staged text when capacity permits.

Pulse sources and pulsed modulation show separate width-intent and shape choices beside their calculated envelope. Duration fields use milliseconds; duty uses percent with a distinct semantic field. Raised-cosine controls expose rise/fall times. Width conversion applies the related pending rate/window values together and preserves other staged input; an invalid conversion changes nothing. Rectangular-to-shaped initialization uses 10% of the requested window per edge, without resizing it to satisfy a sample rate. Requested and resolved facts disclose rounding.

Pair modulation is shared across both lanes. Digital left/right/both routing cannot verify delivery to separate ears. Creation and interpretation changes never turn Play on automatically.

Typing stages text. Enter, leaving a field or a commit action applies the complete finite candidate. Unparseable or nonfinite values preserve staged text for correction. Escape restores the committed value. Notes support multiple lines; Return inserts a line break. Sliders use comfortable ranges and gesture rounding; numeric fields accept values outside those spans without clamping. Display-unit conversion uses binary64 and can round or overflow for extreme finite model values. An unedited field callback preserves the exact committed bits; explicitly edited text is parsed in the displayed unit.

Set device volume low before Play. Workbench playback follows committed settings valid at the actual device rate. Invalid edits retain the last valid playing settings. Pending text is labeled; metadata-only edits leave audio transitions unchanged. Library and Workbench playback remain distinct captured sources.

Saved duration and endpoint fades govern finite playback and WAV export. Live Workbench playback continues until Stop or its frame limit. **Applied rhythm** follows committed settings, and **Saved version** follows the exact confirmed save candidate.

## Save or export

**Save sound** captures the current complete draft. New drafts go to User Library, managed edits use their captured key/token, and external edits use the observed file. Later edits remain dirty after an earlier captured save completes.

**Save independent copy** creates a separate User identity. Portable recipe export preserves identity/history and does not mark a draft saved. In Library, the **Export** section holds the WAV, portable recipe and export-rate controls. **Export WAV…** validates at the export rate and opens the native Save dialog; stop audio first. Native Save dialogs confirm replacement.

Finite but acoustically invalid documents can be saved for repair. File errors keep the draft and identify the affected path. A content conflict requires Refresh/reload, retaining the draft, or saving an independent copy. A confirmed durability/cleanup warning remains a successful save. [Publication and recovery](formats.md#publication-and-recovery) explains these outcomes.

One audio slot and one file slot run independently. Stop targets audio; **Cancel export** targets WAV export. During native path selection, macOS offers Stop in the file panel; Windows/Linux offer modeless Audio controls. A worker slot remains busy until its thread has joined, including after displayed progress reaches completion.

New/copy/close/Quit protect unsaved content through Save/Discard/Cancel. Repeated Quit shares one decision. Failed saves and newer unsaved edits keep the application open.

## View timing and output

**Waveform pattern** shows normalized pulse/envelope timing calculated for the selected recipe, committed draft or playing revision; it is not recorded output. It names its source, rate and revision. **Monitor** in the player row opens or closes the Output monitor; **Preview waveform** shows a labeled seekable preview while idle.

The monitor displays generated stereo samples before device conversion, with peak/RMS values and coverage status. Choose a 2/10/40/100 ms window. **Fit signal** changes the common stereo axis, while **Fixed ±1** uses digital full scale. Neither changes samples or levels.

**Hold display** retains samples, source, rate, frame span, scale and meters together across navigation and sessions. **Return to live** selects current coverage. Monitor gaps, stale coverage and backend underruns remain distinct. Details opens selectable quantities; **Refresh diagnostics snapshot** updates them explicitly.

Worker completion, export progress and native preference changes arrive as events. The desktop redraw timer runs while audio is bound or the monitor is visible; an idle closed monitor needs no periodic workspace polling. Commands refresh their calculated source models before projection, independently of that redraw timer.

The drawer can be resized by pointer, keyboard or accessibility slider actions. When it cannot fit, a focused monitor window provides scrollable content. Closing that window returns focus to its opener. Reduce Motion defaults to a held display; Hold/Return to live overrides it, and **Follow system motion** restores the native preference.

## Appearance, typography and accessibility

The desktop follows the resolved native light/dark appearance and system UI font family. Changing appearance updates native diagram colors and controls without selecting a different lesson or band. Diagram labels follow the application font and text size; names identify bands alongside their colors. Use Tab to reach controls. Guide entry focuses the selected lesson. Arrow keys follow the compact lesson grid’s rows and columns; Home and End select its first and last topics. Enter or Space activates buttons; construction and band choices announce which one is checked. Library, Workbench and lesson sections are always open.

Font sizing follows the platform adapter: macOS uses AppKit’s `systemFontSize`, Windows uses message-font metrics at 96 DPI, and Linux uses toolkit DPI with GNOME Wayland’s text-scaling preference where applicable. The macOS Accessibility **Text size** preferred-reading control does not resize this application. It applies to its listed compatible apps; the desktop does not expose a separate font-size setting.

See [toolkit integration](../third_party/README.md) for native preference handling and [contributor checks](../CONTRIBUTING.md#checks) for accessibility/layout verification.

## Platforms

macOS requires version 15 or later and the matching CPU archive. Packages are ad-hoc signed. Keep the app’s Contents intact when moving it.

Linux downloads require glibc 2.35 or later, ALSA, X11 or Wayland, fonts, fontconfig, xkbcommon, OpenGL and an xdg-desktop-portal backend. X11 also needs `libxkbcommon-x11-0` on Debian/Ubuntu. GNOME Wayland text scaling uses GLib’s `gsettings`. Keep the extracted executable and legal files together and add that directory to PATH. For a per-user launcher, install `auditorylab-gui.desktop` in `~/.local/share/applications/` and merge the packaged `hicolor/` contents into `~/.local/share/icons/hicolor/`. Keep the `org.resoltico.auditorylab` icon names unchanged.

Windows requires Windows 10 on x86_64 or Windows 11 on ARM64 and an OpenGL 2.0 or newer driver. x86_64 requires SSE3, CMPXCHG16B and LAHF/SAHF. Fatal graphics/application failures use a native dialog.

The Edit menu applies Undo, Cut, Copy, Paste and Select All to the focused text field; keyboard shortcuts work in fields directly. **Help › Auditory Lab desktop guide** opens the guide for the build’s source revision, or its version tag when no revision is embedded. About is in the macOS application menu or the Windows/Linux Help menu. It shows product identity and links to source and licenses. Packaged dependency notices stay with the download and inside the macOS app. [Packaging](releasing.md) defines the complete delivered contents.
