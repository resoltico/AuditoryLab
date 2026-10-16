# Contributing

Read [AGENTS.md](AGENTS.md) for design and review requirements, [architecture](docs/architecture.md) for responsibilities and [verification](docs/verification.md) for checks. [Install from source](README.md#install-from-source) lists the prerequisites.

## Changes

Understand the code, contracts and tests involved, design the whole change, and challenge it before implementing. Follow it through callers, tests, configuration, packaging and documentation, and verify the result.

Documentation describes the current software and how to work on it. [CHANGELOG.md](CHANGELOG.md) records published outcomes and supplies release notes. Catalog citations, study descriptions and adaptations must stay accurate to their sources.

On macOS or Linux, run the program from a checkout with:

```sh
cargo run --release --locked -p auditory-cli -- list
```

On Windows, use the native GNU LLVM host from the [source setup](README.md#install-from-source) and pass its target before `--`:

```text
cargo run --release --locked -p auditory-cli --target x86_64-pc-windows-gnullvm -- list
```

On ARM64, use `--target aarch64-pc-windows-gnullvm` instead. Keep the explicit native target for Cargo commands that compile or test the application; [verification](docs/verification.md#standard-checks) gives the check commands.

## Lints and tests

[Cargo.toml](Cargo.toml) sets the workspace lint authority: warnings are errors, handwritten unsafe code is prohibited, and Clippy's `all` group plus the numeric-conversion/panic rules are denied. Library and CLI crates use `unsafe_code = forbid`. GUI alone has a checked full lint copy with deny and one generated-only allowance required by Slint; handwritten GUI unsafe syntax is rejected. Third-party dependencies retain their own reviewed implementation/licensing boundaries. Use a narrow `#[expect(lint, reason = "…")]` only when the code is correct and the reason says why.

- Unit tests sit beside their code; public-interface and binary tests live in each crate's `tests` directory.
- Check signal mathematics against independent formulas, and give every validation guard an input it must reject.
- [Determinism tests](crates/auditory-core/tests/determinism.rs) pin rendered samples. A digest changes only when its samples deliberately change; say why in the change.
- Property tests state their case and shrink budgets and set `failure_persistence: None`. `PROPTEST_RNG_SEED` replays a run; turn a shrunk failure into an ordinary regression test.
- Filesystem tests use their own temporary directories, under `target/`, with TMPDIR, TEMP and TMP initialized there.

The core is `no_std` with `alloc` and computes with portable `libm`; `force-soft-floats` overrides architecture features introduced by transitive GUI feature unification. Determinism tests check this; tests may use the platform's math as an independent oracle.

Frontends must use `auditory-app` for domain workflows. The required architecture integration driver checks every workspace role, dependency kind/target and admitted production source root. New dependencies or target roles require a deliberate policy review in `crates/auditory-app/tests/frontend_boundary/roles.rs`; frontend syntax and source admission live in separate private modules. Native qualification explicitly requires the essential app integration targets and their behavioral/rejection controls to be discovered and executed, so a successful excluded or empty suite is insufficient. Reviewers must still check for copied algorithms and policy; syntax and dependency checks cannot prove semantic equivalence. Prepared playback stays on its native worker; owned reports and failures can cross threads. GUI and CLI have separate presentation allowlists. The reviewed GUI compiler/generated module, embedded resources and actual compiler inputs have narrow checks; they are not general source-inclusion/build-script exemptions. Keep runtime renderer environment variables out of Cargo compiler invocations. Waveform analysis belongs in core/app; frontends map app-supplied ranges to coordinates. No frontend resynthesis, independent extrema/RMS/FFT or raw device/transport ownership is permitted.

## Dependencies and toolchain

[Cargo.toml](Cargo.toml) declares supported dependency ranges and [Cargo.lock](Cargo.lock) pins builds; use `--locked`. To update a dependency, edit the manifest or run `cargo update -p NAME`, then inspect `cargo tree --workspace --target all --duplicates`, run the checks and the advisory audit, and regenerate the third-party notices as [releasing](docs/releasing.md#notices-and-dependency-updates) describes.

[rust-toolchain.toml](rust-toolchain.toml) pins the compiler used in development and CI; `rust-version` in Cargo.toml is the minimum Cargo enforces. [.python-version](.python-version) pins CI's Python for the release tools, which run on Python 3.9 or later with the standard library only.

## Desktop and qualification

Run supported Linux source builds from the checkout root so its Cargo links overrides suppress the bundled FreeType fallback. Install security-updated distribution FreeType/Fontconfig development libraries; outside-root `--manifest-path` and alternate compiler/configuration inputs are outside this checked source-build policy.

Use [desktop setup](docs/desktop.md) for Slint/native prerequisites and [verification](docs/verification.md) for source checks, actual window/renderer checks, silent audio and platform/accessibility evidence. Native code generation, headless package smoke and a painted window prove different properties. GPU feature selection does not establish an actual GPU frame or a power/performance improvement.

[qualify.py](scripts/qualify.py) is the shared native test/build/package entry point used by CI. Local kind remains usable for a dirty checkout and makes no immutable-source claim. Candidate/release kinds require clean exact committed source and isolated per-product environments/floors. Publication is a separate authorized workflow; a development qualifier never publishes. Keep all owned output under target and retain useful failure evidence.

The narrow Slint backend source patch is covered by its [provenance and rationale](vendor/slint-winit/AUDITORYLAB.md), selected upstream terms and independent native/rejection tests. Do not update reviewed hashes merely to accept a changed patch. Dependency notices distinguish actual runtime reachability from separately retained code-generation material and generated assets; none of those identity checks replaces review.
