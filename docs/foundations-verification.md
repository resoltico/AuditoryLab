# Foundations implementation and verification

This is the historical foundations verification record. The later persistent-library work replaces its Unreleased schema-3 shape and persistence behavior; see [current recipe format](recipes.md), [architecture](architecture.md#persistent-library-and-external-files) and the retained `target/persistent-library-handoff/` evidence. The historical failure and timing observations below remain evidence for their stated build, not qualification of the current tree.

Initially implemented on 2026-10-23, audited and corrected on 2026-10-24, against reviewed HEAD `978986ff5763e446151840f6b28fafaea1b5d346`. The initial working tree was clean and had no commit drift. The handoff was listed before extraction, extracted into a fresh `target/tmp/foundations-handoff.qYY0Jh/` directory, and every `SHA256SUMS` entry verified. Live instruction files governed the work; historical source copies were not installed over the checkout.

The initial implementation closeout overstated completion: the 2026-10-24 audit found missing search fields, frontend playback policy, uncached readiness compilation and a missing command-driven file execution seam. All four findings are now corrected and have regression evidence below. Current functional and ARM64 delivery checks pass. The final native-field-remediation callback run fails the unchanged headroom criterion in three of 24 configurations, with two nominal deadline misses. An earlier single passing run, earlier failures and the native backend underrun remain material evidence; stable real-time/release readiness is not established. Five other native target configurations and full upstream Parley fixtures remain unverified. These are local development packages from a dirty working tree, reporting the existing HEAD/build identity, not published artifacts. The audit-correction evidence lives under `target/foundations-corrections/`; the original evidence remains under `target/foundations-qualification/`. The final native-field-remediation results at the end of this document supersede the earlier pass counts and timing result.

## Audit correction acceptance

| Finding | Corrected path and independent control |
| --- | --- |
| Search dropped own notes and retained source context | Shared search indexes `presentation::document_notes`, whose source fields remain explicitly attributed. Tests cover direct research, custom/current/source notes, saved/reopened derivatives, same-origin reimport replacement, empty results and unrelated nonmatches. The original public-API probe now returns both previously missing results. |
| GUI repeated playback policy and literal wording | `ActionView` supplies Library label and availability; shared sizing labels live in application presentation. Slint no longer derives command admission from a global busy flag. Four production-binding tests activate the visible accessible button during real controlled Save/Import launches and verify disabled prompt/export/pending/capability cases. Hidden label-measurement widgets are excluded from test actions. Adjacent Import/New/copy/preview/export/rate gates were reviewed; copy readiness and execution share their existing component-limit check. |
| Action readiness recompiled selected sounds outside the view cache | Snapshot clones share at most two rate-specific immutable `Signal` successes or typed errors. Actual compilation counters, rather than library-view construction counts, prove one compilation across rows/details/repeated readiness/export, error reuse, bounded eviction and invalidation on rate/new identity/reimport/source changes. Fresh admission remains separate from immutable realization reuse. |
| File tests bypassed command capture and launch | `spawn_file` and `spawn_audio` are focused internal launch seams. `FileKind::execute` is shared by real workers and controlled real-thread fixtures. New tests enter Save/Import/Export and path/close commands before controlling launch failure, actual filesystem result, exit, panic and missing-result. The panic-after-publication close test no longer substitutes a manufactured task. Result-before-exit, edits during panel/save, close-after-save join and dependency-specific failures use this boundary. Existing low-level policy and uninstrumented integration tests remain additional evidence. |

`workspace::testing` is compiled only in unit tests or with the nondefault `test-support` feature, enabled by GUI dev tests. It is not a new production runtime or command bus. Normal product builds use the real launch adapters; isolated default consumers still exclude CPAL.

## Breaking schema and Rust APIs

Schema 3 is the only reader/writer format. `Recipe` adds `source`; `About.summary` is current plain notes, while research fields live in `About.research: Option<Research>`. `Source` holds original stimulus/description without recursion. Existing schema-2 files are rejected with the supported version in the error; no compatibility reader or migration is present.

`Workspace::library_view` returns a shared immutable `Arc<LibraryView>`. `Refusal::Invalid` carries title/rate/optional typed `StimulusError` instead of a preformatted string, and `Refusal` implements `PartialEq` rather than `Eq`. `default_carrier` derives control defaults at runtime rather than remaining a const function. All in-repository consumers compile against these interfaces.

## Decisions challenged

- A citation-only record cannot support reopened source navigation. Schema 3 stores one nonrecursive original stimulus and its description/research; repeated derivatives retain that anchor and their own notes. Serialization checks finite current/source numbers and the entire UTF-8 byte bound before publication. Finite acoustically invalid source content remains inspectable and serializable.
- Readiness cannot commit staged input or allocate snapshot identities. Operation preparation runs before commitment, using the same admission and validation as execution. Export acoustic failures retain their title, rate and typed domain error until shared formatting. Dialog replies recheck admission while retaining invocation-time captures.
- Results do not prove thread exit. The existing worker/join boundary remains authoritative, now with fallible thread launch and Export sound content enforced by construction. Shutdown preserves launch failure messages as well as joining both slots.
- A broad draft revision is not an acoustic dependency. Analysis keys include numeric bit patterns, component identities/routing/kinds, envelope and rate; equivalent input text and description changes reuse analyses. Immutable library projections depend on visible snapshot identities, selection and rate; GUI library rows update only for a changed projection.
- Domain component identity may survive Undo; renderer tokens cannot. Removed/replaced mappings retire before model mutation. Undo allocates a new token; exhaustion retires callbacks and displays recovery guidance.
- Verified archive comparison confirmed the six existing patch sets. Parley's deprecated ICU numeric accessor only implemented local set membership. Explicit distinct class bits preserve that property without depending on ICU's private numeric representation. The exact production membership file is tested by xtask.
- Measured deadline misses do not justify changing samples or removing renderable rates. The new timing command has an explicit optional headroom criterion and preserves failing results; hardware scheduling and physical effects remain separate questions.

## Work-order traceability

Paths below are relative to the repository root. Current command output is retained under `target/foundations-corrections/`. Explicitly historical logs remain under `target/foundations-qualification/`.

| Order | Implementation | Verification evidence |
| --- | --- | --- |
| W01 | `crates/core/src/{recipe,parameter}.rs`; deliberate application reexports in `crates/app/src/lib.rs`; operation/worker/view modules; GUI callback modules; `docs/architecture.md` | `workspace-final.log`, `doc-final.log`, `portable-final.log`; default consumer excludes CPAL, enabled control includes it |
| W02 | Schema 3 `About`/`Research`/`Source`; `crates/app/src/{recipe,editing}.rs`; complete-content saved equality and source navigation; all supported fixtures/catalog and xtask smoke input | Core strict-source/prior-schema tests; derivative/reopen/source-file-removal tests; Unicode notes, repeated copies, exact 65,536-byte neighbour tests, nonfinite anchors and unchanged destinations in `workspace-final.log` |
| W03 | `crates/app/src/workspace/operations.rs`; shared Save/Export/audio preparation and typed acoustic refusals; current admission at commands/path replies; immutable captures | `readiness_is_pure_and_failed_preparation_keeps_all_staged_text`, captured-save/dirty tests, actual-device-rate disagreement tests and admission suite in `workspace-final.log` |
| W04 | `workspace/tasks.rs` owns fallible worker launch/events/join and captured `FileKind::execute`; parallel file/audio launch seams have command-driven controls in `workspace/testing.rs`; Export carries `Sound`; workspace owns decisions and shutdown | Real Save/Import/Export/dialog/close commands control launch failure, actual publication/result, exit, missing result and panic. Save-before-close waits for join; panic after actual publication cannot approve close; unrelated file failure cannot block replacement audio. Existing real thread/filesystem/publication tests remain in `workspace-final.log` |
| W05 | Component vocabulary; shared controls/monitor choices/defaults, Library Listen/replacement labels and sizing labels, and operation-specific UI availability; CLI rate derives `SampleRate::DEFAULT`; physical core help separated from finite/live application explanation | Actual visible accessible Library buttons are tested with controlled Save/Import workers and prompt/export/pending/capability refusals; copy readiness/execution share the component-limit guard; view/CLI/conversion/accessibility suites and `clippy-final.log` |
| W06 | `crates/gui/src/adapter/{editing,actions,library,auxiliary}.rs`; declarative `ui/{models,editor,monitor,about}.slint`; bounded retired bindings and visible fail-closed exhaustion | 43 GUI tests: 100 remove/Undo cycles, retired callbacks, exhaustion, row/focus/caret identity, clipped fields, fonts, keyboard, accessibility and window geometry; native observations below |
| W07 | Numeric-input analysis keys in `editing.rs`; two-rate immutable snapshot signal/error memos shared by details/readiness/export; immutable library projection cache in `workspace/views.rs`; identity-based GUI library projection; existing telemetry/held-capture dependency keys retained | Actual realization/compilation counters cover repeated readiness, errors, eviction, rate and reimport/source invalidation, with positive controls; description/equivalent-text and GUI-library counters, idle telemetry and held captures in `workspace-final.log` |
| W08 | `third_party/patches.toml`, six exact unified diffs, `xtask/src/vendor.rs`, production `parley/src/bidi_mask.rs`, maintenance/retirement guidance and normal CI verification | `vendor-final.log`: six authenticated archives/patch/tree comparisons and 12 product/target graphs; corrupt-archive/missing/unexpected-file/wrong-source controls and production bidi bits in xtask's 24 tests; `../foundations-qualification/winit-termination.log`; Parley limitation below |
| W09 | Shared citation/DOI records in `crates/core/studies/`; per-condition sound/methods/adaptations in `catalog/`; build generates standalone recipes; Unicode/quotes/multiline support; unrelated fixed test counts removed; search covers current notes and attributed retained context | Public audit reproduction now reports one result for own notes and retained citation; direct/custom/reopened/source-notes/reimport/nonmatch search regressions pass. Complete discovery and independent numeric oracles retained; shared-source consistency and text checks; actual CLI escaping/roundtrip and exact raw recipe-byte tests in `workspace-final.log` |
| W10 | Correct portable edited-sample export; structural/allocation/lifecycle assurance retained; sustained callback benchmark and dedicated manual CI; source-bound package smoke fixture updated; current guides/changelog updated | Commands below; 334 workspace tests, 99 release core tests; silent hardware/CLI tests; two inspected ARM64 archives; final synthetic pass and all earlier timing failures/native limits explicitly retained |

## Audit-correction commands and outcomes (before native field remediation)

Temporary test paths used an absolute repository `target/tmp` TMPDIR. Test totals below overlap across commands; they are not counts of unique tests.

| Command | Exit / actual result | Log |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | `fmt-final.log` |
| `cargo clippy --workspace --all-targets --locked` | 0 | `clippy-final.log` |
| `cargo test --workspace --locked` | 0; 334 passed, 6 ignored across 19 reported selections; includes 43 GUI and 24 xtask tests | `workspace-final.log` |
| `cargo test -p auditorylab-core --release --locked` | 0; 99 passed, no ignored tests across 9 reported selections | `core-release.log` |
| `cargo doc --workspace --no-deps --locked` | 0 | `doc-final.log` |
| `cargo deny check` | 0; advisories/bans/licenses/sources pass; existing duplicate-version and yanked `clipboard-win 5.4.1` warnings remain | `deny.log` |
| `cargo check -p auditorylab --no-default-features --features test-support --locked` | 0; controlled execution fixtures build without the device feature; also wired into CI | `test-support-final.log` |
| Build default library and compile/run original `target/tmp/foundations-audit/search-probe.rs` against its actual emitted rlib | 0; initial=1, own notes=1, retained source citation=1 | `search-probe.log`, `probe-build.jsonl` |
| `actionlint` | 0; validates CI, release and manual performance workflows | `actionlint-final.log` |
| `cargo xtask verify-vendor` | 0; all six patches and both products' six-target graphs verified | `vendor-final.log` |
| Copy root lock; `CARGO_TARGET_DIR="$PWD/target" cargo run --manifest-path consumers/portable/Cargo.toml` | 0; exported 14,400,000 frames and checked delivered samples against the edited signal | `portable-final.log` |
| Default consumer `cargo tree … -e normal,build --target all -i cpal` | Expected 101; exact no-matching-package diagnostic, not a metadata failure | `portable-no-cpal.log` |
| Same tree with `--features auditorylab/device-playback` | 0; CPAL is present | `portable-cpal-control.log` |
| `cargo test -p auditorylab --features device-playback --test default_output --locked -- --ignored --nocapture` | 0; all 4 silent default-output tests passed | `hardware-silence-final.log` |
| `cargo test -p auditorylab-cli --test cli --locked -- --ignored --nocapture` | 0; 1 silent native playback test passed, 7 other tests filtered by selection | `cli-hardware-final.log` |
| `CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/winit/Cargo.toml --locked --lib termination` | Retained prior result: 0; 2 native termination tests passed, 7 unrelated tests filtered; unchanged patch not rerun | `../foundations-qualification/winit-termination.log` |
| `cargo test --manifest-path third_party/parley/Cargo.toml --locked --lib bidi` | Retained prior result: 101; upstream sources omit `parley_dev`; no upstream tests ran; limitation unchanged | `../foundations-qualification/parley-bidi.log` |
| `cargo run --locked -p auditorylab-core --example render_cost --release` | Retained prior result: 0; observational report, no deadline assertion | `../foundations-qualification/render-cost.log` |
| Release callback prebuild, then `AUDITORYLAB_PERF_MAX_BUDGET_PERCENT=75 cargo test --locked -p auditorylab --features device-playback callback_monitor_costs --release -- --ignored --nocapture` | 0; 1 selected test passed, 149 other unit tests filtered; 24 measured configurations meet criterion, zero deadline misses; historical failures remain below | `callbacks-controlled-final.log` |
| `cargo xtask dist --target aarch64-apple-darwin --icons target/release-0.5.0-qualified-icons --out target/foundations-corrections/packages` | 0; CLI and GUI archives inspected; catalog, one-frame RIFF and no-clobber smoke checks pass | `dist-final.log` |
| `cargo xtask inspect-package aarch64-apple-darwin target/foundations-corrections/packages/auditorylab-gui-0.5.0-macos-aarch64.zip` | 0; explicit repeat of GUI package inspection | `gui-package-inspection-final.log` |

The six default-ignored tests comprise the callback timing test, four default-output hardware tests, and one native CLI playback test. All were explicitly selected separately: five hardware/CLI cases pass; the latest synthetic timing selection passes, with earlier failures retained. Zero-test doctest/feature selections are visible in the logs; no unexpectedly empty acceptance selection is counted as a pass.

The full upstream Parley library test command remains unavailable from the published archive because its development helper is absent (historical attempted command retained, not repeated in this correction pass). The focused replacement assurance includes the exact production bidi-membership file in xtask and the integrated GUI shaping/font/keyboard suites. It does not claim execution of upstream's omitted font-fixture tests. Its isolated compiler output was removed after retaining the failure log.

No synthesis, compilation, live-transition, fact, waveform or timing production implementation changed, and no deterministic sample digest was changed. Schema literals in determinism fixtures changed only to select the current reader. File-scoped DSP mutation runs were therefore not required for this change; existing numerical and allocation tests and their independent oracles remained selected.

## Timing and native scope

Environment is retained in `environment.json`: Apple M4, aarch64, macOS 27.0.1, pinned Rust 1.99.1, release profile. No competing compilation/test load was running during either callback measurement, but the host remained a general-purpose desktop rather than a dedicated real-time runner. The workload replaces all 16 component carriers every callback, using up to 32 outgoing/incoming slots, at 48/768 kHz, 64/256/1024/4096-frame buffers, and no/quantities/waveform monitoring. Long buffers include settled frames after the 30 ms transition. There are 100 warmup callbacks and 3,000 measured callbacks per configuration, reported as three groups of 1,000; timing is synthetic and runs faster than real time.

The original 2026-10-23 final-tree run met 25% measured headroom in 20 of 24 configurations, but failed these four (historical evidence retained):

| Rate / frames / monitor | Maximum ns | Nominal budget ns | Deadline misses |
| --- | ---: | ---: | ---: |
| 48,000 / 256 / quantities | 6,800,542 | 5,333,333 | 1 |
| 768,000 / 64 / quantities | 76,292 | 83,333 | 0 |
| 768,000 / 64 / waveform | 66,333 | 83,333 | 0 |
| 768,000 / 1,024 / quantities | 19,918,625 | 1,333,333 | 29 |

All earlier failures remain evidence: the first original run failed ten configurations with 17 deadline misses; the original final-tree run above reported 30 misses. During the correction pass, a run before the adjacent copy-limit guard failed seven configurations with nine misses (`callbacks-before-copy-guard.log`). The latest corrected-tree run passed the unchanged 25% headroom criterion in all 24 configurations with zero nominal deadline misses (`callbacks-controlled-final.log`, `callbacks-summary-final.json`). No competing build/test load was running during either correction timing run. A single synthetic pass does not erase the previous failures, establish a stable real-time configuration or prove that these application corrections improved callback cost. No supported render rate was removed. The 124 deliberately skipped monitor publications per monitored configuration are lock-contention probes; simulated backend underruns were zero and do not measure physical device scheduling.

Silent hardware tests used the default “Mac mini Speakers”, 48 kHz, two channels, F32, without changing volume or routing. The retained 2026-10-23 manual packaged-app accessibility/native-panel checks covered empty Workbench silence, typed title commitment, coalesced Command-Q, Cancel retaining dirty work, Save-before-Quit publication and actual app termination. A native Save panel exposed its session-bound Stop while component-free silence played; Stop stayed bound to that session and the panel remained open. That manual session reported **one backend underrun**, separately from synthetic deadline misses and deliberate monitor gaps. The correction pass repeated the current packaged app's native component-free Play/Save-panel/Stop, coalesced Quit, Cancel and Save-before-Quit paths. Stop stayed available inside the panel and disabled after request without closing it; Save published; Quit terminated after publication. No new backend-underrun warning was displayed in this smoke session, which does not erase the earlier one-underrun observation. Editing after Stop retained the Last capture's old source title while Next Play used the new title. Current saved schema-3 examples and observations are under `target/foundations-corrections/native/`, and the final CLI inspected both files. The test app was closed; routing and volume were unchanged. Original native evidence remains under `target/foundations-qualification/native/`.

Package inspection covered the actual ARM64 CLI tarball and GUI ZIP, including Mach-O architecture, executable/build identity, resource/legal/source contents, source-bound icon inputs, and extracted executable smoke behavior. Full Xcode icon compilation was not available under the selected Command Line Tools installation; existing icon resources matched the source revision and were validated/reused. Package files retain the current development version/build metadata; nothing was uploaded or published.

Native Linux (X11/Wayland, GNOME/KDE preferences, AT-SPI/compositor behavior), Windows (both architectures, native fonts/input/dialogs), and Intel macOS execution/packages were unavailable on this ARM64 Mac. Their Cargo graphs and structural target/archive-set tests passed; their native behavior and delivered binaries remain unqualified. On each native target, run the contributor checks and `cargo xtask dist --target TARGET` (with matching precompiled macOS icons where needed), then inspect/exercise the extracted products. The authoritative target/product set remains `xtask/src/targets.rs`; collection requires all target products. Real hearing/brain responses, calibrated acoustic levels, other devices/formats/rates/buffer configurations and assistive-technology sessions were not qualified.

## Delivery hygiene

`AGENTS.md` and `AGENTS_EXTRA.md` are unchanged. No instruction supplement changes, migrations, compatibility aliases or release publication were made. Unrelated retained target artifacts were preserved. `target/foundations-corrections/` contains current logs, source fingerprints, command outcomes and packages. `target/foundations-qualification/` retains the original logs, timing failures, native observations and original packages; it sits outside compiler profile directories. Temporary vendor compiler output was cleaned. The original handoff extraction and authenticated upstream inputs remain available for review.

The original 1,407-file source inventory is preserved as `target/foundations-corrections/baseline-source-sha256.json`. Current source inventories are written to `target/foundations-corrections/final-source-sha256.json` and the canonical `target/foundations-qualification/final-source-sha256.json`; the current command ledger is `target/foundations-corrections/command-outcomes.json`. Earlier command/timing/native logs remain historical evidence rather than being relabeled as current checks.

## Native field remediation (24 October 2026)

FT-01–FT-05 address multiline note editing, persistent global feedback, nonfinite retained-anchor admission, concise file-name fallbacks, and main-before-auxiliary startup presentation. Source/binary identities and the independent 170-case CLI / 23-group native replay are recorded outside the repository at `/Users/erst/tools/auditorylab-field-retest-2026-10-24-foundations-07/`. Its `RESULTS.md` is the field closeout; baseline materials at `/Users/erst/tools/auditorylab-field-testing-2026-10-24/` remain immutable. Existing timing/platform limits above remain separate from field acceptance.


The final field candidate combines native focus suspension and first-frame readiness in one Winit event filter; installing a second filter would replace the first. Explicit Details actions raise an existing reader, and standalone monitor controls reapply the authoritative projected choices after native initialization. These corrections retain the captured binding/token and worker/publication contracts.

Final source verification before this documentation-only closeout update passed:

- 340 workspace tests (including 46 GUI tests), with six default-ignored tests; 99 release core tests, none ignored. Reported test selections overlap.
- Formatting, all-target workspace Clippy, workspace docs, dependency checks, vendor verification, actionlint and the no-default-feature test-support build.
- The portable consumer's edited-sample export and both CPAL dependency-graph controls.
- All four separately selected silent default-output tests and the separately selected silent native CLI test.
- All 170 fresh CLI cases and all 23 native GUI groups, including exact LF/Unicode/CRLF notes, source-anchor admission, real publication failure, cancellation cleanup, two verified scale-2 cold starts, modeless Details ordering, Save-before-Quit/exit, and full GUI/CLI WAV byte equality.
- Current ARM64 CLI and GUI package generation, extracted-package inspection, catalog/one-frame RIFF/no-clobber smoke checks. No release publication.

The final timing selection, run after prebuild without competing agent build/test processes, **exited 101**. At 768,000 Hz, these configurations failed the unchanged maximum-cost limit of 75% of the nominal deadline:

| Frames / monitor | Maximum ns | Nominal budget ns | Deadline misses |
| --- | ---: | ---: | ---: |
| 1,024 / waveform | 2,221,959 | 1,333,333 | 1 |
| 4,096 / quantities | 13,867,750 | 5,333,333 | 1 |
| 4,096 / waveform | 4,155,292 | 5,333,333 | 0 |

The other 21 configurations met the criterion. This failed final result was retained without rerunning to replace it. No sample oracle, supported rate or acceptance threshold changed. The synthetic workload does not qualify physical scheduling on this general-purpose desktop.

Native automatic-reflow focus preservation remains unqualified: supplementary resize attempts on the final binary did not reliably reach the intended transition. Cold-start/Hide/Details passes and offscreen reflow/caret tests do not establish that separate native property. An earlier candidate's foreground-monitor observation remains retained. macOS `details_parent` is a no-op, so modeless ordering checks do not establish an NSWindow parent relationship. The unavailable platform, upstream Parley and hardware scopes above still apply.

Final logs and command ledgers are retained in `target/field-remediation/final-verification-07/`, final package archives in `target/field-remediation/packages-07/`, and a copy of the logs plus per-finding/W01–W10 traceability in the external field root's `RESULTS.md`. Its source inventory matches all 1,408 source files before this documentation-only closeout update; its 697 regular evidence hashes were verified separately. The 836 baseline evidence files, both instruction files and all failed/superseded retests remain unchanged.
