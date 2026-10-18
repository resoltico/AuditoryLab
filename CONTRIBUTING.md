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

Frontends use the library for domain workflows. Keep rendering and validation in core, publication and playback in the library, and presentation in frontends. Core, library and CLI forbid unsafe code; the GUI's only unsafe allowance covers Slint-generated code. [Workspace lints](Cargo.toml) are shared.

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

Open the built window. Check selection, static previews, waveform windows, selectable research text, native Open and Save, confirmed replacement, export cancellation, close during an operation and About. Play only a component-free recipe during development checks. Check progress and CPU use at the intended display scale. The UI uses OpenGL and synthesis runs on the CPU.

## Packaging and releases

Install cargo-about, cargo-deny and cargo-mutants at the versions defined in [CI](.github/workflows/ci.yml). When installing cargo-about from source, enable its `cli` feature. `cargo xtask dist --target TARGET` builds both products with the distribution profile, generates notices and README files, writes archives under `target/dist`, and runs host smoke checks. Supported target arguments are defined in [xtask](xtask/src/main.rs). Universal macOS packaging needs both Apple Rust targets plus `lipo` and `codesign` from Xcode Command Line Tools.

To inspect notices without building archives, use `cargo xtask notices cli TARGET target/cli-notices.txt` or substitute `gui`. The checks reject missing dependency identities and missing license sections. For ZIP and tar archives, xtask writes members in-process and preserves executable permissions.

For a release, set the workspace version, add a dated changelog section, run `cargo xtask check-release VERSION`, and review `cargo xtask notes VERSION`. Commit on main and push. CI tests each platform and uploads the product archives. Test the downloaded archives, then dispatch [release.yml](.github/workflows/release.yml) with that version.

Release checks the ref, version, notes and absent tag, calls CI for those source bytes, collects archives, writes SHA256SUMS and attests the artifacts. It creates a draft release, compares uploaded digests with the built bytes, then publishes. Published tags and assets are never replaced. If publication stops at a draft, inspect the failed step and its logs; compare every uploaded digest and source commit before completing that same draft. Choose a new version for changes to published bytes. To verify a public download, compare SHA256SUMS and the release asset digests, inspect the attestation against the release workflow/source commit, and run the binary's `--version`.
