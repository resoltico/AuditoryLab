# Desktop

Start `auditorylab-gui` without arguments. On macOS, open **Auditory Lab.app**. Choose the **Mac · Apple silicon** or **Mac · Intel** download for your processor; each contains a single architecture and requires macOS 15 or later. Windows downloads contain `auditorylab-gui.exe`.

`auditorylab-gui --version` prints the product version, numeric build counter and optional source revision without opening a window or output device. About is available only through the macOS application menu or Windows/Linux Help menu. It shows the product version, native build number, selectable source identity, licenses and toolkit attribution. **Source and issue tracker** opens the repository.

## Library

Search title, ID, sound parameters and research text. Titles wrap; each result includes sound structure, research topic and duration. Clear search restores the catalog. **Open Recipe…** uses a native file dialog; imported entries have an Imported marker and their origin appears in details. Reopening the same path replaces that entry. A failed import preserves the existing recipes.

Selecting a recipe never starts playback. Details show its structured sound description, duration and summary. **Research, citation and recipe adaptations** expands selectable source text. Human and animal findings remain attributed to their source recipe. Drag the Library divider or adjust it with its keyboard/accessibility controls to resize the panes. At compact window widths, choose a recipe to open details and use **Back to recipes** to return.

**Listen** starts finite playback on the default device. While another sound plays, the action reads **Stop current & play recipe**: it fades and drains the old stream before preparing the replacement. Browsing and research disclosure remain available throughout. The player below the screens continues to identify the actual session.

**Edit a copy** opens components, duration and fades in Workbench with an edited title. Custom notes start blank. Source summary/citation remain in the attributed **Based on** context under **Recipe settings**; **Show source recipe in Library** returns to that source. Saving the custom recipe does not copy the source's study claims. Recipes exceeding 16 components remain in Library and require reducing their component count before editing.

## Workbench

An empty draft generates silence. **New**, **Save Recipe…**, **Add Tone** and **Add Clicks** remain above the scrolling editor; the persistent player provides **Play live** and Stop. **Export WAV…** is under Recipe settings. Up to 16 components have stable identities. New components start at −30 dBFS in both ears without modulation.

Each component leads with its role and physical context. Routing, source, modulation kind and removal are under **Routing, modulation and component options**. Active modulation controls remain visible. Tone frequency, pulse rate/width, level, sinusoidal rate/depth and gate rate/width have sliders and exact-value fields. Width fields use milliseconds; depth fields use percent. Frequency/rate/width sliders are logarithmic; level/depth are linear. Slider gestures use three significant digits for frequency/rate/width, 0.1 dB for level and whole percentage points for depth; exact fields commit on Enter or blur. At compact widths exact fields remain available while sliders hide. Pending typing survives status, visualization, About and native-font updates, and actions commit pending text before capturing their draft. Typed values outside a slider's range remain usable when valid at the rendering rate.

Whole targets validate at the live device rate. Invalid text or target values retain the last accepted settings and identify the failed fields. The warning **Edits not applied — playing the last valid settings** distinguishes those controls from generated quantities in the player. Pending text is labeled as pending. Metadata-only changes do not restart transitions.

**Recipe settings · notes and saved/exported audio** contains custom notes, attributed source context, duration, fades and export sample rate. Library has a corresponding Export settings disclosure. Device status reports the bound playback rate separately. Duration and saved edge fades affect finite recipes/WAV files. Live playback continues until Stop or its frame lifetime limit.

## Persistent player

The player stays bound to its captured session while either screen is browsed or edited. It names the source and finite/live mode, estimates elapsed output time from backend timestamps, and provides global Stop. A finite Library session keeps its original title/duration when the draft changes. Save/import/export use the same serialized worker; unavailable mutations have a visible reason. Export offers cancellation through the operation control.

**Generated audio** reports the last generated frame's typed quantities. Exactly one unmodulated left-only tone and one right-only tone produce **Interaural difference**, with both carriers. Click, gate and active AM rhythms retain their independent rates, carriers, widths/depth and routing; clicks plus a steady tone retain both meanings. Equal-level unmodulated tones on the same route have a narrow monaural-beat description. Additional voices or modulation reject that pair classification. A click or gate rate change crossfades simultaneous outgoing/incoming generators, labeled Changing. Generated details disclose component roles, gains and accepted/active/settled revisions. Accepted updates may still be waiting; only the latest queued target survives. The description concerns generated digital audio, not a measured perceived beat or an acoustic playhead.

