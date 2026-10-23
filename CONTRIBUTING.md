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

The canonical product build counter is `workspace.metadata.auditorylab.build-number` (integer 1..9999). Increase it before distributing changed product binaries; never reset it with a new semver. CLI, GUI and CPU variants share one value. Private/source rebuilds and CI reruns do not increment it. Git history stores iterations (`git log -p -- Cargo.toml`); semver, source revision and archive checksums identify different properties. The library build script regenerates identity when the manifest or `AUDITORYLAB_SOURCE_REVISION` changes. Supplied revisions must be full 40- or 64-character ASCII hexadecimal IDs. CLI/GUI `--version`, About and macOS `CFBundleVersion` use this counter. Release preparation fetches tags and `cargo xtask check-release VERSION` rejects a counter that does not exceed the nearest prior release tag's committed counter; release tags must identify published releases. Keep pending notes under Unreleased and move them into a dated section matching the next version before release qualification.

Distribution formats are product-specific: Unix CLI and Linux GUI use tar.gz, Windows uses ZIP, macOS GUI uses ZIP through Apple ditto. Packaging resolves `SOURCE_DATE_EPOCH` from HEAD's commit time and rejects conflicting explicit epoch/revision values. A source archive without `.git` requires both `AUDITORYLAB_SOURCE_REVISION` and `SOURCE_DATE_EPOCH` obtained from the release commit; these supplied values identify a claimed source and do not authenticate it. For checkout qualification commit the reviewed candidate first: local dirty-tree packages carry HEAD's base identity and are development evidence only. ZIP stores UTC civil fields rounded down to two-second precision, without a timezone marker; extraction tools may interpret the civil date locally. The common supported epoch range is 1980–2107. Tar members retain exact seconds and zero owner IDs. Repeated archive output over identical staged bytes is tested separately from compilation; it does not establish reproducible binaries.

macOS desktop packaging requires full Xcode 26 or later with `actool` for editable `artwork/auditorylab-icon/assets/macos/AppIcon.icon`; select it with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` or `xcode-select`. CI selects its exact Xcode version from `MACOS_XCODE_VERSION` in ci.yml. No flat-icon fallback is used. The app bundles legal/notices resources before ad-hoc signing. Both CLI architectures are also ad-hoc signed. This provides local signature validity, not trusted Developer ID signing or notarization. Windows native builds require the Windows SDK resource compiler; the pinned build-only embed-resource crate locates it and fails if the required icon cannot be compiled. Linux GUI downloads include the desktop Icon identifier and matching hicolor exports; install these together with the executable. CLI downloads have no unused GUI images.

The root [LICENSE](LICENSE) owns the software/artwork terms. [artwork/README.md](artwork/README.md) identifies retained sources and supplied draft/provenance material; do not apply the supplied draft's unmodified-software-only restriction. Normal app, preview and build code remains MPL-covered.

Generated-output verification includes `cargo test -p auditorylab callback_monitor_costs --release -- --ignored --nocapture` for synthetic callback timing at 48/768 kHz. It opens no device. Run file-scoped mutation checks for numerical/transition changes with timeout 60, output and TMPDIR under target/. Native UI qualification covers SF/Segoe and real GNOME/KDE font changes; cross-compilation alone does not qualify rendering or desktop preferences. Linux runtime font adapters use Pango/GTK/GIO; install `libpango-1.0-0 libgtk-3-0` for those preference sources. Hardware/GUI tests use component-free silent recipes.
