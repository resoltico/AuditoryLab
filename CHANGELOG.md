# Changelog

## [Unreleased]

- Recipes require schema 4: put duration, fade and components in `[sound]`; use `clicks`, `am` and `gate`. Rewrite custom files against the [format](docs/formats.md#recipe-schema).
- Schema-3 User files stay unchanged; discovery is incomplete and `list` exits 5. Rewrite or move them outside the library; see [recovery](docs/formats.md#unsupported-and-unreadable-recipes).
- CLI `--method am` replaces `amplitude-modulation`; `--analysis` accepts only `complete` or `invalid`. Plain tones remain independent; author pairs explicitly.
- CLI JSON schema 2 exposes generator lanes under `voices`; diagnostics and warnings contain stable `code` and human `message` fields. Update consumers.
- Classifier revision 2 uses expanded owner/lane identities and removes inferred raw-tone pairings.
- The Rust app package is `auditorylab-app`, exposing core as `auditorylab_app::core`. Call typed services/workspace views; native storage/audio adapters are private.
- `auditorylab-presentation` owns research/About views, parameter help and reports through extension traits. `Recipe::fork` requires an explicit attribution note.
- Rust authored numbers use `Finite`; `Sound` fields are private with checked constructors/mutators enforcing unique owners and both budgets.
- `Finite` equality preserves signed-zero bits; `RecipeContent`/`Recipe` equality replace manual content/document comparisons. `Recipe` fields use checked builders/accessors.
- `ComponentForm` replaces `Node` with flat tone/click/pair variants; `Voice` is private. Build forms with `Finite` values and use `get`/`set`.
- `Clicks`, `Am` and `Gate` replace `Click`, `Sinusoidal` and `Rectangular`; update Rust matches and recipe generation.
- Core uses `SoundError`/`VoiceError` instead of `StimulusError`/`ComponentError`; `SoundError::Voice` identifies the authored owner and lane.
- Rust generator APIs use `CompiledVoice`, `VoiceFacts` and `voices()`; `observations()` yields origin-zero observations. Update lane-level callers.
- `ComponentId` is nonzero; expanded generators use `VoiceId { component, lane }`. Update constructors and consumers of generator identities.
- App origin keys/tokens live in `document::origin`, job controls in `job` and outcome categories in `outcome`. Shared vocabulary and query choices belong to core.
- Editable number text belongs to `document::input`; monitor/control models and typed views belong to workspace. Update app/presentation imports.
- App errors/warnings retain typed causes and display stable codes; use presentation explanations/renderers for human text and update error matches.
- Workspace jobs run through `Executor`; `Runner::shutdown` returns typed failures with captured context after joining workers.
- Nonfinite TOML numbers fail at their key with line/column format diagnostics; finite but acoustically invalid documents remain savable for repair.
- Components are named by authored ID, including a pair's second tone; removal keeps references stable. Method group captions use shared vocabulary labels.
- Unedited numeric fields retain exact model bits. Explicit numeric input uses displayed units, whose binary64 conversions can round.
- Core users can drive finite playback and Stop fading through `FiniteSource` without the native app adapters.
- Workbench preview, copy, restore and export controls share operation admission. Stale selections, save failures and newer edits retain their captured context.
- App-owned buttons guard disabled actions; authenticated toolkit patches include archive lockfiles and require verification during dependency maintenance.
- GUI vocabulary generation builds core without app/audio build dependencies; product display identity is declared in workspace metadata.
- Local compiler hygiene warns; CI denies workspace Clippy/doc warnings. Missing-documentation warnings apply only to core.

## [0.6.0] - 2026-10-26

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.6.0).

- Schema 3 requires UUIDs and component IDs; schema 2 is not migrated. Recreate custom files using the [recipe guide](https://github.com/resoltico/AuditoryLab/blob/v0.6.0/docs/recipes.md).
- CLI sources require builtin:ID or user:UUID; bare catalog IDs reject. Files and streams still use --file or --stdin. Update scripts and saved command lines.
- CLI JSON schema 1, text reports and exit codes change; update consumers using the [CLI guide](https://github.com/resoltico/AuditoryLab/blob/v0.6.0/docs/cli.md#output-and-failures).
- Rust APIs use Sound, Component, Node, Voice and Rendition. Application APIs move into named modules under crates/app; core is exposed through auditorylab::core. Update imports and callers.
- The device-playback feature is removed; auditorylab always depends on CPAL. Remove the feature from dependency declarations; device-independent consumers can use auditorylab-core.
- Workspace construction requires a library binding; use typed jobs/responses and Runner for execution. File/audio slots remain occupied until their workers join.
- Library layout 1 stores plain recipe TOML with separate membership dates. Unsupported layouts, including incompatible prerelease stores, are refused without automatic conversion.
- CLI and desktop share a persistent User Library, immutable Built-ins, temporary Open files and recoverable Trash. Add, copy, rename, restore and confirmed purge preserve external originals.
- Absolute library overrides require initialization; see [setup](https://github.com/resoltico/AuditoryLab/blob/v0.6.0/docs/cli.md#root-and-lifecycle). Unavailable roots never silently fall back.
- Recipes retain dates, notes, bibliography and scoped evidence. Copies get fresh UUIDs and ancestry; copying or changing sound settings demotes current-study evidence to ancestor scope.
- Workbench and TOML support signed-offset binaural/monaural pairs with separate levels and stable IDs. Each pair uses two of 16 expanded voices; the authored-component limit is also 16.
- Shared search uses Unicode-normalized terms and whole numeric values. Correlated rhythm/evidence filters, unique grouping and deterministic sorts cover Built-in, User and temporary sounds.
- Desktop date filters use inclusive local days with selectable UTC details. Unknown dates sort last; draft and saved classifications follow their respective content.
- Managed writes lock and verify content; CLI --expect checks exact-byte SHA-256 tokens. GUI saves check captured versions. External Save rechecks identity/bytes but can still race another writer.
- Ambiguous publication, deletion and move results are verified. Unreadable outcomes remain unconfirmed; confirmed durability/cleanup warnings preserve successful saves.
- Library separates collection/band navigation, results and sound details. Filters and storage tools use bounded popups; committed query details remain accessible with empty or incomplete results.
- Library/Workbench tabs, Save and transport stay visible while scrolling. Component creation is grouped, coupled errors appear once, and About uses toolkit typography with system text scaling.
- Managed/external edits and independent-copy saves retain captured destinations. Newer edits stay dirty after saves; stale row callbacks cannot change replacement documents or restored controls.
- The Output monitor starts closed; waveform preview reveals it. Show/Hide preserves selection and audio, while persistent Stop remains available through browsing and file tasks.
- Linux uses SystemUi and upstream Xft DPI; GNOME Wayland text scaling needs GLib gsettings. X11 needs its xkbcommon library. Pango/GTK font-preference loading is removed.
- The catalog adds one-minute Ioannou 2015 binaural conditions at 2, 6, 20 and 40 Hz, with source evidence and adaptation limits. Rhythm labels describe audio structure rather than EEG effects.
- Pulse generation reduces integer division, and closed live pulses skip unused modulation work without changing phase or sample bits.
- Monitor publication avoids callback locks, keeps coherent bounded captures and counts dropped visual samples.
- CI qualifies callback cost with shipped optimization settings, authenticates vendor patches and checks authored-sound mutations. File-menu tests capture jobs before timers can complete them.

## [0.5.0] - 2026-10-23

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.5.0).

- macOS desktop archives use ZIP; update tar.gz download/extraction scripts. Archives add SOURCE.txt, and Linux desktop packages add hicolor icons; update archive allowlists and launcher installs.
- macOS icon compilation requires Xcode 26+ on macOS 26+; macOS 15 assembly accepts matching precompiled icons. Windows desktop builds require the SDK resource compiler.
- Source packages without Git require AUDITORYLAB_SOURCE_REVISION and SOURCE_DATE_EPOCH from the release commit. Archive times and source identity are bound to that commit.
- Release builds require an increased product build counter. Dirty-tree packages identify base HEAD and remain development evidence.
- Rust APIs adopt Recipe, RecipeSnapshot, ParameterId, ComponentId, Session and EditorSession. Update imports and matches; recipe schema 2 and catalog signal parameters are unchanged.
- Library playback requires the device-playback feature; CLI and desktop enable it. Workspace coordinates editing, prompts, one audio session and one file task.
- Whole-draft input commits preserve invalid text for correction and allow finite-invalid saves. Playback and live edits validate at the actual device rate, independently of export settings.
- Workbench adds Undo remove, stable transport controls and requested/realized pulse widths. Equivalent live settings update revision facts without an audio transition.
- Icons have separate proprietary terms: unchanged art may accompany Auditory Lab distributions; modification or independent reuse requires permission. macOS apps retain legal resources when moved.
- macOS Quit shares the save/stop decision; file-panel Stop targets captured audio. Disabled accessibility presses, sliders, selection, layout and selected-rate inspection are corrected.
- Intel macOS CLI signatures are checked after extraction; downloads remain ad-hoc signed and unnotarized. Existing recipe/WAV output paths fail before rendering or writing.
- Contributor lint exceptions use scoped, reasoned attributes; check-lints and its manifest are removed. Vendor review authenticates source patches, and CI shards mutation checks.

## [0.4.0] - 2026-10-21

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.4.0).

- macOS archives use tar.gz; update ZIP extraction scripts. Archives add COPYRIGHT-library.html; retain it with dependency notices when redistributing.
- Desktop drawing requires OpenGL, including OpenGL 2.0+ on Windows. Software-renderer recovery is unavailable; install a compatible driver.
- Integer-phase synthesis changes sample bits; regenerate sample-exact references. Pulses round widths to frames, reject widths above 2^36 frames and require a closed frame per period.
- Rust packages become auditorylab-core, auditorylab, auditorylab-cli and auditorylab-gui. WAV/device APIs join auditorylab; update imports, observation consumers and error matches.
- Source builds require Rust 1.99.1 and native MSVC tools on Windows. Linux uses system fontconfig and Pango/GTK 3/GIO for desktop fonts; install the tagged source prerequisites.
- --build-info and build-kind markers are removed; use --version or About. Playback start/status/completion use stderr; update identity and redirected-output consumers.
- Workbench edits up to 16 tone/click components live with ramps, glides and crossfades. Save custom schema-2 recipes or stop audio to export finite WAVs; Pattern explained shows static timing.
- Persistent transport, native typography and accessible scrolling preserve pending edits. Output captures retain source, rates, levels and Hold context; motion preferences control live drawing.
- Saves retain newer edits; Stop and file cancellation are independent. Export requires idle audio. Native dialogs select paths, and Quit protects dirty content and waits for workers.
- Windows failures use a native dialog, macOS shortcuts honor native modifiers, and ALSA fault polling is bounded. Finite Stop fades; live Stop may finish a transition before fading.
- cargo xtask replaces Python release tooling and generates product-specific notices. Release verification checks native archives and attestations; publication requires explicit authorization.

## [0.3.0] - 2026-10-16

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.3.0).

- macOS packages require version 15+ on Apple silicon and Intel; users of older systems must upgrade to run them.
- The CLI executable becomes auditorylab, installed from crates/auditory-cli. Replace auditory-lab invocations and archive-name assumptions; no executable alias is provided.
- describe/recipe replace about/show. Describe, inspect, play and export require a built-in ID, --file or --stdin explicitly; update callers. Recipe accepts only a built-in ID; schema 2 is unchanged.
- Recipe paths admit only regular files and symlinks to them; use --stdin or load_reader for streams. FIFOs, devices and directories reject before reading; stalled filesystems have no read deadline.
- Rust application workflows own WAV publication; the encoder accepts a writer. Rendering spans, parameter diagnostics and device observations change; update callers and exhaustive matches.
- A native desktop provides research details, validation, WAV export, Play/Stop and generated-signal monitoring. Close/Quit waits for workers; charts describe generated samples, not acoustic output.
- Desktop packages include an unnotarized ad-hoc signed macOS app, console-free Windows executable and optional Linux launcher. Enabled accessibility does not establish screen-reader qualification.
- Recipe/WAV export stages and syncs complete output without replacement. Cancellation cleans owned stages; abnormal termination can leave one, and file sync does not guarantee directory durability.
- CLI help succeeds without arguments; reports distinguish requested/resolved facts. AM titles state 39.0625 Hz, and source adaptations retain calibration limits without changing catalog samples.
- Both frontends share application workflows and build identity. Package checks inspect extracted binaries, notices and architectures; portable arithmetic remains stable across GUI feature unification.

## [0.2.0] - 2026-10-15

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.2.0).

