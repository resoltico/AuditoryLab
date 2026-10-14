# Verification

Run checks from the checkout root. [Quick start](../README.md#quick-start) supplies compiler/audio-build prerequisites. [quality.yml](../.github/workflows/quality.yml) pins external verification-tool versions and defines CI jobs; [rust-toolchain.toml](../rust-toolchain.toml) pins Rust.

## Standard checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
```

These do not open an audio device: physical tests are ignored by default. Check nonempty test discovery, ignored cases, real failure output and expected results; an exit code alone is limited to its command's criteria.

Install matching audit/workflow-linter versions from the workflow's `CARGO_AUDIT_VERSION`, `ACTIONLINT_VERSION` and `SHELLCHECK_VERSION`, then run:

```sh
cargo audit --deny warnings
SHELLCHECK_OPTS=--enable=all actionlint -shellcheck "$(command -v shellcheck)" .github/workflows/*.yml
```

CI configures workspace checks on macOS/Linux/Windows, dependency auditing and workflow linting. The scheduled job checks advisories; push/PR jobs also run workspace, native Windows and scoped mutation checks. Local checks do not establish hosted CI or untested native-platform results.

## Physical playback checks

For `auditory-device` changes, run on a compatible available default output:

```sh
cargo test --locked -p auditory-device --test default_output -- --ignored --nocapture
```

The two tests use no-component silence, assert a zero preview, then exercise stream completion/cancellation. They report the device snapshot, backend result and elapsed time. A stereo silent stream does not establish distinguishable ear routing, another channel/rate configuration or acoustic timing. Pure callback tests cover distinguishable channel placement independently.

Record an unavailable check when no compatible default output exists. Do not change devices, routing or volume to make a test run, or use audible/acoustic tests without separate authorization.

## Native Windows interrupts

With the release executable built, use a dedicated Windows process running [PowerShell 7 or later](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows). CI checks the version before Windows verification; the standalone helper rejects older versions before running:

```powershell
pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/test-windows-interrupts.ps1 `
  -Binary target/release/auditory-lab.exe -Scratch target/windows-interrupt-tests
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

The helper uses isolated native consoles/bounded pipes, never broadcasts to the caller's console and opens no audio. Keep it in a dedicated child `pwsh` process; never dot-source it into your terminal or the CI controller. Its process-scoped policy changes no persistent policy and does not override Group Policy. Check all six event/phase cases, quote controls, statuses, file/capture results and cleanup.

Challenge the relevant signal guards in a disposable source copy: ineffective second handler, immediate first exit, early handler installation and omitted export cancellation must be detected. Missing facilities, blocked policy or omitted cases are unavailable or failing evidence, not a pass. Syntax/C# compilation or cross compilation alone does not exercise native events.

## Scoped mutation testing

[cargo-mutants](https://mutants.rs/) checks detection of production-code changes. Locally, use a short single-file selection for a concrete coverage question. Resolve its pinned version from the workflow and list before compiling:

```sh
mutation_tool_version=$(sed -n 's/^  CARGO_MUTANTS_VERSION: "\([^"]*\)"$/\1/p' .github/workflows/quality.yml)
test -n "$mutation_tool_version" || { echo "CARGO_MUTANTS_VERSION was not found in quality.yml" >&2; exit 1; }
mkdir -p target/tool-scratch
TMPDIR="$PWD/target/tool-scratch" CARGO_BUILD_JOBS=2 env -u CARGO_BUILD_TARGET_DIR \
  cargo install cargo-mutants --version "$mutation_tool_version" --locked \
    --root "$PWD/target/tools" --target-dir "$PWD/target/tool-build"
PATH="$PWD/target/tools/bin:$PATH" cargo mutants --list \
  --file crates/auditory-core/src/generator.rs --re 'replace < with <= in Carrier::next'
```

Reuse an available matching tool rather than reinstalling it. This selection targets the half-open click edge. Run in a fresh owned directory; reusing output can rotate/delete reports:

```sh
mkdir -p target/mutations
mutation_run=$(mktemp -d "$PWD/target/mutations/generator.XXXXXX")
mkdir "$mutation_run/scratch"
PATH="$PWD/target/tools/bin:$PATH" TMPDIR="$mutation_run/scratch" \
  env -u CARGO_TARGET_DIR -u CARGO_BUILD_TARGET_DIR \
  cargo mutants --file crates/auditory-core/src/generator.rs \
    --re 'replace < with <= in Carrier::next' \
    --package auditory-core --test-workspace=false --copy-target=false \
    --jobs 1 --jobserver-tasks 2 --timeout 60 --build-timeout 60 \
    --cargo-arg=--locked --output "$mutation_run"
```

Require a successful baseline, nonempty selection, zero missed/timeouts and inspected compiler reasons for unviable cases. A timeout or incomplete run proves no detection. Preserve `mutants.out/outcomes.json` and useful logs. Never weaken meaningful guards to produce a passing record.

[Configuration](../.cargo/mutants.toml) selects the non-incremental/debug-free profile and lint cap. Clearing inherited target overrides keeps each worker's output under its owned TMPDIR; `--copy-target=false` avoids cloning unrelated builds. Keep local selections sequential with one worker for a tiny selection.

CI selects `generator.rs` and `session.rs` in separate jobs, with two workers/shared compiler tasks, 60-second build/test limits and a 15-minute job limit including installation. Ordinary tests cover the other crates. No whole-workspace campaign is permitted.

## Output and resource ownership

Keep build/probe/mutation/report output under `target/`, with TMPDIR or native TEMP/TMP pointing to an owned subdirectory for filesystem checks. `cargo clean` removes reports too; preserve useful evidence outside the build directory before cleaning. Shared registries/toolchains are separate resources and must not be deleted as task scratch.

Worker/time limits bound concurrency and execution, not disk bytes. Measure the selected build/workers on the current toolchain and dependency set, leave working headroom, and reduce simultaneous workers if needed. A hard byte limit requires an actual filesystem quota. Remove only owned expendable scratch after retaining results; avoid redundant permanent build directories or setup wrappers.
