# Contributing

Read [AGENTS.md](AGENTS.md) and the [architecture guide](docs/architecture.md). [Source setup](README.md#build-from-source) lists prerequisites. Understand the affected contracts, design the whole change, challenge it, then update callers, tests and documents together.

Run the CLI with `cargo run --release --locked -p auditorylab-cli -- list`, or the desktop with `cargo run --release --locked -p auditorylab-gui`.

## Checks

Keep temporary files under `target/`. Set TMPDIR (and TEMP/TMP on Windows) to an absolute directory there before running tests.

```sh
mkdir -p target/tmp
export TMPDIR="$PWD/target/tmp"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p auditorylab-core --release --locked
cargo doc --workspace --no-deps --locked
cargo deny check
```

The toolchain is pinned in [rust-toolchain.toml](rust-toolchain.toml); the workspace manifest declares the minimum compiler. CI tool versions and platforms are defined in [ci.yml](.github/workflows/ci.yml).

Unit tests live beside code; integration tests use public APIs and the actual CLI executable. Check signal formulas against independent oracles. Give validation guards accepted and rejected neighbours. Change a [pinned digest](crates/core/tests/determinism.rs) only for a deliberate sample change and explain it in the commit. Property tests set bounded case/shrink budgets and disable persistence; turn minimized failures into regression tests. Allocation tests include a negative control. Device tests are ignored by default and use component-free silence; preserve the user's device, routing and volume.

Frontends use the library for domain workflows. Keep rendering and validation in core, publication and playback in the library, and presentation in frontends. Core, library and CLI forbid unsafe code; the GUI confines unsafe code to generated Slint and audited native adapters. [Workspace lints](Cargo.toml) are shared.

Mutation testing stays short and scoped to one file:

```sh
TMPDIR="$PWD/target/tmp" cargo mutants --file crates/core/src/synthesis.rs \
  --timeout 60 --cargo-arg=--locked --output target/mutations
```

Use `--re` to focus a smaller selection. Investigate survivors against an independent oracle. The CI command excludes equivalent integer-phase mutations with their reasons beside the command. Keep the Nyquist guard in the selection.

## Dependencies

[Cargo.toml](Cargo.toml) declares ranges and [Cargo.lock](Cargo.lock) pins resolution. Update a named dependency with `cargo update -p NAME`, inspect the delta, run the checks and regenerate archives. Portable `libm` with `force-soft-floats` keeps math independent of platform intrinsics after feature unification.

[cargo-deny policy](deny.toml) gates advisories, licenses, sources and duplicate versions. New advisory ignores need a written reason. [about.toml](about.toml) controls license selection. Packaging resolves the full non-dev graph, compares cargo-about package identities against Cargo's selected tree and metadata, checks every crate's license-text coverage, then emits normal/proc-macro dependency notices. Slint terms come from the locked Slint package. Rust standard-library and Windows runtime paragraphs live in [notice templates](xtask/templates).

## Desktop checks

Open an isolated built window. Check fixed toolbar/transport versus scrolling document, bounded drawer/large-text monitor window, document-local static patterns, selectable details, input recovery and field-specific accessibility messages. Save a component-free snapshot while audio continues; verify Stop inside the real native panel, later edits remaining dirty, cancellation/replacement and both-worker quit cleanup. Check native Open, export cancellation, monitor Live/Held/Stale/Last and About. Play only a component-free recipe during development checks. Check progress and CPU use at the intended display scale. The UI uses OpenGL and synthesis runs on the CPU.

## Packaging and releases

Install cargo-about, cargo-deny and cargo-mutants at the versions defined in [CI](.github/workflows/ci.yml). When installing cargo-about from source, enable its `cli` feature. `cargo xtask dist --target TARGET` builds both products with the distribution profile, generates notices and README files, writes archives under `target/dist`, and runs host smoke checks. Supported target arguments are defined in [xtask](xtask/src/main.rs). macOS packaging uses the native `aarch64-apple-darwin` or `x86_64-apple-darwin` target. Build/qualify each on matching hardware. `lipo` inspects extracted architecture; `codesign` signs/verifies the app. There is no universal target.

To inspect notices without building archives, use `cargo xtask notices cli TARGET target/cli-notices.txt` or substitute `gui`. The checks reject missing dependency identities and missing license sections. For ZIP and tar archives, xtask writes members in-process and preserves executable permissions.

For a release, set the workspace version, add a dated changelog section, run `cargo xtask check-release VERSION`, and review `cargo xtask notes VERSION`. Commit on main and push. CI tests each platform and uploads the product archives. Test the downloaded archives, then dispatch [release.yml](.github/workflows/release.yml) with that version and `publish: false` (the default). Verification may run on a branch; publication requires main. Both modes require the requested workspace version, a dated nonempty changelog section and an absent tag.

Both modes call CI for the selected source bytes, collect archives through `cargo xtask collect-assets VERSION INPUT OUTPUT`, reject missing/duplicate/unexpected product/target archives from xtask’s supported mapping, write SHA256SUMS and release notes, and attest the assets. They upload a `release-verification` artifact containing `assets/`, `notes.md` and `provenance.jsonl`. A separate read-only job downloads that artifact and verifies the checksums and provenance against the source commit, ref and release workflow; both modes must pass that round trip. Verification creates no tag or draft release. Download and review that artifact for the exact main commit intended for publication; rerun verification if main changes. It does create workflow artifacts and GitHub attestations.

To publish, dispatch the workflow on main with the same version and `publish: true`. It rebuilds and verifies those source bytes, then a separate job with release-write permission downloads the verification artifact and rechecks its checksums. That job creates a draft release, compares uploaded digests with the built bytes, then publishes. Published tags and assets are never replaced. If publication stops at a draft, inspect the failed step and its logs; compare every uploaded digest and source commit before completing that same draft. Choose a new version for changes to published bytes. To verify a public download, compare SHA256SUMS and the release asset digests, inspect the attestation against the release workflow/source commit, and run the binary's `--version`.

The canonical native build counter is `workspace.metadata.auditorylab.build-number` (integer 1..9999). Product semver remains `workspace.package.version`. Increment the counter deliberately for distributable builds. The library build script regenerates identity when metadata or `AUDITORYLAB_SOURCE_REVISION` changes; supplied revisions must be full 40- or 64-character ASCII hexadecimal IDs. Packaged macOS About uses separate marketing/build fields. Local development keeps 0.4.0 notes under Unreleased: `check-release 0.4.0` must reject until a later authorized release preparation adds dated notes.

Generated-output verification includes `cargo test -p auditorylab callback_monitor_costs --release -- --ignored --nocapture` for synthetic callback timing at 48/768 kHz. It opens no device. Run file-scoped mutation checks for numerical/transition changes with timeout 60, output and TMPDIR under target/. Native UI qualification covers SF/Segoe and real GNOME/KDE font changes; cross-compilation alone does not qualify rendering or desktop preferences. Linux runtime font adapters use Pango/GTK/GIO; install `libpango-1.0-0 libgtk-3-0` for those preference sources. Hardware/GUI tests use component-free silent recipes.