- Windows source builds use native Rust GNU LLVM hosts with rust-mingw and a native --target, replacing MSVC. x86_64 requires SSE3, CMPXCHG16B and LAHF/SAHF.
- Recipes require schema 2, peak_dbfs and optional [about] metadata; schema 1 and old names reject. Rust callers must update Preset/About fields and exhaustive error matches.
- inspect omits research context; use about PRESET. show NAME --output FILE saves UTF-8 built-ins to a new destination without shell encoding changes; UTF-16 inputs receive an explicit diagnostic.
- Rectangular modulation adds gated tones. The catalog adds Martorell 8/40/80 Hz tone trains and Lahijanian 40 Hz tone pips, with study findings, limitations and adaptations.
- Natural catalog ordering handles numeric names. Archives contain the executable, README.txt, LICENSE and consolidated notices; replace INSTALL.txt/USAGE.txt and notice-directory assumptions.
- macOS deployment minima become 11 on Apple silicon and 10.12 on Intel; testing runs on macOS 15 and does not establish execution on the older minima.
- Windows binaries use MinGW/LLVM runtime code and system UCRT, requiring Windows 10+ on x86_64 or Windows 11 on ARM64. Redistribute the bundled runtime notices.
- Release tools require Python 3.9+. Publication checks uploaded digests and refuses automatic draft recovery; published versions must not be replaced.
- Native package checks bind runtime-link evidence to binary hashes; they do not establish Windows Ctrl-C/Ctrl-Break behavior after removal of the packaged-interruption harness.

## [0.1.0] - 2026-10-14

[Published release notes](https://github.com/resoltico/AuditoryLab/releases/tag/v0.1.0).

- First release: a CLI for inspecting, playing and exporting stereo research-inspired sounds, with editable TOML recipes and an attributed built-in catalog.
