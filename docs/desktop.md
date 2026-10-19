# Desktop

Start `auditorylab-gui` without arguments. On macOS, open **Auditory Lab.app**; Windows downloads contain `auditorylab-gui.exe`. `auditorylab-gui --version` prints the version and, for CI builds, a short commit ID.

## Library and Workbench

The **Library** lists titles, research topics and readable durations. Selecting a sound displays its summary and waveform. **Research details** expands selectable study text, components and facts at the export rate, including rounded per-ear peak bounds. The panels scroll while listening guidance stays visible below them.

**Open Recipe…** uses a native file dialog. Imports appear as **Imported: TITLE**; reopening the same path replaces its entry. A failed import preserves the selection and shows its error in selectable status text. **Open in Workbench** copies the sound's components, duration and fade, with a title ending in “(edited)” and its summary. Recipes with more than 16 components stay in the Library; reduce their component count in the file before opening them in the Workbench.

The **Workbench** starts with a silent, component-free draft. **New** creates another empty draft. Edit its title and summary, then use **Add Tone** or **Add Clicks** to add up to 16 components. New components start at −30 dBFS, in both ears, without modulation. Each row offers:

- **Ears:** Both, Left or Right.
- **Source:** Tone with Frequency, or Clicks with repetition Rate and pulse Width.
- **Level:** the component's peak in dBFS.
- **Modulation:** None, Sinusoidal with Rate and Depth, or Gate with Rate and Width.
- **Remove:** removes that component.

Every numeric component control has a slider and an exact-value field. Frequency, rate and width sliders are logarithmic; level and depth are linear. Width fields use milliseconds. Sliders apply while moving. Fields apply on Enter or loss of focus; actions commit pending field text before taking their sound snapshot. Typing remains intact while another control or playback readout updates. Typed values may lie outside the slider span when the core accepts them.

Row messages identify invalid component values. The peak readout reports the per-ear sum or its validation error. An invalid edit keeps the last valid live target and scope preview. Validation uses the device rate while live playback runs, and the export rate otherwise. Duration and fade errors appear beside those fields.

## Listening and live edits

Turn the volume down before playing. Use headphones for sounds that differ between ears. Selecting, importing or editing a sound never starts playback. **Play** uses the default output and validates at its rate.

Library playback follows the recipe duration. Workbench playback is open-ended: **Duration** and **Fade** affect saved recipes and WAV export. Live components fade in over 30 ms. Changes to level, ear routing and depth ramp smoothly; tone and sinusoidal-modulation frequencies glide with continuous phase. Changes of source/modulation kind or pulse rate/width crossfade. Each transition takes 30 ms. An edit arriving during a transition waits, and only the newest waiting target is kept.

**Stop** fades finite playback over 30 ms, shortened to the remaining frames near its end, then reports “Stopped.” Live Stop finishes the current transition before its 30 ms fade, taking up to about 60 ms of generated sound. Device buffering adds its output latency. A live stream ends automatically at `Signal::MAX_FRAMES` (2^36 frames: about 16 days at 48 kHz, or 24.8 hours at 768 kHz).

There is one operation across both tabs. Stop acts on that operation after switching tabs. Library selection, opening, New, saving, export and rate changes are disabled during operations. Workbench components and metadata remain editable during live playback.

Elapsed time follows the latency-compensated playback position. Live playback shows elapsed time alone; finite playback and WAV export also show progress. The two scope lanes use a fixed ±1 scale. Choose a 2, 10, 40 or 100 ms window. During playback it ends at the playback position; stopped Library previews begin after the fade-in. Workbench previews show current valid settings without saved edge fades.

## Save and export

**Save Recipe…** writes a schema-2 TOML recipe through the native Save dialog. It keeps the Workbench title and summary; citation, study and adaptation text are removed when opening a sound for editing. The result can be opened in the Library or used with CLI `--file`. Finite invalid values can be saved for later editing. Save rejects unparseable/non-finite numeric fields and files over the recipe reader's 64 KiB limit.

Choose **Duration**, **Fade**, an export rate and **Export WAV…** for a finite stereo float WAV. Core validation must pass at that rate. Both Save dialogs ask before replacing an existing file. Output is written and synced in a hidden partial file before publication. Stop cancels WAV export; normal cancellation removes the partial file.

Closing the window requests Stop and waits for its worker before quitting. macOS Cmd-Q and forced termination end the process immediately; an interrupted publication can leave a hidden `.NAME.RANDOM.partial` file beside the destination. Remove that file when it is no longer needed.

## Platforms and menus

macOS downloads require macOS 15 or later. For a downloaded app blocked on first launch, try opening it, then use **System Settings → Privacy & Security → Open Anyway**. [Apple's instructions](https://support.apple.com/en-us/102445) describe the confirmation.

Linux needs X11 or Wayland, fontconfig, xkbcommon, fonts and OpenGL. X11 also needs the xkbcommon X11 library (`libxkbcommon-x11-0` on Debian/Ubuntu). Native file dialogs need an xdg-desktop-portal and a portal backend for the desktop environment. Install those packages when dialogs are unavailable; the CLI's `--file` option reads recipes directly.

Windows requires the matching system architecture and an OpenGL-capable graphics driver. Source-build prerequisites are in the [README](../README.md#build-from-source).

**File** offers New Sound, Open Recipe, Save Recipe and Export WAV. **Help → About** shows the version, source link, licenses and Slint attribution. UI drawing uses Slint's OpenGL renderer; synthesis remains on the CPU. Idle windows stop polling; active operations refresh the waveform and progress through a timer.
