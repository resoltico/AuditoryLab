# Contributing

Use the pinned [Rust toolchain](rust-toolchain.toml) and [source prerequisites](README.md#build-from-source). The workspace manifest owns compiler/package metadata, Cargo.lock pins dependencies, and [CI](.github/workflows/ci.yml) owns tool versions and target selection.

## Checks

Keep temporary files inside the checkout's `target/`:

```sh
mkdir -p target/tmp
export TMPDIR="$PWD/target/tmp"
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo test -p auditorylab-core --release --locked
cargo doc --workspace --no-deps --locked
cargo deny check
cargo xtask verify-vendor
actionlint
```

Use the CI-pinned cargo-deny, cargo-mutants, cargo-about and actionlint versions. On Windows set TMP and TEMP to an absolute directory under target as well. Linux calendar tests require the system `tzdata` database. Native desktop checks require the platform libraries in the source prerequisites; X11 also needs its xkbcommon library, and GNOME Wayland text scaling uses GLib gsettings.

Unit tests live beside code. Integration tests exercise public APIs and the actual CLI. GUI tests install production callbacks on an offscreen platform. Use independently written expectations for numerical/structural properties and relevant negative controls. The [determinism digests](crates/core/tests/determinism.rs) describe sample bits; changes require an intended sample-contract change. Device tests remain ignored and use component-free silence. Preserve the user's device, routing, volume and accessibility settings.

Workspace tests drive commands, emitted jobs and typed completions. Runner tests use real threads/barriers to check completion-before-exit, joining, launch failures and panics. A slot cannot be released merely because a computation has a result. Subprocess entry fixtures are invoked by their parent tests for filesystem locking, process interruption and isolated timezone behavior.

## Callback cost check

```sh
cargo test -p auditorylab --profile dist --locked callback_cost -- --ignored --nocapture
```

Run this once on the intended source after other builds/tests finish. Use the distribution profile so the gate measures the shipped thin-LTO/codegen settings. It opens no device. Each configuration warms up 100 callbacks and measures 3,000, with 16 distinct carrier/modulation schedules, full target replacements and up to 32 transition slots. Configurations cover 48/768 kHz, 64/256/1024/4096 frames and none/quantities/waveform monitoring. The median wall time must fit half the buffer period; p99 and maximum are reported without gating them. CI runs it on Linux x86_64 and macOS arm64. Investigate a failure rather than rerunning or changing the limit. Ordinary tests check allocation/free behavior, overflow, coherence and lifecycle authority; playback reports actual backend underruns.

## Mutation testing

Keep local runs short and scoped to one source file:

```sh
TMPDIR="$PWD/target/tmp" cargo mutants --file crates/core/src/synthesis.rs \
  --timeout 60 --cargo-arg=--locked --output target/mutations
```

Use `--re` or a bounded shard to select a small investigation. The [workflow matrix](.github/workflows/ci.yml) owns CI source/shard selection. Its scopes cover synthesis, compilation, live transitions, facts, timing and waveform reduction; update those scopes when code moves. Each shard checks the unmutated baseline. Equivalent mutations are excluded by operator/function name with a reason beside the command, without line or column numbers. Keep Nyquist and pulse-width guards under mutation. Investigate survivors against an independent oracle. Each extra worker adds a compiler directory; use one unless parallel work is explicitly useful and authorized.

## Design and implementation

Keep synthesis and validation in core; persistence, editing, playback, vocabulary and workspace policy in the application; terminal/window mechanics in frontends. The application exposes core through `auditorylab::core` and its own capabilities through named modules. Frontends render domain descriptions instead of composing alternatives. Search uses typed facts independently of captions.

Preserve sample determinism, catalog bytes, actual-rate validation, callback allocation/free/wait/I/O rules and terminal-outcome authority. Storage changes must preserve no-clobber behavior, opened-file identity/content checks, captured-save baselines and readable content through interrupted publication.

Workspace lints are shared. Prefer removing unnecessary work over suppressing warnings. Where a conversion, generated-code boundary or native ABI requires an exception, use `#[expect(LINT, reason = "…")]` on the narrowest item. Keep native `SAFETY` reasoning next to the operation. Core/application crates forbid unsafe code; GUI native adapters and generated output have scoped exceptions.

## Dependencies and native toolkit

`cargo deny check` checks advisories, source policy, duplicate versions and licenses. Review graph changes and Cargo.lock together. [Toolkit integration](third_party/README.md) describes authenticated patches, their retained behavior and retirement conditions. Upgrade source, patch, archive digest and affected consumer checks together; `cargo xtask verify-vendor` reproduces and compares them.

## Build storage and cleanup

Keep all rebuildable/disposable files under `target/`, with one build directory per profile. Ordinary Cargo profiles share their dependency cache. Use `cargo clean --profile dev` or `cargo clean --profile release` for an idle profile. A full `cargo clean` reclaims all checkout outputs, including evidence.

Keep small evidence for open failures under `target/evidence/<topic>/`. Retain explicitly requested final reports, cost/text-scaling results and upstream-ready diffs. Discard passing logs, package snapshots and repeated check outputs. Before deleting earlier/shared outputs, inspect their contents and get owner approval. Do not clean an active build or a requested running service.

An isolated build uses a temporary target directory and removes it after required outputs are retained. Set TMPDIR inside target for subprocess/tool scratch. Omit `--target-dir` for Cargo-installed tools so installation intermediates are temporary. Containers and image layers created for checks are removed afterwards; shared pre-existing images are preserved.

## Packaging

The [release guide](docs/releasing.md) covers native prerequisites, resource compilation, source/build identity, archives and publication. Package checks extract binaries and inspect the delivered content. Run on the intended target OS/CPU; cross-compilation alone does not exercise native rendering or devices. Hardware/desktop checks use silent recipes, and publication requires the owner's approval. Root [LICENSE](LICENSE) and the [artwork guide](artwork/README.md) define software/artwork terms.
