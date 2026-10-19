# CLI

Run `auditorylab` for help, `auditorylab COMMAND --help` for command options, or `auditorylab -V` for the version. Version output includes a short commit ID in CI builds.

## Commands

| Command | Result |
|---|---|
| `list` | Built-in IDs, titles, research topics and durations. |
| `describe SOURCE` | Declared parameters, components and study context. |
| `recipe PRESET_ID [--output PATH]` | Original built-in TOML bytes, printed or saved. |
| `inspect SOURCE [--sample-rate HZ]` | Validation, frame count, resolved duration/fade and per-ear peak bounds. |
| `play SOURCE` | Playback at the default device's sample rate. |
| `export SOURCE --output PATH [--sample-rate HZ]` | A stereo 32-bit float WAV file. |

`inspect` and `export` default to 48,000 Hz. The accepted rate range is defined by [SampleRate](../crates/core/src/rate.rs). Playback validates at the device rate.

## Choosing input

Use exactly one SOURCE: a positional built-in ID, `--file PATH`, or `--stdin`. Files must be regular files or symlinks to them; pipes, directories and devices are rejected before reading. Use `--stdin` for streams. A positional filename still names a built-in ID.

```text
auditorylab describe chan-2022-clicks-40-hz
auditorylab inspect --file clicks.toml --sample-rate 44100
cat clicks.toml | auditorylab inspect --stdin
```

`recipe` retrieves only built-in source text. Save a copy with `--output`, edit it as UTF-8, then use `--file` to inspect, export or play it. Keep `[about]` consistent with your edits. See [presets](presets.md).

When stdin is a terminal, the CLI prints an end-of-input cue before reading. Interrupt handling for export/play begins after input has been read.

## Output and errors

Help, version, list, descriptions, inspection, recipe bytes and completion lines go to stdout. Start lines, listening notices, underrun warnings and errors go to stderr. Human prose wraps at 78 columns, and control/bidi characters are escaped for terminal display. Raw recipe output preserves every original byte. A closed output pipe is quiet success. If completion reporting fails after saving, exporting or playing, the operation remains complete and stderr receives a warning.

| Exit code | Meaning |
|---|---|
| 0 | Success, including help and an empty invocation. |
| 1 | Read, device, write, destination conflict or cleanup failure. |
| 2 | Usage, unknown ID, recipe format or signal validation error. |
| 130 | Cooperative cancellation. |

TOML errors retain line, column and snippet text. Validation errors identify the parameter and component. Failed cleanup names the partial file that remains.

## Playback

Turn the volume down before `play`. Its start line gives the title, source, duration, device, rate, channel count, format and listening notices. Left and right use output channels 0 and 1; additional channels are silent. Completion follows backend timestamps. The CLI reports nonzero underrun counts after playback. A route change, invalid timing or a stalled stream stops playback with an error.

## Files and Ctrl-C

Recipe saving and WAV export never replace an existing file. Choose another output path when a destination exists. The library writes a hidden `.NAME.RANDOM.partial` file beside the destination, syncs it, then publishes it. WAV uses RIFF when its sizes fit, otherwise RF64. Cancellation or write failure removes the partial file; a cleanup error identifies any leftover. Unix output permissions follow ordinary file creation and the inherited umask.

During export/play, the first Ctrl-C requests cancellation and the second exits immediately with 130. On Unix, SIGTERM and SIGHUP also request cancellation. Export checks before creating the partial file, between blocks and before publication. Playback checks cancellation every 10 ms. Sound already queued by the device drains according to its backend. A completed publication stays successful if cancellation arrives afterward. Forced termination can leave the hidden partial file beside the destination.
