# Releasing and native qualification

[release.py](../scripts/release.py) is the authority for two checked Cargo products and six native targets. `auditory-cli` produces `auditorylab`; `auditory-gui` produces `auditorylab-gui`. Each release has one separate archive per product/target pair plus checksums. Missing, ambiguous or extra distributable binaries reject.

## Identity and archive contents

Both binaries expose `--build-info` before opening a window, reading stdin or querying audio. Its immutable identity includes protocol version, product/package/bin, workspace version, build kind, source revision and Cargo target. Human CLI version and GUI About render that identity. An ordinary Cargo build works without Git and identifies itself as local/unqualified with no source SHA.

Release names/roots are `BINARY-VERSION-PLATFORM`. Candidate names add `-dev-SOURCEPREFIX` after VERSION; local names add `-local`. The extension is ZIP on macOS/Windows and tar.gz on Linux. Binary identity and README retain the full qualified SHA; a short filename prefix is not the source authority. Qualified README source/documentation links use that exact commit, including release builds before a tag exists. Local README identifies the builder's unqualified checkout and does not invent a Git ref or link the same-semver published source as its own.

CLI archives retain exactly the executable, README, LICENSE and THIRD-PARTY-NOTICES. GUI archives contain a launchable application and those documentation/license files: an ad-hoc signed `Auditory Lab.app` bundle on macOS, a GUI-subsystem executable on Windows, or an executable plus minimal desktop entry on Linux. Exact members and bundle metadata are checked by [package.py](../scripts/package.py). There is no installer, updater or elevated setup helper. Ad-hoc signing is not Apple developer signing or notarization.

## One native path

[qualify.py](../scripts/qualify.py) runs formatting, Clippy, documentation, Python tooling, notice freshness, debug/release tests, explicit offline signal characterization, actual window/renderer and nonzero signal-geometry pixel checks, then isolated final builds and extracted-copy package smoke. It retains command logs, renderer observations and package digests under `target/qualification-results`. It never publishes. Choose a fresh output directory for another qualification run; completed archives are never replaced.

Use the native target/host and [verification prerequisites](verification.md). For a local build without an immutable-source claim:

```sh
python3 scripts/qualify.py --target aarch64-apple-darwin --build-kind local --output target/local-packages
```

Choose the native target from release.py; use `python` on Windows. The wrapper supplies isolated per-product target directories, resets identity inputs and ambient deployment settings, and builds CLI and GUI for the common macOS 15.0 minimum on both architectures. release.py is the deployment-policy authority for the compiler environment, Mach-O check, GUI bundle declaration and generated archive instructions. Per-product native import checks remain distinct. A direct unqualified Cargo build uses toolchain defaults and does not establish this packaging baseline. Execution on macOS 15 is separate evidence from declaring that minimum.

The shared minimum is a project support decision. Native jobs use `macos-15` and `macos-15-intel`; runner selection and an executable's checked deployment target are distinct requirements.

Candidate/release qualification requires the full expected SHA and checks clean HEAD before and after its batches. The read-only [candidate wrapper](../.github/workflows/candidate.yml) and reusable [native workflow](../.github/workflows/native.yml) use the selected workflow ref's immutable github.sha for checkout and build identity; they do not accept an unrelated source SHA or a publication switch. Quality calls the same native implementation. Remote qualification requires committed source available to the workflow; local success is not remote qualification.

GPU and forced-software first-frame checks compile the nonshipping fixture first, then set runtime renderer variables only on its executable. The backend line is construction evidence; first-frame results require FPS reporter output for either renderer, because that reporter starts only after a rendered frame. Lazy counts can be zero after an idle interval. Software fallback is labeled WGPU unavailable. A WGPU frame alone does not establish a physical GPU adapter; record hardware_adapter as unverified unless independently established. Linux virtual X11 evidence is labeled as such. Neither startup nor instrumentation establishes keyboard/accessibility/picker behavior, minimum-OS execution or acoustics.

## Windows build contract

Use native GNU LLVM Rust hosts, `x86_64-pc-windows-gnullvm` or `aarch64-pc-windows-gnullvm`, with explicit `--target`. The compiler version is pinned by [rust-toolchain.toml](../rust-toolchain.toml); `python scripts/release.py toolchain --target TARGET` derives its host identifier. The bundled rust-mingw/rust-lld route requires no Visual Studio. [.cargo/config.toml](../.cargo/config.toml) keeps self-contained startup/support and static LLVM unwind; UCRT implementation remains supplied by Windows. Preserve the pinned target's x86_64 CPU instructions and existing Windows floors.

[windows_runtime.py](../scripts/windows_runtime.py) forces the final product link, retains its trace/map and binds its receipt to executable bytes, product and target. Reviewed MinGW/LLVM inputs and selected members remain enforced. GUI-only windows-targets system import archives are bound to exact published producer/VCS identity, archive bytes and member vocabulary; inspection records only import descriptors/short imports, not Windows implementations. No arbitrary extension/directory allowance substitutes for that review.

Packaging requires the product receipt and its separate native import policy; GUI-only imports are not admitted for the CLI. GPU drivers and optional shader compilers can load dynamically: an ordinary import list cannot prove their availability. No driver or static DXC implementation is bundled; selected WGPU can use existing dynamic DXC or OS FXC according to its documented startup behavior. Unknown native inputs, delayed imports, unreviewed runtime DLLs and stale/wrong-product receipts reject. Actual tests on both Windows architectures remain required.

