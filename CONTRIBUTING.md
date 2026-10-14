# Contributing

Read [AGENTS.md](AGENTS.md) for design and review requirements, [architecture](docs/architecture.md) for responsibilities, and [verification](docs/verification.md) for runnable checks. [Quick start](README.md#quick-start) lists build prerequisites.

## Change workflow

Understand the relevant code, contracts and tests. Design the whole change and challenge its assumptions before implementation. Follow affected callers, tests, configuration, packaging and documentation. Preserve unrelated work and verify the delivered state.

Keep live guides about the current application and contributor procedures. Explain present constraints and reasons; omit development chronology and results from individual audit runs. [CHANGELOG.md](CHANGELOG.md) records public release outcomes and supplies release notes; unpublished development churn does not belong there. Catalog citations and adaptations describe the sources of the currently included stimuli.

For development, run commands from the checkout:

```sh
cargo run --release --locked -p auditory-lab -- list
```

## Lints and tests

[Cargo.toml](Cargo.toml) defines workspace lint levels, inherited by every member. Warnings are errors, `unsafe` is forbidden, and Clippy's `all` group plus explicit numeric-conversion and correctness lints are denied. Use a narrow `#[expect(lint, reason = "…")]` only when the code is correct and the reason explains the exception; stale expectations are compiler findings.

- Unit tests sit beside their code. Public-interface and binary tests live in each crate's `tests` directory, with file-level `#![cfg(test)]` for the test-only unwrap/expect allowances.
- Check signal mathematics against independent formulas. Give validation and ownership guards inputs they must reject.
- [Determinism tests](crates/auditory-core/tests/determinism.rs) pin representative rendered samples. Preserve existing digests when samples are unchanged; explain deliberate sample changes and update their affected expectations together.
- Property tests state case and shrink budgets, set `failure_persistence: None`, and use only proptest's `std` feature with defaults disabled. Shrink time limits minimization, not total test execution. `PROPTEST_RNG_SEED` replays a run; concrete shrunk failures belong in ordinary regression tests.
- Parser tests cover strict structure, byte bounds, schema and semantic rejection. Filesystem tests use owned `tempfile` directories, with temporary output under `target/`.

The core uses `no_std` with `alloc` and portable `libm` arithmetic. Review production dependencies, imports and allocation/I/O behavior at the numerical boundary. Tests can use platform math as an independent oracle.

## Dependencies and toolchain

[Cargo.toml](Cargo.toml) declares supported dependency ranges; [Cargo.lock](Cargo.lock) pins builds. Use `--locked` for build, test and install commands. To change a dependency, edit the root manifest or run `cargo update -p NAME`, replacing `NAME` with its package name, then inspect:

```sh
cargo tree --workspace --target all --duplicates
```

Run the applicable [checks](docs/verification.md#standard-checks) and advisory audit after the change. Core reads caller-supplied bounded text through `serde`/`toml` and computes samples with `libm`; it opens no filesystem or device handles.

[rust-toolchain.toml](rust-toolchain.toml) pins the development/CI compiler. Root `rust-version` is the minimum supported compiler, enforced by Cargo. Tool versions for workflow linting, dependency auditing and mutation testing are pinned in [quality.yml](.github/workflows/quality.yml).

Native archive preparation and public publication follow [releasing](docs/releasing.md). Release-tool checks use Python 3.11 or later locally; the workflows pin their Python version. Run `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v` from the checkout.
