# Releasing

[release.yml](../.github/workflows/release.yml) qualifies and publishes a manually requested version from `main`. [rust-toolchain.toml](../rust-toolchain.toml) selects Rust; the workflow pins Python and official action commits. [release_archive.py](../scripts/release_archive.py) defines the six native targets and their archive names. Windows x64/ARM64, Linux x64/ARM64 and macOS Intel/Apple Silicon are separate packages. Mac packages require macOS 15 or later and are separate thin, ad hoc signed ZIPs; no universal binary, Apple signing certificate or notarization is used.

[.python-version](../.python-version) selects CI's release-tool interpreter. Local release-tool tests require Python 3.11 or later. The source-notice coverage test reads the authenticated locked libm crate from Cargo's cache; the standard build/checks populate it. A missing cache is an actionable failure, not a skipped check; the test does not fetch dependencies or use the network.

Archive packaging and verification also require the pinned Rust/Cargo toolchain. They resolve the complete target dependency closure and authentic legal-file bytes from the locked sources; Cargo can fetch missing locked crate sources. The publisher installs the prepared pinned toolchain before verifying archives. These are contributor/release tools, not requirements for running the downloaded application.

Windows qualification and extraction require [PowerShell 7 or later](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows), invoked as `pwsh`. The workflow checks its version before qualification, and the native helper and archive extraction reject an older or missing shell. Recommend the current stable release. PowerShell is verification tooling; it is not bundled with the executable.

## Prepare and dispatch

Set the workspace version in [Cargo.toml](../Cargo.toml) and add one dated section beneath Unreleased in [CHANGELOG.md](../CHANGELOG.md). Its nonempty body becomes the exact GitHub release notes. Do not generate a commit inventory. Run the [standard checks](verification.md#standard-checks) and release-tool tests:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -v
python3 scripts/release.py notes --version 0.1.0
```

Commit the complete reviewed tree, push `main`, then dispatch the version from that branch:

```sh
gh workflow run release.yml --repo resoltico/AuditoryLab --ref main -f version=0.1.0
gh run list --repo resoltico/AuditoryLab --workflow release.yml
gh run view RUN_ID --repo resoltico/AuditoryLab
```

Replace `RUN_ID` with the resulting run identifier. Inspect failed job logs and actual artifact/test discovery. Dispatching or creating a draft does not establish release completion.

## Qualification and publication

The same-source reusable Quality workflow retains formatting, lint, tests, documentation, advisory audit and two bounded single-file mutation jobs. Six native runners also test the release profile, assert compiler host/executable architecture, inspect runtime dependencies and package the CLI with instructions, source identity and applicable license notices. Windows uses the selected toolset's static retail CRT. Mac signing happens after binary changes. Extraction tests the delivered archive away from checkout assets without opening audio; each Windows package also runs the six native interruption cases. Ignored physical tests remain unexecuted.

GitHub generates the job-scoped `GITHUB_TOKEN`; no personal token or Apple credentials are stored as repository secrets. Builds have only the permissions needed for file provenance. Only the publisher can write release content. It verifies source/workflow/hosted-runner provenance, the six archives and exact notes, then assembles a draft. Publication requires the actual tag and `main` to match the qualified commit and exactly the six archives plus `SHA256SUMS` to match downloaded remote bytes. Checksums establish content integrity; they do not establish test execution or acoustic effects.

Notices come from each target's resolved normal/build Cargo dependency closure, with upstream filenames preserved regardless of case. [upstream-notices.json](../scripts/licenses/upstream-notices.json) binds reviewed supplemental legal material to the exact package, repository and crate VCS identity. Packaging verifies those hashes offline and rejects unknown or changed inputs. The Objective-C materials retain upstream's Apple SDK disclosure and its MIT grant; the separate standard MIT text retains its authentic template without invented copyright details. Libm retains its full license and a compact bundle of complete verbatim source copyright/permission headers; its implementations and tests are not packaged. Its policy also pins the locked crate checksum, and a separate source-coverage test challenges the bundle. Rust's exact standard-library copyright file is included. Windows packages include the selected runtime's complete terms and attribution.

Archives contain one executable, UTF-8 guides/build identity and applicable legal materials. Their file types, paths, permissions and complete notice identities/bytes are checked against trusted source materials, independently of self-described file hashes. File paths cannot also serve as parent directories. Linux tar entries use numeric owner/group zero and empty owner names. Build/test tools, source trees and scratch reports are not binary-package contents; executable symbols useful for diagnostics are retained.

When updating dependencies, inspect the actual published crate notices and `.cargo_vcs_info.json`, including notices embedded in source comments. If a required notice is omitted, retrieve it from the exact immutable upstream commit, review the grant and referenced terms, update the narrowly scoped supplement material/identity/hash together, and run its rejecting controls plus native packaging. A libm update requires a complete source-notice review and coverage update. Do not synthesize a copyright notice or bypass a missing-license failure.

Version-scoped concurrency serializes these publishers. GitHub does not provide an atomic transaction across branch/tag refs and release visibility: do not change the qualified refs concurrently during publication. The publisher rechecks both refs before and after visibility changes. Repository immutable releases protect published tags and assets. Verify that setting and the final release's immutable flag. Never retarget or overwrite a published version.

## Retry and recovery

A publisher-only retry must reuse the original qualified staging artifacts. Existing uploaded draft assets are downloaded and compared, and only absent assets are uploaded. An empty failed starter asset may be removed within that verified draft. Unexpected assets, conflicting notes/source or differing completed bytes stop publication and preserve the draft for review.

Rebuilding the same source can change archive bytes. Do not assume rerunning every job can resume a partially uploaded draft. For explicit recovery, first inspect and retain evidence; remove only the verified owned unpublished draft and its unpublished tag, then requalify. Never apply that recovery to a published release. A lost API response is ambiguous; inspect remote state before retrying. A published matching version is accepted only after complete identity/content verification.

## Verify the public result

Check the run's required gates, public release visibility, tag commit, exact notes, seven asset names and the six provenance records. Independently download all assets and verify `SHA256SUMS`; inspect package build identities and architectures. Verify the Mac signatures after extraction. Native runner execution qualifies those runner environments, not every minimum OS version or physical playback setup. Keep reports under `target/` and clean only owned scratch.

Use the archive verifier and notice policy from that release's qualified source commit. Package content contracts can change with source; a verifier from mutable `main` is not an archive reader for all published layouts. Review an immutable release without replacing its assets or retargeting its tag.

For provenance verification, replace `ARCHIVE` and `SOURCE_SHA` with the downloaded archive and release's exact commit:

```sh
gh attestation verify ARCHIVE --repo resoltico/AuditoryLab \
  --signer-workflow resoltico/AuditoryLab/.github/workflows/release.yml \
  --signer-digest SOURCE_SHA --source-digest SOURCE_SHA \
  --source-ref refs/heads/main --deny-self-hosted-runners
```

Platform requirements and tested-environment distinctions are in each archive's `INSTALL.txt`. Linux requires the documented system ALSA/glibc runtime. Windows includes separate Microsoft runtime terms/attribution. Mac ad hoc signatures verify integrity and do not constitute Developer ID signing or Apple notarization. Managed security policies may prevent launching an unnotarized download.