## Linux system font contract

Build from the checkout root so `.cargo/config.toml` applies; invoking Cargo with `--manifest-path` from another directory does not discover that root policy. Do not bypass it with alternate Cargo configuration or compiler/linker flags. Native qualification requires a clean environment without ancestor/CARGO_HOME Cargo config files or compiler/linker/runner overrides; it reports those prerequisites without changing user settings. Both Linux target `links=freetype` overrides select only `dylib=freetype`, suppressing freetype-sys build.rs and its vulnerable bundled C fallback. Install security-updated distribution `libfreetype6-dev` and `libfontconfig1-dev`; the native Ubuntu 22.04 workflow installs them explicitly. Window startup also requires `libxkbcommon0` and, on X11, `libxkbcommon-x11-0`; the native workflow installs both. These dynamically loaded keyboard libraries are required for software drawing too, and ELF import inspection alone cannot prove their presence. This uses the maintained distribution ABI, not a bundled FreeType implementation or a requirement for upstream version numbers to match distro backports.

GUI packaging requires actual ELF DT_NEEDED entries for `libfreetype.so.6` and `libfontconfig.so.1`, while retaining the GLIBC 2.35 floor and no RPATH/RUNPATH policy. The native qualifier records pkg-config and distro package versions. Distribution documentation retains the FreeType Team acknowledgment and full reviewed reference FreeType/Fontconfig terms in a separately labeled external-system-library section; system linkage does not turn those engine terms into the Rust bindings' MIT grant. Source/profile rejection controls and a real Cargo panic-build-script fixture prove the override mechanism; native Linux linking, system package selection and Wayland decoration behavior still require the Linux jobs. macOS success cannot establish them.

## Public release procedure

Publication is a separate explicitly authorized operation. Development qualification has no write permissions or release step. Before an intended public release, set the workspace version, refresh workspace lock entries normally, consolidate Unreleased outcomes into one dated changelog section, run verification and review `python3 scripts/release.py notes --version VERSION`. Commit/push main and dispatch release.yml only when authorized:

```sh
gh workflow run release.yml --repo resoltico/AuditoryLab --ref main -f version=VERSION
```

Prepare requires main, exact clean source, matching product versions, dated notes and an absent remote tag. The release wrapper calls the shared native path with release-kind identity; it does not rebuild successful packages before upload. Build/package jobs have read permissions. Attestation and contents-write publication remain isolated in the release wrapper.

Downloaded artifacts remain in separate product/platform directories. Validation checks every assigned archive, source/product/target/build-kind identity, exact members and bundle metadata before flattening; missing, duplicate, extra, nested and misassigned artifacts reject. A same-semver candidate cannot pass as a release. Keep `digest-mismatch: error`, SHA256SUMS, provenance attestations, draft creation and the uploaded digest comparison before making the draft public. Published tags/assets are never replaced.

### When publishing fails

If publication fails, inspect server state first: a failed command may follow successful publication. Verify a public release and correct it only in a new version. For a remaining draft, delete only that draft before retrying with the already qualified artifacts from the same run. If rebuilding is necessary, rerun qualification for the entire product/target set.

## Notices and dependency updates

Regenerate `python3 scripts/notices.py` after a normal dependency, source patch or generated-resource change, then review its diff. Both exact product roots come from the same target-filtered Cargo metadata. Cargo owns selected feature/target resolution: normal material is retained, and `normal,no-proc-macro` separately identifies runtime reachability. Host-only producer material is labeled without claiming its compiled code ships or denying obligations for emitted code. Explicit build/dev-only edges are excluded; generated widget/resource grants are reviewed separately.

[supplements.json](../scripts/licenses/supplements.json) binds otherwise missing legal material to published version/repository/VCS/source identity. Slint's selected Royalty-free Desktop, Mobile, and Web Applications License 2.0, AboutSlint attribution and generated assets are recorded in [desktop-review.md](../scripts/licenses/desktop-review.md). The narrow backend and manifest-only SCTK patches retain every upstream source/legal file, including the inactive Cantarell font and its full SIL OFL terms; its exact base, reviewed patch and reverse reconstruction are checked rather than omitted as source=None.

The notice file retains complete official Rust component and MinGW/LLVM bundles, including material beyond a particular executable, and prints ordinary legal text once. A freshness/hash/identity check establishes neither review quality nor legal completeness. Inspect grants, generated/native inputs and actual qualification evidence when changing dependencies or toolchains.

The lock deliberately selects typed-index-collections 3.2.3 within the Slint compiler's declared ^3.2 range. Source/API review found no relevant missing correctness fix for the compiler's usage; this avoids later optional bincode retained in the whole lockfile without stripping features or adding an unused direct dependency. It is not a promise of a separately maintained 3.2 branch. Review that decision when updating the compiler or producer.

## Verify published bytes

Check every downloaded checksum and verify archive provenance against the exact release commit:

```sh
gh attestation verify ARCHIVE --repo resoltico/AuditoryLab \
  --signer-workflow resoltico/AuditoryLab/.github/workflows/release.yml \
  --source-ref refs/heads/main --source-digest COMMIT
```

Checksums establish byte agreement; attestations bind the workflow/source. Neither proves native interaction, all graphics hardware, assistive technology or acoustic behavior was tested.
