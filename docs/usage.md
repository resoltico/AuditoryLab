# Usage

[Quick start](../README.md#quick-start) covers installation. The catalog is embedded; commands need no repository files after installation. `auditory-lab --help` and `auditory-lab COMMAND --help` show the supported grammar without opening files or a device.

## Commands and input

| Command | Result |
|---|---|
| `list` | Built-in names, durations and titles. |
| `show NAME` | A built-in preset's TOML, including comments, ready to save and edit. |
| `inspect [--sample-rate HZ] [--] PRESET` | Validation and a description without opening a device. |
| `export --output FILE [--sample-rate HZ] [--] PRESET` | A fresh stereo 32-bit float WAV file. |
| `play [--] PRESET` | Playback using the default output's backend-reported stream configuration. |
| `-h`, `--help`, `--version` | Usage or executable version; global help/version must be used alone. |

`PRESET` resolves by its spelling: `-` reads stdin; an operand containing `/`, `\` or `.` is a file path; anything else is a built-in name. Filesystem existence does not change this rule. Use `./name` for a dotless file, `./-` for a file named `-`, and `./--help` for an option-looking filename. `show` accepts only a built-in name.

```sh
auditory-lab show chan-2022-clicks-40-hz | auditory-lab inspect -
```

With the one-second `short-clicks.toml` copy from [quick start](../README.md#quick-start):

```sh
auditory-lab inspect --sample-rate 44100 ./short-clicks.toml
auditory-lab export --output ./--help.wav ./short-clicks.toml
```

Options and the operand may appear in any order before `--`; option/value pairs stay adjacent. Duplicates are errors. `--` ends option parsing and retains the same source-resolution rule. Command help before `--` takes precedence over execution arguments. `--output -` names a file; export does not write audio to stdout.

Inspect/export default to 48,000 Hz. `--sample-rate` accepts whole hertz from 1 to 768,000; [stimulus rules](presets.md#rules-checked-at-a-sample-rate) still apply. Playback uses the backend-reported default stream rate.

Terminal stdin receives one best-effort stderr cue before reading until EOF. On Unix, press Ctrl-D on an empty line to finish a pasted document. Piped stdin remains quiet. Native interrupt handling stays in effect during input reading.

## Reporting and errors

Inspect reports origin, resolved rate/duration/fade and frame counts, components or silence, per-ear conservative peak bounds, and optional provenance. A bound is not a measured or necessarily attained maximum or an acoustic level.

Export/play attempt a start line on stderr with resolved timing, origin, optional quoted title, and the device snapshot for playback. There is no periodic progress or liveness guarantee. Completion is reported on stdout; notices, errors and underrun warnings use stderr. Failed best-effort notices do not block the operation. A closed output pipe stops reporting quietly. An error writing a completed export/play result leaves the operation successful; a stderr warning is attempted for errors other than a closed pipe.

File paths, inspect provenance, start titles and device names are debug-quoted. `list` titles are ordinary text. The shared terminal boundary escapes C0/C1 and bidi controls while retaining ordinary Unicode, LF and tab. Maintained catalog text is invariant under that boundary, so `show` emits the preset bytes unchanged. Quoted newlines cannot forge result lines.

| Exit | Meaning |
|---|---|
| 0 | Success, including a completed export/play whose result could not be reported. |
| 1 | Read, filesystem, device or other runtime failure; non-pipe output errors for help/list/show/inspect. |
| 2 | Usage, unknown name, format or stimulus validation error. |
| 130 | Cooperative export/play cancellation or a forced second handled interrupt. |

For an unknown `show` name, use `list` and select a built-in NAME. For an unknown PRESET name that was meant to be a file, use an explicit path such as `./name`. Format errors identify the source and invalid field; [presets](presets.md) defines the format. Rate-dependent validation errors name the rendering rate. For an existing export destination, choose a fresh path. Output errors do not imply that a completed WAV was removed.

## Playback

Playback uses the backend-reported default stream rate, channel count and representation; there are no CLI configuration overrides. Discovery does not start this app's playback stream, but backend queries may open resources. The reported configuration does not certify the current physical hardware rate. At least two channels are required: left/right go to channels zero/one, with further channels silent. Supported representations are 64/32-bit float and 32/24/16-bit signed integer.

After discovery and validation, `play` attempts a low-volume notice on stderr. It recommends headphones when components declare separate left/right routing. This notice follows the declared routes; it does not compare resulting waveforms or detect physical ear routing.

Completion uses the backend's estimate of when the last frame has played, not an acoustic measurement. Underruns are counted. A route-change report, invalid clock/buffer, 30 seconds without progress, or exceeding session length by 30 seconds ends playback with an error. These checks cannot interrupt a backend call that never returns.

## Export and ownership

Create-new semantics preserve existing files, directories and symbolic links. Samples are rounded to binary32. RIFF is used while its size fields fit, otherwise RF64. The complete header precedes payload; the file is synced before success.

The output is incomplete during export. Keep its pathname and parent unchanged until the operation ends. Under this exclusive ownership, cancellation/failure removes incomplete output or reports the original failure and failed removal. Cleanup addresses the pathname; concurrent replacement is outside this contract. Forced termination can leave incomplete output.

## Interrupts

`export` and `play` install cooperative handlers after input reading. The first Unix SIGINT, SIGTERM or SIGHUP, or Windows Ctrl-C/Ctrl-Break, requests cancellation. Export checks before each block and before final success. Playback supervision polls at 10 ms intervals; when it observes cancellation, subsequent callbacks return silence. Thread scheduling and backend calls can delay observation, and queued sound cannot be recalled.

A second handled interrupt forces exit 130, including during completion reporting. Completed files remain; unfinished output may remain. Cancellation cannot undo a completed operation. Input reading and help/list/show/inspect use native signal handling. Console closure, shutdown and forced kills have no cooperative-cleanup guarantee.
