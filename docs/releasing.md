# Packaging and releases

## Build distributions

Use the [source prerequisites](../README.md#build-from-source) and pinned [toolchain](../rust-toolchain.toml). Install cargo-about at the version in [CI](../.github/workflows/ci.yml), with its `cli` feature; contributor checks also use the pinned cargo-deny and cargo-mutants versions there.

```sh
cargo xtask dist --target TARGET
```

Run from the repository root. This builds both CLI and desktop with the distribution profile, generates notices and README files, writes archives under `target/dist`, extracts them, checks delivered content/identity and runs host CLI smoke checks. The [supported target/product mapping](../xtask/src/targets.rs) defines the complete set. Build and check each target on its matching OS and CPU; package inspection runs the extracted executable, so cross-compilation alone is insufficient. Failed staging inputs remain under `target/dist-stage` for diagnosis.

### Native toolchains

macOS icon compilation needs full Xcode 26 or later with `actool` on macOS 26 or later. Select it with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` or `xcode-select`; CI selects the version declared by `MACOS_XCODE_VERSION`. To assemble the app on another supported native macOS host, supply matching precompiled resources: `cargo xtask compile-icons OUTPUT` followed by `cargo xtask dist --target TARGET --icons OUTPUT`. The resource input must match the exact source revision and commit epoch and contain the required native icon outputs. CI transfers these resources through an integrity-checked artifact to both macOS 15 native builders, which still run the extracted-app checks. App deployment remains macOS 15. Without `--icons`, distribution compiles the layered icon on its own host. Both `aarch64-apple-darwin` and `x86_64-apple-darwin` are single-architecture packages.

Both Windows products need the Windows SDK resource compiler for version information. The desktop also embeds its icon. Missing required resources or a resource compiler fail the build.

To inspect notices without building archives, use `cargo xtask notices cli TARGET target/cli-notices.txt`, or substitute `gui`; add a built executable as a final argument to include its embedded fonts. Notice generation checks the selected non-dev Cargo graph and required license texts. Auditory Lab's own crates are covered by `LICENSE` and are not listed as third parties. Each MPL-2.0 component names where its source code is published. Fonts compiled into the executable are found by their font structure and credited from their own copyright and license records; a font without a copyright or under a license other than the SIL Open Font License 1.1 stops packaging. Each archive includes the pinned compiler's original `COPYRIGHT-library.html` for Rust components outside that graph.

## Formats and installed resources

| Product | macOS | Linux | Windows |
| --- | --- | --- | --- |
| CLI, `auditorylab` | tar.gz | tar.gz | ZIP |
| Desktop, `auditorylab-gui` | ZIP | tar.gz | ZIP |

Names use `PRODUCT-VERSION-PLATFORM`, with a matching top-level folder. [xtask](../xtask/src/archive.rs) writes tar and Windows ZIP members in-process with normalized permissions. macOS desktop ZIP creation uses Apple `ditto`, then preserves its compressed members/attributes while normalizing member order and incidental metadata. Extraction uses `ditto` for those app archives. The extracted executable, architecture, permissions, legal materials, source reference and platform icon resources are checked before an archive is accepted.

All archives contain `LICENSE`, `README.txt`, `THIRD-PARTY-NOTICES.txt`, `COPYRIGHT-library.html` and `SOURCE.txt`. CLI downloads contain no unused GUI images. The desktop embeds its runtime icon and draws learning diagrams from application-owned data using native paths, text and palette colors. The twelve standalone onboarding SVG exports, editable masters, optional MPL exporter and reference compositions stay in the source tree. See the [onboarding artwork guide](../artwork/onboarding/README.md) for source ownership, generation and the independent verification command. The macOS app also retains its legal materials in `Contents/Resources`, independently of the outer folder; its About/license actions resolve them after moving only the app. Source-built desktops embed the root license, but full generated component notices require packaging.

The [editable artwork](../artwork/README.md) supplies the runtime icon, native Windows executable resource and Linux hicolor tree. macOS packaging compiles `AppIcon.icon` into `Assets.car` and `AppIcon.icns` before app signing; it uses no flat-icon fallback. Keep the Linux executable and legal materials in one folder on PATH, and install the `.desktop` file and hicolor tree as described in the [desktop guide](desktop.md#platforms).

Both macOS CLI architectures and the app are ad-hoc signed and verified after extraction. This checks local signature validity; it does not provide Developer ID signing or notarization. The app's `Contents/Resources` holds exactly the legal files and compiled icons; the icon compiler's partial plist is merged into `Info.plist`. `Info.plist` declares `NSSupportsAutomaticGraphicsSwitching`, so the OpenGL renderer keeps dual-GPU Macs on their integrated GPU.

Windows executables embed an application manifest declaring Windows 10/11 support, long-path awareness and `asInvoker`; the desktop's also binds Common Controls 6, which native confirmation dialogs need for their Save/Discard/Cancel buttons, and declares per-monitor DPI awareness. Both link the Microsoft C runtime statically and are built with Control Flow Guard. Package inspection requires ASLR, high-entropy ASLR, DEP and Control Flow Guard, no dynamically linked C runtime and the exact manifest.

The Linux desktop archive's `auditorylab-gui.desktop` names `StartupWMClass=auditorylab-gui`, matching the XDG app ID the desktop sets at startup, so Wayland and X11 desktops match its window to the launcher and icon.

## Source and build identity

`workspace.package.version` in root [Cargo.toml](../Cargo.toml) is the only maintained version. About shows the version and an optional 12-character source revision; --version retains the full revision. macOS CFBundleVersion is derived as X*10000 + Y*100 + Z; packaging rejects minor or patch over 99 and any version suffix. Windows executables carry X.Y.Z.0 version resources and product identity.

The app build script reads `workspace.metadata.auditorylab.product-name` and `copyright` for display identity, and regenerates identity when the manifest or `AUDITORYLAB_SOURCE_REVISION` changes. LICENSE supplies legal terms independently of display metadata. Supplied revisions must be full 40- or 64-character ASCII hexadecimal IDs. CLI/GUI `--version` includes the source revision when supplied; About shows the version and an optional short source revision. macOS `CFBundleShortVersionString` uses the release version. The GUI and CLI build scripts validate the same root metadata before generating Windows resources.

Distribution builds remap the checkout to `/auditorylab` and the Cargo home to `/cargo` in compiled source locations, through a target-specific `--config` that keeps `.cargo/config.toml` flags; package inspection rejects executables containing either build-host path. Distribution resolves source revision and `SOURCE_DATE_EPOCH` from HEAD and its commit time, rejecting conflicting supplied values. A source tree without `.git` requires both values from the release commit; these inputs identify a claimed source and do not authenticate it. Commit reviewed source before native checks: dirty-tree packages still carry HEAD's base identity and are development evidence only. `SOURCE.txt` records the revision, source URL, version and epoch.

Archive times accept 1980–2107, the common ZIP range. Tar members retain exact epoch seconds and zero owner IDs. ZIP stores UTC civil fields rounded down to two-second precision, without a timezone marker; extraction tools may interpret those fields locally. Repeated archive output over identical staged bytes is tested separately from compilation and does not establish reproducible binaries.

## Prepare and qualify a release

Keep pending changes under `Unreleased` in [CHANGELOG.md](../CHANGELOG.md). For an authorized release, set the workspace version and move the reviewed cumulative entry into a dated section matching it. Review `cargo xtask notes VERSION`. Complete dependency and documentation review before freezing the candidate. Commit the reviewed candidate on main and push. Use `[skip ci]` on an authorized release-preparation push when the release workflow will perform qualification, so ordinary push CI does not duplicate the same native checks. Do not create a pull-request qualification run solely to repeat the release checks.

Check the hosted release/assets and failed-publication state as well as tags before selecting the published baseline. `cargo xtask check-release VERSION` validates the requested workspace version, dated nonempty notes, compiler notices. Verify the baseline release status and resolve any stray or failed-publication tag before dispatch.

Both verification and publication require an unused target version with matching dated notes; neither mode accepts an existing target tag. For an authorized public release, dispatch [release.yml](../.github/workflows/release.yml) once with `publish: true`: it qualifies the source, verifies the exact generated assets, then publishes those same bytes. A separate `publish: false` run is optional when a verification-only bundle is needed; it is not a prerequisite for publication. Verification may run on a branch; publication requires main. Both modes compile shared icon resources on macOS 26 before native packaging/mutation jobs, call every native CI target and file-scoped mutation job, collect the exact archive set through `cargo xtask collect-assets VERSION INPUT OUTPUT`, write checksums/notes and attest them. Missing, duplicate, stale or unexpected assets are rejected.

The `release-verification` artifact contains `assets/`, `notes.md` and `provenance.jsonl`. A read-only job downloads it and verifies checksums and attestations against the source commit/ref and release workflow. Notes are verified alongside archive/checksum files. Verification creates workflow artifacts and attestations, without a tag or draft release. Review the intended commit's bundle and repeat package checks if its source changes.

## Publication and recovery

The publishing job downloads the verified bytes and rechecks their checksums. It checks that main still names the checked commit, creates a draft release, compares uploaded digests with the built bytes, then publishes. It never replaces published tags or assets.

If publication stops at a draft, inspect the failed step and its logs and compare every uploaded digest and source commit. Resolve recovery of that draft explicitly; a failed job alone does not establish whether server-side publication occurred. Changed published bytes need a new version.

To verify a public download, compare its `SHA256SUMS` entry and GitHub asset digest, verify its attestation against the release workflow/source commit, then inspect the extracted binary's `--version` and `SOURCE.txt`.

Release qualification runs `cargo xtask verify-vendor`, the [callback cost check](../CONTRIBUTING.md#callback-cost-check), deterministic synthesis, callback allocation/free checks, lifecycle/publication rules, scoped mutations and native package checks. Local checks are useful during development; they need not be duplicated solely to prepare an unchanged authorized release candidate. Inspect the intended source's hosted job results and verify public downloads after publication. Keep main stable while qualification and publication run. If a source correction is required, stop publication from that candidate and qualify the corrected commit; a retry cannot transfer evidence across changed source.
