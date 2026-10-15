# Verification

Run checks from the repository root. [Install from source](../README.md#install-from-source) lists the compiler and audio-library prerequisites. [quality.yml](../.github/workflows/quality.yml) defines the CI jobs and pins the external tool versions.

## Standard checks

The release tools need Python 3.9 or later and the standard library only; [.python-version](../.python-version) selects Python in CI. Initialize temporary output before running the checks. In a POSIX shell on macOS or Linux:

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

None of these checks opens an audio device; device tests are ignored by default. Read the test counts and failures, not just the exit status.

With the versions named in `quality.yml` installed, CI's audit and workflow lint use these commands in a POSIX shell:

```sh
cargo audit --deny warnings --db target/advisory-db
SHELLCHECK_OPTS=--enable=all actionlint -shellcheck "$(command -v shellcheck)" .github/workflows/*.yml
```

For pushes to `main` and pull requests, Quality checks formatting, Clippy, debug-profile tests, Python release-tool tests, documentation, a release build and package smoke tests on macOS, Ubuntu 22.04 and native Windows x86_64 and ARM64. Its Linux job also checks generated notices. Separate jobs audit advisories, lint workflows and test mutations in two core files; the weekly schedule runs the advisory audit only. The [release workflow](../.github/workflows/release.yml) also runs release-profile tests on all six native release targets before packaging them. Windows packaging checks the final link trace and map against the pinned bundled runtime inputs, then checks executable imports and smoke-tests the extracted executable; see the [Windows build contract](releasing.md#windows-build-contract). Passing local checks does not establish results on other platforms.

## Device checks

After changing playback code in `auditory-device`, run its device tests on an available default output:

```sh
cargo test --locked -p auditory-device --test default_output -- --ignored --nocapture
```

On Windows, after the [native host setup](../README.md#install-from-source), pass the target before the test arguments:

```text
cargo test --locked -p auditory-device --test default_output --target x86_64-pc-windows-gnullvm -- --ignored --nocapture
```

On ARM64, use `--target aarch64-pc-windows-gnullvm` instead.

They play silence only: one test completes a second of it, the other cancels a long one. Silence cannot show ear routing or acoustic timing; unit tests check channel placement. If no compatible output exists, record the check as not run. Do not change devices, routing or volume to make a test run, and never play audible sound in tests.

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