Live level/routing/depth changes ramp over 30 ms. Tone and sinusoidal-modulation rates glide with continuous phase. Source/modulation kind and pulse rate/width changes crossfade. Stop fades finite audio over 30 ms, shortened at its end. Live Stop finishes any current transition and then fades over 30 ms: almost 60 ms of generation can remain, plus device buffering. A live stream's maximum lifetime is 2^36 frames, about 16 days at 48 kHz.

**Timing model** projects the validated recipe, last valid draft or accepted target from frame zero. It shows independently attributed click/gate occupancy and AM envelope bounds, with steady-tone and interaural relationships kept distinct. Models aim for four cycles within 250 ms–4 s, with at most 32 lanes and 512 columns each. Slow processes are labeled when the capped span cannot contain one cycle. Dense columns preserve brief open intervals. These are static projections, separate from generated phase history. Exact frame widths and revisions are under Audio details and diagnostics.

**Audio waveform** contains actual mixed samples after ramps, fades and clamping, before device channel/format conversion. **Fixed ±1** uses the digital full-scale range. **Fit signal** uses one symmetric peak limit shared by both ears; silence uses ±1. It changes drawing geometry, not samples or meters. Window sample peak and RMS, in dBFS, describe the contiguous captured window; silence is −∞. Peak bounds in component context are theoretical bounds. Choose 2/10/40/100 ms; actual monitor history stays bounded to 100 ms.

**Freeze waveform** captures traces, meters, title/session, rate, frame span, observation, sequence, gap/underrun counters and scale together. Window and scale controls are disabled until Resume. Audio, elapsed time, generated status and errors continue. Resume requests the latest window. Motion preferences default the trace to frozen; unknown preferences also default to frozen. Capture provenance remains separate from current monitoring status. Monitoring gaps and backend output underruns are distinct. Monitoring failure does not stop healthy audio. Hide visualization to give the editor more space; the transport and errors remain reachable.

Before playback, traces are explicitly recipe/draft previews at the preview/export rate. After Stop/completion/failure, the player retains a labeled last generated window, or the explicitly labeled frozen window when drawing was frozen. **Preview selected sound** explicitly replaces that retained session with the selected preview.

## Save, export and quit

Save writes schema-2 TOML with the custom title, notes and sound. Finite model-invalid numbers can be saved for later correction, reported as **Saved with validation errors**. Unparseable/non-finite fields and files over the reader's 64 KiB limit cannot be saved. Play and Export require validation at their respective rates. Save/Export use native overwrite confirmation and atomic staged publication.

The dirty indicator compares saved recipe content, excluding navigation, focus, generated status and monitoring. New, replacing the draft, window close, File Quit and native macOS Cmd-Q use **Save / Discard / Cancel**. Cancel preserves the draft. Save-and-close first stops/drains any active operation, then saves the captured draft and closes only on success. Failed/cancelled saves keep the window open. If edits arrive after the saved snapshot, they remain dirty and prevent the pending destructive action.

Window close, File Quit and app-menu Quit/Cmd-Q wait for worker cleanup. Forced process termination can leave a hidden `.NAME.RANDOM.partial` file beside an interrupted publication destination; remove it when no longer needed. Cancellation and completion do not depend on monitoring availability.

## Platforms

A downloaded macOS app may require **System Settings → Privacy & Security → Open Anyway** after its first launch attempt; see [Apple's instructions](https://support.apple.com/en-us/102445).

Linux needs X11 or Wayland, fontconfig, xkbcommon, fonts, OpenGL and an xdg-desktop-portal with a desktop backend for native dialogs. X11 also needs `libxkbcommon-x11-0` on Debian/Ubuntu. Windows needs the matching architecture and an OpenGL-capable graphics driver. See [source prerequisites](../README.md#build-from-source).

The UI uses native San Francisco on macOS, the Windows message font with text scaling (Segoe UI by default), and configured Linux desktop family/size. GNOME uses its settings portal or native GSettings; KDE uses its portal QFont preference; other GTK desktops use native XSettings. Linux needs Pango for native font-description parsing and GTK 3/GIO for those fallback preference sources. Missing services or unavailable families are diagnosed in Audio details and diagnostics. Preference queries run off the UI/audio paths and coalesce; updates reflow app-owned windows without submitting audio targets.

UI drawing uses Slint/OpenGL. Synthesis runs on the CPU. Native menu and context-menu commands retain the backend’s ordinary event routing. Parent-bound asynchronous dialogs leave the application event loop responsive. Listening guidance remains visible; turn down the volume before playing and use headphones for separate-ear sounds.
