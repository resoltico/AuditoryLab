# Packaging and releases

## Build distributions

Use the [source prerequisites](../README.md#build-from-source) and pinned [toolchain](../rust-toolchain.toml). Install cargo-about at the version in [CI](../.github/workflows/ci.yml), with its `cli` feature; contributor checks also use the pinned cargo-deny and cargo-mutants versions there.

```sh
cargo xtask dist --target TARGET
```

Run from the repository root. This builds both CLI and desktop with the distribution profile, generates notices and README files, writes archives under `target/dist`, extracts them, checks delivered content/identity and runs host CLI smoke checks. The [supported target/product mapping](../xtask/src/targets.rs) defines the complete set. Build and check each target on its matching OS and CPU; package inspection runs the extracted executable, so cross-compilation alone is insufficient. Failed staging inputs remain under `target/dist-stage` for diagnosis.

macOS icon compilation needs full Xcode 26 or later with `actool` on macOS 26 or later. Select it with `DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer` or `xcode-select`; CI selects the version declared by `MACOS_XCODE_VERSION`. To assemble the app on another supported native macOS host, supply matching precompiled resources: `cargo xtask compile-icons OUTPUT` followed by `cargo xtask dist --target TARGET --icons OUTPUT`. The resource input must match the exact source revision and commit epoch and contain the required native icon outputs. CI transfers these resources through an integrity-checked artifact to both macOS 15 native builders, which still run the extracted-app checks. App deployment remains macOS 15. Without `--icons`, distribution compiles the layered icon on its own host. Both `aarch64-apple-darwin` and `x86_64-apple-darwin` are single-architecture packages. Windows desktop builds need the Windows SDK resource compiler, which the build dependency locates; a missing compiler or icon fails the build.

To inspect notices without building archives, use `cargo xtask notices cli TARGET target/cli-notices.txt`, or substitute `gui`. Notice generation checks the selected non-dev Cargo graph and required license texts. Each archive includes the pinned compiler's original `COPYRIGHT-library.html` for Rust components outside that graph.

## Formats and installed resources

| Product | macOS | Linux | Windows |
| --- | --- | --- | --- |
| CLI, `auditorylab` | tar.gz | tar.gz | ZIP |
| Desktop, `auditorylab-gui` | ZIP | tar.gz | ZIP |

Names use `PRODUCT-VERSION-PLATFORM`, with a matching top-level folder. [xtask](../xtask/src/archive.rs) writes tar and Windows ZIP members in-process with normalized permissions. macOS desktop ZIP creation uses Apple `ditto`, then preserves its compressed members/attributes while normalizing member order and incidental metadata. Extraction uses `ditto` for those app archives. The extracted executable, architecture, permissions, legal materials, source reference and platform icon resources are checked before an archive is accepted.

All archives contain `LICENSE`, `README.txt`, `THIRD-PARTY-NOTICES.txt`, `COPYRIGHT-library.html` and `SOURCE.txt`. CLI downloads contain no unused GUI images. The macOS app also retains its legal materials in `Contents/Resources`, independently of the outer folder; its About/license actions resolve them after moving only the app. Source-built desktops embed the root license, but full generated component notices require packaging.

The [editable artwork](../artwork/README.md) supplies the runtime icon, native Windows executable resource and Linux hicolor tree. macOS packaging compiles `AppIcon.icon` into `Assets.car` and `AppIcon.icns` before app signing; it uses no flat-icon fallback. Keep the Linux executable and legal materials in one folder on PATH, and install the `.desktop` file and hicolor tree as described in the [desktop guide](desktop.md#platforms).

Both macOS CLI architectures and the app are ad-hoc signed and verified after extraction. This checks local signature validity; it does not provide Developer ID signing or notarization.

## Source and build identity

`workspace.package.version` and `workspace.metadata.auditorylab.build-number` in root [Cargo.toml](../Cargo.toml) are the release version and the sole product build counter. Increase the counter (1..9999) before distributing changed product binaries; keep it increasing across release versions. CLI, GUI and CPU variants share it. Private/source rebuilds and CI reruns reuse the committed value. Source revision and archive checksums identify source and exact delivered bytes separately.

The app build script reads `workspace.metadata.auditorylab.product-name` and `copyright` for display identity, and regenerates identity when the manifest or `AUDITORYLAB_SOURCE_REVISION` changes. LICENSE supplies legal terms independently of display metadata. Supplied revisions must be full 40- or 64-character ASCII hexadecimal IDs. CLI/GUI `--version` includes the source revision when supplied; About shows version/build. macOS `CFBundleVersion` uses the build counter, and `CFBundleShortVersionString` uses the release version.

Distribution resolves source revision and `SOURCE_DATE_EPOCH` from HEAD and its commit time, rejecting conflicting supplied values. A source tree without `.git` requires both values from the release commit; these inputs identify a claimed source and do not authenticate it. Commit reviewed source before native checks: dirty-tree packages still carry HEAD's base identity and are development evidence only. `SOURCE.txt` records the revision, source URL, version, build and epoch.

Archive times accept 1980–2107, the common ZIP range. Tar members retain exact epoch seconds and zero owner IDs. ZIP stores UTC civil fields rounded down to two-second precision, without a timezone marker; extraction tools may interpret those fields locally. Repeated archive output over identical staged bytes is tested separately from compilation and does not establish reproducible binaries.

## Prepare and qualify a release

Keep pending changes under `Unreleased` in [CHANGELOG.md](../CHANGELOG.md). For an authorized release, set the workspace version and move the reviewed cumulative entry into a dated section matching it. Review `cargo xtask notes VERSION`. Commit the reviewed candidate on main and push. A release-preparation commit may use `[skip ci]` to avoid duplicate push-triggered native checks; workflow dispatch still runs all release checks.

Check the hosted release/assets and failed-publication state as well as tags before selecting the published baseline. `cargo xtask check-release VERSION` validates the requested workspace version, dated nonempty notes, compiler notices and an increased build counter against the nearest ancestor `v[0-9]*` tag. That tag lookup does not prove publication; verify its release status and resolve any stray or failed-publication tag before dispatch. The workflow fetches full history/tags and refuses an existing target tag.

Dispatch [release.yml](../.github/workflows/release.yml) with `publish: false` (the default) to inspect a verification bundle, or `publish: true` only when publication is authorized. Verification may run on a branch; publication requires main. Both modes compile shared icon resources on macOS 26 before native packaging/mutation jobs, call every native CI target and file-scoped mutation job, collect the exact archive set through `cargo xtask collect-assets VERSION INPUT OUTPUT`, write checksums/notes and attest them. Missing, duplicate, stale or unexpected assets are rejected.

The `release-verification` artifact contains `assets/`, `notes.md` and `provenance.jsonl`. A read-only job downloads it and verifies checksums and attestations against the source commit/ref and release workflow. Notes are verified alongside archive/checksum files. Verification creates workflow artifacts and attestations, without a tag or draft release. Review the intended commit's bundle and repeat package checks if its source changes.

## Publication and recovery

The publishing job downloads the verified bytes and rechecks their checksums. It checks that main still names the checked commit, creates a draft release, compares uploaded digests with the built bytes, then publishes. It never replaces published tags or assets.

If publication stops at a draft, inspect the failed step and its logs and compare every uploaded digest and source commit. Resolve recovery of that draft explicitly; a failed job alone does not establish whether server-side publication occurred. Changed published bytes need a new version.

To verify a public download, compare its `SHA256SUMS` entry and GitHub asset digest, verify its attestation against the release workflow/source commit, then inspect the extracted binary's `--version` and `SOURCE.txt`.

Before native distribution, run `cargo xtask verify-vendor` and the [callback cost check](../CONTRIBUTING.md#callback-cost-check). Normal CI also checks deterministic synthesis, callback allocation/free behavior, lifecycle and publication rules. Inspect packages built from the intended source and check their native UI behavior.
