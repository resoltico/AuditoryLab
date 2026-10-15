# Contributing

Read [AGENTS.md](AGENTS.md) for design and review requirements, [architecture](docs/architecture.md) for responsibilities and [verification](docs/verification.md) for checks. [Install from source](README.md#install-from-source) lists the prerequisites.

## Changes

Understand the code, contracts and tests involved, design the whole change, and challenge it before implementing. Follow it through callers, tests, configuration, packaging and documentation, and verify the result.

Documentation describes the current software and how to work on it. [CHANGELOG.md](CHANGELOG.md) records published outcomes and supplies release notes. Catalog citations, study descriptions and adaptations must stay accurate to their sources.

On macOS or Linux, run the program from a checkout with:

```sh
cargo run --release --locked -p auditory-lab -- list
```

On Windows, use the native GNU LLVM host from the [source setup](README.md#install-from-source) and pass its target before `--`:

```text
cargo run --release --locked -p auditory-lab --target x86_64-pc-windows-gnullvm -- list
```

On ARM64, use `--target aarch64-pc-windows-gnullvm` instead. Keep the explicit native target for Cargo commands that compile or test the application; [verification](docs/verification.md#standard-checks) gives the check commands.

## Lints and tests

[Cargo.toml](Cargo.toml) sets lint levels for every crate: warnings are errors, `unsafe` is forbidden, and Clippy's `all` group and the numeric-conversion and panic lints are denied. Use a narrow `#[expect(lint, reason = "…")]` only when the code is correct and the reason says why.

- Unit tests sit beside their code; public-interface and binary tests live in each crate's `tests` directory.
- Check signal mathematics against independent formulas, and give every validation guard an input it must reject.
- [Determinism tests](crates/auditory-core/tests/determinism.rs) pin rendered samples. A digest changes only when its samples deliberately change; say why in the change.
- Property tests state their case and shrink budgets and set `failure_persistence: None`. `PROPTEST_RNG_SEED` replays a run; turn a shrunk failure into an ordinary regression test.
- Filesystem tests use their own temporary directories, under `target/`, with TMPDIR, TEMP and TMP initialized there.

The core is `no_std` with `alloc` and computes with portable `libm`, so a recipe renders the same samples across supported targets. Determinism tests check this; tests may use the platform's math as an independent oracle.

## Dependencies and toolchain

[Cargo.toml](Cargo.toml) declares supported dependency ranges and [Cargo.lock](Cargo.lock) pins builds; use `--locked`. To update a dependency, edit the manifest or run `cargo update -p NAME`, then inspect `cargo tree --workspace --target all --duplicates`, run the checks and the advisory audit, and regenerate the third-party notices as [releasing](docs/releasing.md#dependency-updates) describes.

[rust-toolchain.toml](rust-toolchain.toml) pins the compiler used in development and CI; `rust-version` in Cargo.toml is the minimum Cargo enforces. [.python-version](.python-version) pins CI's Python for the release tools, which run on Python 3.9 or later with the standard library only.
