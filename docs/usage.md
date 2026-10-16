# Usage

Run `auditorylab` with no arguments for a starting point, or `auditorylab COMMAND --help` for command help. Both return successfully without reading stdin or opening a device. From an unpacked download, use `./auditorylab` on macOS/Linux or `.\auditorylab.exe` in PowerShell; an installed command on PATH uses `auditorylab`.

## Commands

| Command | Result |
|---|---|
| `list` | Built-in preset IDs, human durations and wrapped Sound descriptions. |
| `describe SOURCE` | Declared sound facts, listening guidance, research context and adaptations. |
| `recipe PRESET_ID [--output PATH]` | Exact built-in UTF-8 TOML, printed or saved to a new file. |
| `inspect SOURCE [--sample-rate HZ]` | Validation and exact technical facts without opening an output device. |
| `play SOURCE` | Playback on the default output, at its stream rate. |
| `export SOURCE --output PATH [--sample-rate HZ]` | A new stereo 32-bit float WAV file. |
| `--help`, `-h`, `--version`, `--build-info` | Help/version; top-level forms must be used alone. |

Recipe files use `[about]` for descriptive metadata. `recipe` accepts only a built-in ID because it retrieves the original source, rather than reconstructing a document.

## Choosing input explicitly

Choose exactly one SOURCE:

- A positional built-in preset ID, such as `chan-2022-clicks-40-hz`.
- `--file PATH` for a regular UTF-8 TOML recipe file on disk (including symlinks to regular files). Directories, pipes and devices reject before reading; use `--stdin` for intentional streams.
- `--stdin` to read a UTF-8 recipe until EOF.

A positional value always names a built-in preset: `tone.toml`, `./tone`, `-` and values after `--` never select a file or stdin. Existing files and piped data never change that rule. Unknown IDs suggest `list` and the explicit `--file PATH` form; names are not autocorrected.

Options and the preset ID may appear in either order before `--`. The delimiter only ends option recognition; remaining operands are still built-in IDs. `list --` succeeds, `list -- extra` rejects, and a top-level `auditorylab --` is a usage error. Missing sources, source combinations, duplicate options, extra operands and invalid values reject before input or output I/O.

Path and rate options accept both separated and equals forms: `--file recipe.toml`, `--file=recipe.toml`, `--output=beat.wav`, `--sample-rate=44100`. Native path spelling is retained without canonicalization or lossy conversion. A separated path beginning with a hyphen is rejected except a lone `-`; use `--file=-draft.toml` or `--output=-draft.wav` instead. `--file=--help` and `--file=--` name those literal files. `--file -` reads a file named `-`; `--output -` creates a file named `-`. Export never writes audio to stdout.

Only exact standalone `-h` or `--help` before the first standalone `--` requests command help, even if other arguments are incomplete. For example, `play --file --help` shows help without I/O, while `play --file=--help` selects a file. After `--`, `--help` is a catalog ID. Unknown commands still reject even when followed by help. Short bundles/attached forms (`-hh`, `-hfoo`, `-h=foo`), `--help=foo`, abbreviations and all other short options reject unless a separate exact help request takes precedence.

When explicit `--stdin` selects a terminal, a one-line cue is printed on stderr; on Unix Ctrl-D on an empty line ends input. Input is bounded at 65,536 bytes. Regular-file admission does not promise an I/O deadline for stalled remote or failing filesystems, and captured bytes are not an atomic filesystem snapshot. No signal handler is installed until input has been read.

## Discovery, description and inspection

`list` preserves natural catalog order and complete IDs. Each record has an ID/duration row and an indented Sound field, which may wrap onto continuation lines. The human layout is not a machine protocol.

`describe` presents declared duration, actual component ear assignments and study metadata without validating numeric playability or querying a device. No components means silence. Both-ear, left-only and right-only assignments remain distinct; separate equal tones are still separate-ear assignments. The app supplies headphone guidance from its routing policy. Guidance is for later playback, and does not mean describing a sound starts audio. Missing metadata is stated explicitly. Descriptive prose is not checked against the numeric parameters.

`inspect` and `export` use the application default of 48,000 Hz unless `--sample-rate` gives whole decimal hertz from 1 to 768,000. Inspection labels the selection as default or explicit, including an explicit selection of 48,000 Hz. It reports exact frames/rate and declared versus resolved duration and fade, including a positive declared fade rounded to zero frames (fading disabled). The frame count and rate determine resolved seconds; division can round. Playback validates again at the output stream's sample rate. Neither inspection nor a backend stream configuration certifies the physical hardware rate. See [validation rules](presets.md#rules-checked-at-a-sample-rate).

