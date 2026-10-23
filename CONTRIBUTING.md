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

The toolchain is pinned in [rust-toolchain.toml](rust-toolchain.toml); the workspace manifest declares the minimum compiler. CI tool versions and platforms are defined in [ci.yml](.github/workflows/ci.yml). The lint job checks both workflows with a checksum-pinned actionlint. Pinned cargo-tool executables are cached by tool version, runner architecture and build configuration; dependency/advisory data and product build outputs are not restored from those caches.

Unit tests live beside code; integration tests use public APIs and the actual CLI executable. Check signal formulas against independent oracles. Give validation guards accepted and rejected neighbours. Change a [pinned digest](crates/core/tests/determinism.rs) only for a deliberate sample change and explain it in the commit. Property tests set bounded case/shrink budgets and disable persistence; turn minimized failures into regression tests. Allocation tests include a negative control. Device tests are ignored by default and use component-free silence; preserve the user's device, routing and volume.

Frontends use the library for domain workflows. Keep rendering and validation in core; editing, publication, playback, interactive workflow and shared wording in the library; and terminal or widget mechanics in frontends. GUI tests install the production bindings with `SlintAdapter::install` on an offscreen platform, one per test thread, rather than wiring callbacks of their own. Core, library and CLI forbid unsafe code; the GUI confines unsafe code to generated Slint and audited native adapters. [Workspace lints](Cargo.toml) are shared.

### Lint exceptions

All authored packages inherit `[lints] workspace = true`. Cargo.toml owns lint levels; clippy.toml owns Clippy configuration. Argument count is a review signal: split unrelated responsibilities when that simplifies the whole design, and justify an irreducible ABI or cohesive operation locally rather than introducing a parameter bag to silence a lint. Test-function unwrap/expect panics deliberately fail the test; fixture helpers need their own narrow exception.

There is no baseline exemption for existing code. First fix the code or remove an unnecessary allowance. When a conversion, generated-code boundary or native ABI genuinely requires an exception, put `#[expect(LINT, reason = "…")]` on the narrowest item, with a reason that states why the exception is correct there. `expect` fails the build when the lint no longer fires, so stale exceptions surface; the workspace denies `allow` attributes without a reason and `allow` attributes generally. Keep local `SAFETY` comments at unsafe operations. A changed bound or native lifetime requires fresh review of the reason.

`third_party/` contains upstream dependency sources governed by [their record](third_party/README.md); review changes there at their native boundary.

Mutation testing stays short and scoped to one file:

```sh
TMPDIR="$PWD/target/tmp" cargo mutants --file crates/core/src/synthesis.rs \
  --timeout 60 --cargo-arg=--locked --output target/mutations
```

Use `--re` to focus a smaller selection. Investigate survivors against an independent oracle. CI mutates each core source file that holds synthesis, compilation, live transitions, facts, timing or waveform reduction; when such code moves to another file, change that file list in [ci.yml](.github/workflows/ci.yml). The CI command excludes equivalent integer-phase mutations by location, with their reasons beside the command; a change that moves those lines must update the locations. Keep the Nyquist guard in the selection.

A downstream program that uses the library without the `device-playback` feature must not reach CPAL. CI runs the [portable consumer](consumers/portable/src/main.rs) and checks its dependency tree:

```sh
cp Cargo.lock consumers/portable/Cargo.lock
CARGO_TARGET_DIR=target/portable-consumer cargo run --manifest-path consumers/portable/Cargo.toml
CARGO_TARGET_DIR=target/portable-consumer cargo tree --manifest-path consumers/portable/Cargo.toml -e normal,build --target all -i cpal
```

The last command must report that no package matches.

## Dependencies

[Cargo.toml](Cargo.toml) declares ranges and [Cargo.lock](Cargo.lock) pins resolution. Update a named dependency with `cargo update -p NAME`, inspect the delta, run the checks and regenerate archives. Portable `libm` with `force-soft-floats` keeps math independent of platform intrinsics after feature unification.

[cargo-deny policy](deny.toml) gates advisories, licenses, sources and duplicate versions. New advisory ignores need a written reason. [about.toml](about.toml) controls license selection. Packaging resolves the full non-dev graph, compares cargo-about package identities against Cargo's selected tree and metadata, checks every crate's license-text coverage, then emits normal/proc-macro dependency notices. Slint terms come from the locked Slint package. Rust standard-library and Windows runtime paragraphs live in [notice templates](xtask/templates). Every archive also includes the pinned compiler’s unmodified `COPYRIGHT-library.html` for standard-library component notices outside Cargo’s dependency graph. Release preflight checks the compiler and source file; extracted package checks require an exact byte match.

