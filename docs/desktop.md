# Desktop

Start `auditorylab-gui` without arguments. On macOS, open **Auditory Lab.app**; Windows downloads contain `auditorylab-gui.exe`. `auditorylab-gui --version` prints the version and, for CI builds, a short commit ID.

## The window

The sound list shows titles, research topics and durations. Selecting a sound displays its summary and a waveform preview. **Research details** expands selectable citation, source, study, adaptations, components and facts resolved at the export rate. Validation errors appear with those facts.

**Open Recipe…** uses a native file dialog. Imported recipes appear as **Imported: TITLE**. Reopening the same path replaces its list entry. A failed import keeps the selected sound and shows the error in selectable status text.

## Listening

Turn the volume down before playing. Use headphones for sounds that differ between the ears. **Play** uses the default output device and validates at its sample rate. **Stop** requests cancellation. The list and export controls are disabled while an operation runs.

Elapsed time and progress follow the latency-compensated playback position. The two scope lanes use a fixed ±1 scale and show the same computed signal as playback. Choose a 2, 10, 40 or 100 ms window. While playing, the window ends at the playback position; when stopped, the preview begins after the fade-in. Selecting a sound never plays it.

## Export

Choose an export rate and **Export WAV…**. The native Save dialog supplies the destination and asks before replacing an existing file. The app writes and syncs a hidden partial file, then publishes the completed stereo float WAV. Use **Stop** to cancel; normal cancellation removes the partial file.

Closing the window cancels an active operation and waits for its worker to finish before quitting. macOS Cmd-Q and forced termination end the process immediately; an interrupted export can leave a hidden `.NAME.RANDOM.partial` file next to the chosen destination. Remove that file when it is no longer needed.

## Platforms and menus

macOS downloads require macOS 15 or later. For a downloaded app blocked on first launch, try opening it, then use **System Settings → Privacy & Security → Open Anyway**. [Apple's instructions](https://support.apple.com/en-us/102445) describe the confirmation.

Linux needs X11 or Wayland, fontconfig, xkbcommon, fonts and OpenGL. Native file dialogs need an xdg-desktop-portal and a portal backend for the desktop environment. Install those packages when dialogs are unavailable; the CLI's `--file` option reads recipes directly.

Windows requires the matching system architecture and an OpenGL-capable graphics driver. Source-build prerequisites are in the [README](../README.md#build-from-source).

**File** offers Open and Export. **Help → About** shows the version, source link, licenses and Slint attribution. UI drawing uses Slint's OpenGL renderer; synthesis remains on the CPU. Idle windows stop polling; active operations refresh the waveform and progress through a timer.