Human durations use hours/minutes/seconds, such as `1 h` or `5 min 7.2 s`. `~` marks an approximation when conversion to nanoseconds changes the binary64 input. Tiny, huge or invalid declarations retain round-trip seconds rather than being silently clipped or rounded to zero. Technical views retain exact counts, rates and round-trip scalar values. Peak bounds describe f64 generation before output conversion; float/integer quantization can change peaks slightly.

## Editing recipes

```text
auditorylab recipe chan-2022-clicks-40-hz --output clicks.toml
auditorylab describe --file clicks.toml
auditorylab inspect --file clicks.toml
auditorylab export --file clicks.toml --output clicks.wav
```

`recipe PRESET_ID` writes exact source bytes to stdout without headers, sanitization or completion text. `recipe --output` saves those same bytes through the staged publisher. Edit a copy as UTF-8 and update `[about]` when its meaning changes. The source study's nominal frequency can differ from the configured value; the van Eeckhoutte AM recipes explicitly configure 39.0625 Hz, while their study discusses nominal 40 Hz.

## Output and errors

Help, version, list, describe, inspection and raw recipes go to stdout. Preparation, reminders, warnings, cancellation and errors go to stderr. Successful save/export/play completion goes to stdout. A closed output pipe is quiet success. Ordinary read-only reporting failures return status 1; after a side effect completes, a reporting failure cannot undo it and produces only a best-effort warning containing the completed result/path.

Human prose and continuation fields wrap at 78 columns using Unicode widths and grapheme boundaries. Inline control characters, bidi controls, newlines and tabs are escaped before width measurement. Native paths/device names are quoted; IDs and path identity remain intact rather than truncated. A grapheme wider than a line is rendered as escapes. Human output is static in terminals and pipes, with no pager, color or implicit prompt.

| Exit status | Meaning |
|---|---|
| 0 | Success, including empty invocation/help. |
| 1 | File, device, publication, cleanup or other operational failure. |
| 2 | Usage, unknown ID, format/schema or validation rejection. |
| 130 | Clean cooperative export/playback cancellation. |

Usage errors give one diagnosis, short usage, a targeted example and a help pointer. Runtime errors retain source/destination context and technical causes. A final no-clobber conflict says the destination was untouched and suggests a different `--output` path. An AlreadyExists error before commit remains a publication failure and does not blame the requested destination. Cleanup failure retains both causes and the owned stage path. UTF-16 BOMs are diagnosed; input still requires UTF-8.

## Playback

Turn output volume down before running `play`. Preparation says `Starting playback` before execution and shows the source, resolved duration, actual device or unavailable name, stream rate, channel count and sample representation. It supplies listening reminders and `Press Ctrl-C to stop`; there is no confirmation prompt or countdown. Digital levels do not limit sound pressure.

The default output needs at least two channels: left goes to the first, right to the second, and extras remain silent. Supported formats are 64-/32-bit float and 32-/24-/16-bit integer. Completion is based on backend timing estimates, not acoustic delivery. Underruns are counted; nonzero counts produce the app's typed warning. Route changes, invalid timing, 30 seconds without progress, or running 30 seconds beyond the session length end playback with an error.

## New-file publication

Export preparation binds the requested destination spelling and creates no output. The app stages recipe/WAV bytes in a randomly named file in that destination folder, syncs them, then publishes without replacing any existing file, symlink or directory. WAV uses RIFF when 32-bit container sizes fit, otherwise RF64, and rounds samples to 32-bit float.

These operations create no final file before commit and never replace an existing destination. Failure/cancellation cleans only the owned stage; cleanup failure identifies it alongside the primary cause. Unix permissions follow ordinary creation and the inherited umask. Keep the directory under ordinary caller control: binding a pathname is not binding a directory handle. No-overwrite publication is not universally atomic; some filesystems may leave a staging hard link after success. Abnormal termination can leave a stage, there is no startup scavenger, and file sync does not guarantee directory-entry persistence after power loss.

## Interrupts

During export/play, the first Ctrl-C (Unix SIGINT/SIGTERM/SIGHUP; Windows Ctrl-C/Ctrl-Break) requests cancellation; the second exits immediately with 130. Export checks before writing, between blocks and before commit. Successful publication completes the operation even if cancellation races after the last check. A forced exit can leave a stage. Playback polls at 10 ms intervals, subject to scheduling/backend delay, and callbacks silence after observing the terminal outcome. Already queued sound cannot be recalled. Cancellation reports no invented partial-frame count or acoustic delivery. Input reads and other commands retain default system interruption behavior.

`--version` identifies the product/version and qualification kind rather than implying that a local build is published. Standalone `--build-info` emits the narrow artifact-identity JSON protocol before input/device work; it is not a general JSON report mode. Qualified identities include the full checked source revision and target.
