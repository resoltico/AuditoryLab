# Contributing

Start from a clean checkout with the [source prerequisites](README.md#build-from-source). [rust-toolchain.toml](rust-toolchain.toml) owns the compiler/components; root [Cargo.toml](Cargo.toml) owns product name/copyright/version/build metadata, and [Cargo.lock](Cargo.lock) pins dependencies. [CI](.github/workflows/ci.yml) owns auxiliary tool versions and target/shard selection.

## Checks

Keep tool scratch under this checkout’s `target/`:

```sh
mkdir -p target/tmp
export TMPDIR="$PWD/target/tmp"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p auditorylab-core --release --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo deny check
cargo xtask verify-vendor
actionlint
```

The compiler warns about ordinary hygiene locally. Core emits missing-documentation warnings; unsafe-code rules remain denied. CI denies workspace Clippy warnings and documentation warnings without applying a global warning denial to vendored path dependencies.

Use the CI-pinned cargo-deny, cargo-mutants, cargo-about and actionlint versions. On Windows, point TMP/TEMP to an absolute directory under `target/`. Linux calendar checks need system `tzdata`.

Unit tests exercise owners and private adapters; integration tests use public services and the actual CLI. GUI tests install production callbacks on an offscreen platform with an injected deterministic executor; startup injects the real Runner. App private-state tests share the authoritative presentation source, while public presentation integration suites live in `crates/presentation/tests`. Use independent numerical/structural expectations, relevant failures and negative controls. Check discovered/ignored counts and retain useful failure evidence.

The [determinism tests](crates/core/tests/determinism.rs) and [seek tests](crates/core/tests/seek.rs) pin sample bits. Preserve them unless an intended sample-contract change is authorized. Device checks stay ignored by default and use component-free silence. Preserve device, routing, volume and accessibility preferences.

Workspace/Runner tests cover captured inputs, typed responses, real threads, joining, panics and launch failures. A completed result cannot release a still-running task slot. Subprocess fixtures check locking, killed writers and isolated timezone behavior.

## Callback cost check

After other builds/tests finish, run once on the intended source:

```sh
cargo test -p auditorylab-app --profile dist --locked callback_cost -- --ignored --nocapture
```

This opens no device. It warms up 100 callbacks and measures 3,000 per configuration, using distinct carrier/modulation schedules, full target replacements and bounded transition slots. Coverage includes 48/768 kHz, 64/256/1024/4096 frames and none/quantities/waveform monitoring. The median must fit half the buffer period; p99/maximum are reported. Investigate a failure before rerunning or changing its limit.

## Mutation testing

Keep local runs short, scoped to one file, with a timeout and output under `target/`:

```sh
TMPDIR="$PWD/target/tmp" cargo mutants --file crates/core/src/synthesis.rs \
  --timeout 60 --cargo-arg=--locked --output target/mutations
```

Use `--re` or a bounded shard for the question being investigated. Each selected shard checks its unmutated baseline. Keep Nyquist/pulse-width guards under mutation. Equivalent exclusions use operator/function names with a reason, without line/column numbers. Update CI scopes when code moves; `number.rs` owns finite-value invariants and `catalog_id.rs` owns catalog-name grammar. Lint capping is disabled, so unsafe-code policies still apply to generated mutants. Each worker adds a compiler directory; parallel work needs explicit authorization and a concrete benefit.

## Ownership and implementation

Follow [architecture](docs/architecture.md) and the repository agent instructions. Core owns sound/format invariants. Application services own operations; the workspace owns interactive admission/continuation. Private adapters enforce checks against the actual action state. Frontends translate requests and render `auditorylab-presentation` descriptions. Number-entry grammar belongs to `document::input`; workspace views carry typed data and causes. GUI-only chrome stays in the GUI.

Preserve actual-rate validation, callback allocation/free/wait/I/O rules, typed causes, terminal-outcome authority, captured-save baselines and no-clobber publication. Use the existing shared rules and tests when changing affected consumers. [Formats](docs/formats.md) defines compatibility contracts.

Prefer removing unnecessary work over lint suppression. For a necessary numeric conversion, generated boundary or native ABI, put a reasoned `#[expect(...)]` on the narrowest item. Keep native `SAFETY` reasoning next to the operation. Core/application forbid unsafe code; native GUI adapters and generated output have scoped boundaries.

## Dependencies and native toolkit

Review graph changes with Cargo.lock. `cargo deny check` covers advisories, source policy, duplicates and licenses. [Toolkit integration](third_party/README.md) records archive pins, patch behavior and retirement conditions. Update source, patch, digest and affected consumer checks together. `cargo xtask verify-vendor` authenticates, applies and compares every source byte and checks target/product resolution.

## Build storage and cleanup

Keep rebuildable/disposable outputs under `target/`, with one build directory per profile. Ordinary profiles share dependency caches. Clean only idle profiles; full `cargo clean` also removes evidence.

Retain small open-failure evidence under `target/evidence/<topic>/` and explicitly requested reports/results/diffs. Discard passing logs, repeated check outputs and package snapshots after required evidence is retained. Inspect earlier/shared outputs and obtain owner approval before deleting them. Keep active builds and requested services intact.

Isolated outputs use a temporary target directory, removed after required outputs are saved. Put TMPDIR under target for subprocesses. Omit `--target-dir` for Cargo-installed tools so their installation intermediates stay temporary. Remove task-created containers/layers; preserve shared pre-existing images.

## Release summaries

[AGENTS.md](AGENTS.md#changelog-rules) owns the changelog rules. Keep `Unreleased` cumulative against the latest published non-prerelease release, and verify that publication through GitHub. Describe implemented compatibility effects and required action. Documentation-only maintenance does not need a changelog entry.

## Packaging

Use the [release guide](docs/releasing.md) for native toolchains, icon resources, identity, archives and extracted-package checks. Execute checks on the matching OS/CPU; cross-compilation alone does not exercise native rendering/devices. Use silent recipes for hardware checks. Versioning, tagging, publication and upstream submission require the owner’s authorization. [LICENSE](LICENSE) and the [artwork guide](artwork/README.md) define distribution terms.
