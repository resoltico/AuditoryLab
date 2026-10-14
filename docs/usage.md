# Usage

`auditory-lab --help` and `auditory-lab COMMAND --help` show the grammar without opening any file or device. The built-in catalog is part of the executable, so no other files are needed.

## Commands

| Command | Result |
|---|---|
| `list` | Every built-in sound: name, length and title. |
| `about [--] PRESET` | What a sound is, what its source studied and found, its citation and adaptations, as wrapped text. |
| `show [--output FILE] [--] NAME` | A built-in recipe as UTF-8 TOML, printed or saved to a new FILE. |
| `inspect [--sample-rate HZ] [--] PRESET` | Validation and a technical description at a sample rate, without opening a device. |
| `export --output FILE [--sample-rate HZ] [--] PRESET` | A new stereo 32-bit float WAV file. |
| `play [--] PRESET` | Playback on the default output, at its configured rate. |
| `--help`, `-h`, `--version` | Usage or the version; these must be used alone. |

`inspect` and `export` render at 48,000 Hz unless `--sample-rate` gives whole hertz from 1 to 768,000. A recipe can be valid at one rate and not another; see [rules checked at a sample rate](presets.md#rules-checked-at-a-sample-rate).

## Naming a preset

`PRESET` is read by its spelling, never by what exists on disk:

- `-` reads standard input until end of file.
- An operand containing `/`, `\` or `.` is a file path.
- Anything else is a built-in name.

Write `./name` for a file without a dot, `./-` for a file named `-` and `./--help` for a file named like an option. Options and the operand can come in any order; `--` ends options, and the operand after it follows the same rule. `--output -` names a file called `-`; export never writes audio to standard output. `show` accepts only built-in names.

When standard input is a terminal, the program first prints a one-line cue on standard error; on Unix, Ctrl-D on an empty line ends input. Input is at most 65,536 bytes.

## Saving and editing a recipe

`show NAME --output FILE` writes the recipe's exact bytes and never replaces an existing file. On a write or sync failure, it removes the incomplete file or reports both the original failure and why removal failed. Edit the copy in a plain-text editor, keep it UTF-8, and check it with `inspect` before export or playback. When you change a study's parameters, update `[about]` so it describes your copy; validation does not compare text with numbers.

## Output and errors

Results go to standard output: listings, descriptions, inspection reports and the completion line of a recipe save, export or playback. Notices, warnings and errors go to standard error. `export` and `play` print a start line with the resolved length and rate, and `play` adds the device and listening advice. A closed output pipe ends reporting quietly; a completed recipe save, export or playback stays successful even if its result cannot be printed. Other completion-reporting errors produce a warning with the completed result and output error.

`about` wraps the escaped text at 78 terminal columns using Unicode widths (ambiguous-width characters count as one column) and preserves combining and emoji sequences. An indivisible grapheme wider than a line is shown as escapes.

File paths, device names and text from recipes are shown quoted in technical reports and with control characters escaped everywhere, so text from a file cannot rewrite the terminal or forge report lines.

| Exit status | Meaning |
|---|---|
| 0 | Success. |
| 1 | A file, device or other runtime failure, including an existing output file. |
| 2 | Wrong usage, an unknown name, or a recipe the format or its rules reject. |
| 130 | Export or playback cancelled by an interrupt. |

An unknown name suggests `list`, and for a recipe file the explicit `./name` form. Errors in a recipe name its source and the field; rule violations also name the sample rate. A file saved as UTF-16, as some Windows tools do, is reported as such; save it as UTF-8.

## Playback

`play` uses the default output's configured sample rate, channel count and sample format; there are no overrides. The output needs at least two channels: left goes to the first, right to the second, and any others stay silent. Supported formats are 64- and 32-bit float and 32-, 24- and 16-bit integer.

Before playing, the program advises a low volume, and headphones when a sound routes components to one ear. Playback ends when the backend reports that the last frame has played. Underruns are counted and reported. A change of output device, invalid timing from the backend, 30 seconds without progress, or running 30 seconds past the sound's length ends playback with an error.

## Export

Export creates the file only if nothing has that name, writes the complete header and then the samples, and syncs the file before reporting success. Files up to 4 GiB of samples are RIFF/WAVE; longer ones are RF64. Samples are rounded to 32-bit float.

The file is incomplete while export runs. Leave its path and folder alone until the command ends: a failed or cancelled export removes the incomplete file, or reports that it could not.

## Interrupts

During `export` and `play`, the first Ctrl-C (Unix SIGINT, SIGTERM or SIGHUP; Windows Ctrl-C or Ctrl-Break) asks the operation to stop: export stops before its next block and removes its file, and playback falls silent once its 10 ms check sees the request. Sound already queued in the audio system still plays. A second interrupt exits at once with status 130 and may leave an incomplete file. Other commands, and reading input, end on the first interrupt as the system does.