## Desktop checks

Open an isolated built window. Check fixed toolbar/transport versus scrolling document, bounded drawer/large-text monitor window, document-local static patterns, selectable details, input recovery and field-specific accessibility messages. Save a component-free snapshot while audio continues; verify Stop inside the real native panel, later edits remaining dirty, cancellation/replacement and both-worker quit cleanup. On macOS, quit with unsaved edits through the application menu and Command-Q, choose Cancel, then Save, and check that one dialog appears however often Quit is pressed. Check native Open, export cancellation, monitor Live/Held/Stale/Last and About from the application menu. Play only a component-free recipe during development checks. Check progress and CPU use at the intended display scale. The UI uses OpenGL and synthesis runs on the CPU.

## Packaging and releases

Install cargo-about, cargo-deny and cargo-mutants at the versions defined in [CI](.github/workflows/ci.yml). When installing cargo-about from source, enable its `cli` feature. `cargo xtask dist --target TARGET` builds both products with the distribution profile, generates notices and README files, writes archives under `target/dist`, and runs host smoke checks. Supported target arguments are defined in [xtask](xtask/src/main.rs). macOS packaging uses the native `aarch64-apple-darwin` or `x86_64-apple-darwin` target. Build/qualify each on matching hardware. `lipo` inspects extracted architecture; `codesign` signs/verifies the app. Each macOS target builds a single architecture.

To inspect notices without building archives, use `cargo xtask notices cli TARGET target/cli-notices.txt` or substitute `gui`. The checks reject missing dependency identities and missing license sections. For ZIP and tar archives, xtask writes members in-process and preserves executable permissions.

For a release, set the workspace version, add a dated changelog section and review `cargo xtask notes VERSION`. Commit the reviewed candidate on main and push. To proceed directly to release without a duplicate push-triggered CI run, use `[skip ci]` on the release-preparation commit; workflow dispatch still runs the full qualification. Dispatch [release.yml](.github/workflows/release.yml) with that version and `publish: true` for authorized publication, or `publish: false` (the default) to inspect a verification bundle without publishing. Verification may run on a branch; publication requires main. Both modes check the requested workspace version, a dated nonempty changelog section and an absent tag before building.

Both modes call CI for the selected source bytes, including every native target and every file-scoped mutation job, collect archives through `cargo xtask collect-assets VERSION INPUT OUTPUT`, reject missing/duplicate/unexpected product/target archives from xtask’s supported mapping, write SHA256SUMS and release notes, and attest the assets. They upload a `release-verification` artifact containing `assets/`, `notes.md` and `provenance.jsonl`. A separate read-only job downloads that artifact and verifies the checksums and provenance against the source commit, ref and release workflow; both modes must pass that round trip. Release notes are attested and verified alongside the archive/checksum files. Verification creates no tag or draft release. Use non-publishing mode when the bundle needs manual review before publication. Review it for the exact intended main commit and repeat that review if main changes. It does create workflow artifacts and GitHub attestations.

The publishing dispatch builds and verifies the selected main source bytes once, then a separate job with release-write permission downloads the verification artifact and rechecks its checksums. That job first checks that main still names the verified source commit, creates a draft release, compares uploaded digests with the built bytes, then publishes. Published tags and assets are never replaced. If publication stops at a draft, inspect the failed step and its logs; compare every uploaded digest and source commit before completing that same draft. Choose a new version for changes to published bytes. To verify a public download, compare SHA256SUMS and the release asset digests, inspect the attestation against the release workflow/source commit, and run the binary's `--version`.

The canonical native build counter is `workspace.metadata.auditorylab.build-number` (integer 1..9999). Product semver remains `workspace.package.version`. Increment the counter deliberately for distributable builds. The library build script regenerates identity when metadata or `AUDITORYLAB_SOURCE_REVISION` changes; supplied revisions must be full 40- or 64-character ASCII hexadecimal IDs. Packaged macOS About uses separate marketing/build fields. Keep pending notes under Unreleased. Release preparation moves those notes to a dated section matching the workspace version; `check-release VERSION` requires that section before release verification.

Generated-output verification includes `cargo test -p auditorylab callback_monitor_costs --release -- --ignored --nocapture` for synthetic callback timing at 48/768 kHz. It opens no device. Run file-scoped mutation checks for numerical/transition changes with timeout 60, output and TMPDIR under target/. Native UI qualification covers SF/Segoe and real GNOME/KDE font changes; cross-compilation alone does not qualify rendering or desktop preferences. Linux runtime font adapters use Pango/GTK/GIO; install `libpango-1.0-0 libgtk-3-0` for those preference sources. Hardware/GUI tests use component-free silent recipes.
