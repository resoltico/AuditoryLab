# Verification

Run checks from the repository root. Linux source builds require the checked Cargo system-FreeType overrides and security-updated distribution FreeType/Fontconfig development packages; do not bypass root configuration with outside-root `--manifest-path`, alternate configs or compiler/linker flags. [Install from source](../README.md#install-from-source) lists the compiler and audio-library prerequisites. [quality.yml](../.github/workflows/quality.yml) defines the CI jobs and pins the external tool versions.

## Standard checks

The release tools need Python 3.9 or later, Git for source/patch verification, and the Python standard library only; [.python-version](../.python-version) selects Python in CI. Initialize temporary output before running the checks. In a POSIX shell on macOS or Linux:

```sh
mkdir -p target/verification-temp
export TMPDIR="$PWD/target/verification-temp" TEMP="$PWD/target/verification-temp" TMP="$PWD/target/verification-temp"
export PYTHONDONTWRITEBYTECODE=1
```

In PowerShell on Windows:

```powershell
New-Item -ItemType Directory -Force target/verification-temp | Out-Null
$env:TMPDIR = (Resolve-Path target/verification-temp).Path
$env:TEMP = $env:TMPDIR
$env:TMP = $env:TMPDIR
$env:PYTHONDONTWRITEBYTECODE = "1"
```

On Windows, use `python` in place of `python3` and the native GNU LLVM host from the [source setup](../README.md#install-from-source). Append `--target x86_64-pc-windows-gnullvm` to the `cargo clippy`, `cargo test`, `cargo doc` and `cargo build` commands below; on ARM64 use `--target aarch64-pc-windows-gnullvm`. Keep the explicit target even when it matches the host, so target linker flags do not apply to host proc-macro builds. `cargo fmt` needs no target.

Before the Windows workspace checks, prepare the GUI's reviewed system-import material:

```text
python scripts/windows_runtime.py prepare --target x86_64-pc-windows-gnullvm
```

Use `aarch64-pc-windows-gnullvm` on Windows ARM64. This is also the prerequisite for ordinary GUI `cargo test`; the authoritative native qualifier prepares and validates it automatically before any GUI compilation. See the [source-build contract](desktop.md#build-and-start).

Then run:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo test --workspace --release --locked
cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
python3 -m unittest discover -s tests -v
python3 scripts/notices.py --check
```

None of these checks opens an audio device; device tests are ignored by default. Read the test counts and failures, not just the exit status. Confirm discovery of `auditory-app/tests/frontend_boundary.rs` and its dependency/source rejection controls, and the facade public-consumer tests. Unix signal tests and the native Windows console/process-group harness must use deadlines and reap children; a result on one platform does not establish the other.

With the versions named in `quality.yml` installed, CI's audit and workflow lint use these commands in a POSIX shell:

```sh
cargo audit --deny warnings --db target/advisory-db
SHELLCHECK_OPTS=--enable=all actionlint -shellcheck "$(command -v shellcheck)" .github/workflows/*.yml
```

Quality keeps source-format/Python/notices, minimum-Python, workflow lint, advisory and scoped mutation jobs separate from [native.yml](../.github/workflows/native.yml). The reusable native path is shared by Quality, the read-only development wrapper and release qualification. It covers the checked six targets and both products, with explicit native targets and isolated delivery environments. There is no second final-build/package matrix. Publication remains in its separate release wrapper.

## Product identity and native qualification

Install the freestanding library-check target for the active pinned Rust toolchain before running native qualification. This is separate from the native CLI/GUI target. From the repository root, on macOS/Linux:

```sh
qualification_target="$(python3 -c "import sys; sys.path.insert(0, 'scripts'); from qualify import FREESTANDING_TARGET; print(FREESTANDING_TARGET)")"
rustup target add "$qualification_target"
```

In PowerShell on Windows:

```powershell
$qualificationTarget = python -c "import sys; sys.path.insert(0, 'scripts'); from qualify import FREESTANDING_TARGET; print(FREESTANDING_TARGET)"
rustup target add $qualificationTarget
```

Run the authoritative native entry point after source inputs are stable:

```sh
python3 scripts/qualify.py --target aarch64-apple-darwin --build-kind local --output target/local-packages
```

Use the actual host target from release.py and `python` on Windows. The wrapper includes standard code/Python checks, debug/release workspace tests, explicit offline numerical/budget tests, actual window/renderer tests, isolated per-product builds and final extracted-copy smoke. It retains logs and records under target/qualification-results. Local kind is deliberately unqualified with no SHA. Candidate/release modes require a clean exact committed source; native workflows use their own immutable github.sha, not an arbitrary caller-supplied checkout. Local verification does not establish remote qualification or authorize publication.

Inspect actual archive membership and extracted build info. CLI keeps four regular files; CLI and GUI share the macOS 15.0 deployment baseline on both architectures. Check the actual Mach-O minimum for each product and the GUI app bundle's matching LSMinimumSystemVersion; archive instructions derive from the same policy. [package.py](../scripts/package.py) checks native imports and the Windows/Linux compatibility policies for each product. Windows GUI subsystem, product-specific native link receipts, source-bound import libraries and exact imports remain separate from CLI policy. Ordinary workspace compilation does not prove each product's deployment floor, and a declared floor is not execution on that minimum OS. See [releasing](releasing.md).

CLI smoke verifies canonical identity, the controlled two-part listing against all source IDs/durations/titles, raw/saved recipe bytes, explicit file inspection, byte-exact silent WAV and no-overwrite. GUI no-display build-info/package checks verify facade loading/inspection/one-frame export without a window/device. The real Cargo links-override fixture must pass with its panic build script suppressed and fail after override removal, including cache reuse. Linux GUI package checks require actual system FreeType/Fontconfig DT_NEEDED entries and retain pkg-config/distro package versions, including the dynamically loaded libxkbcommon0/libxkbcommon-x11-0 keyboard libraries; native Linux CSD behavior remains separate evidence. Native window smoke compiles its test executable before runtime SLINT_* settings, then records actual FemtoVG/WGPU and forced-software first-frame evidence with lazy instrumentation. A backend header alone does not prove a frame; the FPS reporter marker is required for both routes. CPU/name overrides are removed. Software fallback is labeled WGPU unavailable. A WGPU frame alone does not identify the adapter; record hardware_adapter as unverified unless independently established, including on virtual/CPU Vulkan drivers. Linux Xvfb is declared virtual display evidence.

The native signal-geometry fixture uses the production generated MainWindow and shared presentation mapper, with explicitly labeled test-only anchors/gap markers and an asymmetric/silent-ear view from the public analysis facade. It captures three logical sizes at scales 1 and 1.25 for requested WGPU and forced software, then runs an independent pixel oracle over all 36 captures. The oracle checks shared trace/axis bounds, fixed reference amplitudes, separate ears and missing intervals. A deliberately misaligned rendered layout must fail the same oracle; a compile failure or flat-silence screenshot is not a geometry rejection test. Runtime renderer/scale variables are supplied only after compiling the nonshipping fixture. The macOS fixture sets its own native minimum for each case, so its window can extend beyond the screen’s work area. Captures verify renderer readback; full physical screen presentation requires separate acceptance.

Actual native keyboard/focus, selectable/copyable research/errors, resizing/scaling/theme, parented chooser/manual paths, import/inspect/export/refusal and Stop/close remain separate acceptance. Check accessibility roles/names/values/state/focus/reading order and native assistive technologies where available; enabled AccessKit or a mock backend is not screen-reader qualification. Native macOS termination fixtures cover ordinary Quit, pending worker/join and modeled-dialog holds plus a rejection control against the unpatched backend. Force termination and OS logout negotiation are not covered. Never start audible built-in playback or change output devices/volume to satisfy a test.

Check common identity cache transitions: distinct CLI/GUI products in one target directory, source A to B, candidate to local and candidate to release. Partial/malformed identity inputs reject and Cargo TARGET remains authoritative. Product/source/target/kind mismatch must reject even at the same semver. Source provenance/hashes bind reviewed bytes, not review quality or arbitrary-binary authenticity.

For isolated CLI installation use `cargo install --locked --path crates/auditory-cli --root target/verification-install`, with the explicit native Windows target where required. Inspect bin membership and run auditorylab from that owned root; do not replace a user's installed command. GUI app bundles are produced by packaging, not Cargo's ordinary executable installation.

## Signal observation and computation

Retain pinned PCM digests and independent formulas, long/late-index phase and pulse/fade checks, composition/partition/bound tests and independent WAV decoding. Observation enabled/disabled must preserve generated PCM, channel conversion, timing and terminal results. Test authoritative RenderedSpan boundaries including empty calls and silent tails, independent per-ear min/max/bin boundaries, short positive pulses, unequal/silent ears, awkward rates and incomplete buckets. Missing samples remain gaps, not silence.

Exercise queue full/disconnected/unconsumed and stalled-reader recovery, fixed per-poll work, fresh post-stall coverage, source switching/restart, and actual type/capacity accounting across the supported rate range. Verify the 8 MiB product-owned monitoring cap, bounded frozen/latest views and low-rate insufficient-sample behavior; this is not process RSS or a limit on arbitrary consumer clones. Keep callback allocation/locking controls meaningful for enabled and disabled observation and all terminal/tail/failure paths.

The native qualifier explicitly runs the offline long-phase, longest-built-in stream, production callback/conversion, paired observed-callback and maximum-rate app observation budget tests described in [signal-verification](signal-verification.md); it never invokes every ignored test. Reports must be nonempty, use verified active frames and actual production conversion/channel placement, and identify source/toolchain/CPU/workload. Timing distributions and deadline ratios are measurements, not shared-runner timing gates or backend/acoustic proof. Compare monitoring overhead and reactive GUI updates using actual GPU/forced-software observations; do not claim lower energy merely from lower CPU.

The Compact session view needs native checks for fixed shared amplitude scale, left/right labels, source/actual rate/coverage, explicit stale/missing intervals, focused/unfocused, app-hidden, occluded, minimized, frozen and reduced-motion behavior, and genuine last samples after Stop/failure. The narrow pinned Winit bridge must propagate every native event and retain no native window object; focus/occlusion only controls presentation. Live views conservatively suspend while unfocused, minimized or reported occluded. Freeze display does not pause audio. Neither latest-generated cursor nor chart coverage is an audible playhead or a substitute for the actual terminal result.

## Device checks

After changing playback code in `auditory-device`, run its device tests on an available default output:

```sh
cargo test --locked -p auditory-device --test default_output -- --ignored --nocapture --test-threads=1
```

On Windows, after the [native host setup](../README.md#install-from-source), pass the target before the test arguments:

```text
cargo test --locked -p auditory-device --test default_output --target x86_64-pc-windows-gnullvm -- --ignored --nocapture --test-threads=1
```

On ARM64, use `--target aarch64-pc-windows-gnullvm` instead.

The checks run serially so their native streams do not compete for the default output. They play silence only: one test completes a second of it, the other cancels a long one. Silence cannot show ear routing or acoustic timing; unit tests check channel placement. If no compatible output exists, record the check as not run. Do not change devices, routing or volume to make a test run, and never play audible sound in tests.

## Mutation testing

[cargo-mutants](https://mutants.rs/) shows whether tests detect changes to production code. Locally, run it only on one file, ideally with a narrow `--re` selection, to answer a specific question about coverage; never on a whole crate or the workspace. Install the version pinned as `CARGO_MUTANTS_VERSION` in `quality.yml` under `target/`, list the selection before running it, and keep all scratch under `target/`, because an interrupted run leaves its copies behind. This example uses a POSIX shell:

```sh
version=$(sed -n 's/^  CARGO_MUTANTS_VERSION: "\(.*\)"$/\1/p' .github/workflows/quality.yml)
mkdir -p target/mutations/scratch
export TMPDIR="$PWD/target/mutations/scratch" TEMP="$PWD/target/mutations/scratch" TMP="$PWD/target/mutations/scratch"
cargo install cargo-mutants --version "$version" --locked --root target/tools --target-dir target/tool-build
export PATH="$PWD/target/tools/bin:$PATH"
cargo mutants --list --file crates/auditory-core/src/generator.rs --re PulseTrain
cargo mutants --file crates/auditory-core/src/generator.rs --re PulseTrain \
  --package auditory-core --test-workspace=false --copy-target=false --jobs 1 \
  --timeout 60 --build-timeout 60 --cargo-arg=--locked --output target/mutations
```

[.cargo/mutants.toml](../.cargo/mutants.toml) selects a lean build profile and caps lints, so a mutant that leaves a variable unused is still built and tested. A run counts only with a passing baseline, a nonempty selection and every mutant caught or its survival explained; a timeout proves nothing. Never weaken a guard to make a run pass. CI runs `generator.rs` and `session.rs` as separate single-file jobs.

## Output and disk

Keep build, probe and report output under `target/`, which `cargo clean` removes; initialize TMPDIR, TEMP and TMP there for tools that write elsewhere. Copy evidence you want to keep out of `target/` first. Remove only scratch you created.

CLI regression checks cover exact help precedence, explicit source conflicts, native equals-form paths, tiny/huge duration rendering, declared/resolved fade, raw-versus-human writer controls and private bound report context. Package listing checks validate the controlled human layout, not a machine-output contract.

Native qualification also requires Cargo to discover the admitted essential `auditory-app` integration targets for architecture, workflows, typed diagnostics, observation, build identity and file admission. It explicitly lists and runs each target, requires named positive and rejection controls, and compares every discovered test with successful, nonignored and unfiltered execution. A successful workspace suite cannot substitute for these checks: disabling automatic integration-test discovery or omitting a required control rejects qualification. The runner also requires the admitted GUI binary unit target, including allocation-capacity, presentation, permit/result and close/join rejection controls; `test=false` cannot remove them. App library capacity controls run individually by exact name, requiring one successful nonignored execution each while leaving the intentional timing workload to its separate explicit command. The runner does not freeze the total workspace test count.

The source and native qualification gates compile `auditory-core` for the freestanding target declared by `FREESTANDING_TARGET` in `scripts/qualify.py`. They also compile an isolated no_std consumer with a std-bearing dependency and require the compiler to reject that dependency. This checks the selected dependency boundary beyond the presence of `#![no_std]`; it does not certify every possible downstream feature configuration. CI installs the target before running the gates.
