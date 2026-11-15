# Install, update and uninstall the CLI

The installer downloads Auditory Lab for the current user, verifies the archive's
SHA-256 checksum and test-runs the program before replacing an owned installation.
No administrator account, background updater, scheduled task or file association
is needed. The CLI's inputs and outputs are outside the installer's scope.

## Install

From a reviewed source checkout, use `sh install/install.sh` or
`powershell -ExecutionPolicy ByPass -File .\install\install.ps1`. These scripts
download a CLI archive without compiling the application. The release commands
below download the standalone scripts instead.

On macOS or Linux:

```sh
curl -fsSL -o install-auditorylab.sh https://github.com/resoltico/AuditoryLab/releases/latest/download/install.sh &&
sh ./install-auditorylab.sh
```

If curl is unavailable, use
`wget -O install-auditorylab.sh https://github.com/resoltico/AuditoryLab/releases/latest/download/install.sh && sh ./install-auditorylab.sh`. A failed fetch must prevent execution, even when a stale saved script exists. Inspect the saved script before running it; retain it for offline uninstall. A bare `curl | sh` pipeline can hide a failed download behind a successful empty shell.
The installer itself requires standard shell tools, tar and a SHA-256 tool
(`sha256sum`, `shasum` or `openssl`).

On Windows, use PowerShell:

```powershell
& {
    $ErrorActionPreference = 'Stop'
    Invoke-WebRequest -UseBasicParsing https://github.com/resoltico/AuditoryLab/releases/latest/download/install.ps1 -OutFile .\install-auditorylab.ps1 -ErrorAction Stop
    powershell -NoProfile -ExecutionPolicy ByPass -File .\install-auditorylab.ps1
    if ($LASTEXITCODE -ne 0) { throw "Installer failed with exit $LASTEXITCODE" }
}
```

Windows PowerShell 5.1 and PowerShell 7 are supported. `ByPass` applies only to the
launched PowerShell session; it does not change persistent policy or override
Group Policy. Managed systems can still refuse the program.

Supported CLI targets are macOS 15 or later on Apple Silicon and Intel, Linux
x86-64/ARM64 with glibc 2.35 or later and the ALSA library, Windows 10 on
x64, and Windows 11 on ARM64. Linux compatibility is checked by running the verified candidate;
the installer does not reject a distribution by its name. Native CPU selection
also handles translated shells.

The default location is `$XDG_BIN_HOME/auditorylab` when `XDG_BIN_HOME` is set,
otherwise `~/.local/bin/auditorylab`. On Windows it is
`%LOCALAPPDATA%\Programs\AuditoryLab\auditorylab.exe`.
Choose another absolute folder with `--install-dir DIR` or `-InstallDir DIR`.

Open a new terminal after PATH changes, then verify:

```sh
auditorylab --version
auditorylab --help
```

The CLI and desktop app are independent. Installing or removing the CLI
does not install or remove the app. See [CLI usage](cli.md).

The documented Windows command runs in a child process, so its PATH changes do
not reach an already-open parent terminal. Running the script directly in the
current PowerShell session may also update that session's PATH. On macOS/Linux,
use the quoted full executable path printed by the installer while this terminal
keeps its current PATH, or open a new terminal after successful persistence. `--no-modify-path` / `-NoModifyPath` leaves persistent
and current-session PATH unchanged.

## Released installers and this checkout

The public 0.12.0 installer retains its published behavior and has no explicit unmanaged-replacement option. The lifecycle corrections below describe the reviewed checkout and will be available in a future release. To use them before publication, inspect and run `install/install.sh` or `install/install.ps1` from this checkout; do not pass the new flag to an older downloaded script.

This checkout requires the expanded project licensing notices in newly built CLI archives. Immutable public 0.12.0 archives do not contain those added files. Before a future release, download the matching `dist-PLATFORM` CI artifact into a fresh flat directory; it contains the CLI and desktop archives. From the matching reviewed source checkout, run `python3 install/prepare-test-release.py /absolute/qualified-artifacts` to copy its installers and calculate `SHA256SUMS` for this local qualification mirror. Select that directory with `AUDITORYLAB_DOWNLOAD_BASE` and the archive's stable version with `--version` / `-Version`. Obtain the same workflow run's `corresponding-source` artifact. Public install commands above continue to use the published script and compatible published assets. For an older release's archived payload, use that release's reviewed installer; do not edit or replace public assets to satisfy this checkout's package contract.

## Update and choose a release

Run the same install command again. The installer resolves latest once to a stable
`vX.Y.Z` tag, then downloads both archive and `SHA256SUMS` from that release.
A failed download, checksum or candidate test happens before replacing the active
program. An update requires intact verified ownership metadata and every remaining owned file to match its receipt. This checkout also permits repairing or uninstalling an intact receipt whose managed executable is absent; a replacement or changed file is refused.

Select a release explicitly, for example:

```sh
curl -fsSL -o install-auditorylab.sh https://github.com/resoltico/AuditoryLab/releases/download/v0.12.0/install.sh &&
sh ./install-auditorylab.sh --version 0.12.0
```

```powershell
& {
    $ErrorActionPreference = 'Stop'
    Invoke-WebRequest -UseBasicParsing https://github.com/resoltico/AuditoryLab/releases/download/v0.12.0/install.ps1 -OutFile .\install-auditorylab.ps1 -ErrorAction Stop
    powershell -NoProfile -ExecutionPolicy ByPass -File .\install-auditorylab.ps1 -Version 0.12.0
    if ($LASTEXITCODE -ne 0) { throw "Installer failed with exit $LASTEXITCODE" }
}
```

These examples pin both installer and payload to 0.12.0. Substitute the same chosen stable release in both places. If instead you use `AUDITORYLAB_VERSION`, that environment pin remains set in the session; clear it to return to latest:

```sh
unset AUDITORYLAB_VERSION
```

```powershell
Remove-Item Env:AUDITORYLAB_VERSION -ErrorAction SilentlyContinue
```

Keep a copy of the reviewed installer you used, from the selected release tag or
reviewed source checkout. The commands above save one as `install-auditorylab.sh` or `install-auditorylab.ps1`. Do not assume that
fetching the current latest script later gives the same ownership protocol.

The installer stages files privately on the destination filesystem and uses a
per-product, per-directory exclusive lock. A held lock fails promptly; there are
no automatic lock retries. It attempts to restore the prior owned state if
activation fails. Errors describe any remaining state; an interruption or failed
PATH write is not a promise that nothing was installed. Inspect the reported
paths before trying again.

On Windows an update can retain one uniquely named, receipt-owned running
executable. Close processes using it before another update or uninstall if
cleanup is refused. The installer never adopts or deletes arbitrary `.old` files.

## Ownership and unmanaged installations

The installer uses `.auditorylab-install` beside the executable
for ownership receipts and managed data. It verifies managed file hashes before
update or removal and refuses existing unowned or changed files. A matching
filename, version output or PATH entry does not establish ownership.

A receiptless ordinary executable is refused by default, with its exact path and an explicit replacement command. In this checkout, use `--replace-unmanaged` / `-ReplaceUnmanaged` only for an install where private installer state is absent:

```sh
AUDITORYLAB_DOWNLOAD_BASE='/absolute/qualified-artifacts' \
sh install/install.sh --version 0.12.0 --install-dir '/absolute/my tools' --replace-unmanaged
```

```powershell
$env:AUDITORYLAB_DOWNLOAD_BASE = 'C:\QualifiedArtifacts'
powershell -NoProfile -ExecutionPolicy ByPass -File .\install\install.ps1 -Version 0.12.0 -InstallDir 'C:\My Tools' -ReplaceUnmanaged
if ($LASTEXITCODE -ne 0) { throw "Installer failed with exit $LASTEXITCODE" }
```

The option authorizes replacement of only the named ordinary executable. The installer never runs that unowned executable or infers ownership from a version/hash. It verifies the candidate first, locks and rechecks the inspected target immediately before mutation, then preserves the original bytes in a unique recovery backup in the install directory's parent. It reports the exact backup path. A successful install retains the backup; uninstall never deletes it. Existing PATH entries and directory ownership are not adopted. Windows refuses replacement at a filesystem-root install directory because an outside-directory recovery backup cannot be created there.

The flag is install-only. It cannot bypass changed owned payloads, corrupt/incomplete metadata, nonregular targets or access failures. Precise errors name the affected executable, receipt/state directory, lock or retained recovery stage. Preserve those paths and inspect the stated cause; do not edit receipts or hashes, copy ownership metadata to a new directory, or remove an entire shared bin directory.

A normal activation failure restores the prior executable and receipt, preserving prior absence when repairing a missing payload. If rollback is incomplete, the installer retains recovery evidence, reports exact paths and refuses to proceed on retry until that evidence is inspected. Never assume restoration succeeded after an incomplete-rollback error. To recover an original unmanaged backup after successful replacement, first uninstall the new managed executable, then move the exact reported backup back to its original executable path. Review PATH separately.

On Unix, existing directory aliases may be used only when requested and recorded paths resolve to the same physical directory; exact owned PATH text is retained. Windows normalizes ordinary equivalent path spellings such as a trailing `.` but refuses reparse-point install directories; use the ordinary recorded directory command identified by the diagnostic. Moved receipts do not acquire ownership of a new directory.

Pre-existing directories may be shared with other tools. They are never removed
merely because they became empty. Unrelated files are preserved.

## Uninstall

Use the same reviewed installer, saved as `install-auditorylab.sh` / `install-auditorylab.ps1`, to remove
an unchanged installation and only the PATH entries it owns:

```sh
sh ./install-auditorylab.sh --uninstall
```

```powershell
powershell -ExecutionPolicy ByPass -File .\install-auditorylab.ps1 -Uninstall
```

Pass the same custom install directory used for installation, for example
`sh ./install-auditorylab.sh --uninstall --install-dir '/absolute/my tools'` or
`powershell -ExecutionPolicy ByPass -File .\install-auditorylab.ps1 -Uninstall -InstallDir 'C:\My Tools'`.
Removal does not
need a download of the program. An offline saved copy of the installer can be
run with the same uninstall option.

Changed or unowned managed files are refused rather than silently deleted. For
manual cleanup, inspect the executable and its private `.auditorylab-install`
directory, then remove only the files you intend to remove. An installer-created
directory is removed only if empty and never if it is a filesystem root.

Unix PATH cleanup uses the exact recorded startup path and generated line or Fish
entry for this install directory. It preserves other installations and user edits;
symlinked startup files remain symlinks. Windows removes only its exact added user
PATH entry, preserving raw text, empty segments, environment references and value
kind. A PATH entry can be conservatively retained with a warning if unrelated
files remain in the shared install directory; removing it could break other tools.
`NoModifyPath` also suppresses PATH cleanup during
uninstall and reports the entry left for manual removal.

Windows uninstall removes an owned PATH entry before deleting
the executable or receipt. A failed PATH write leaves those files intact for
retry. A later file-removal failure can leave partial progress; inspect the
reported paths before retrying or using the manual cleanup above.

Your recipes, User Library, recordings, exports and desktop preferences are not installer-managed files.

## Options and mirrors

| macOS/Linux | Windows | Environment | Effect |
|---|---|---|---|
| `--version X.Y.Z` | `-Version X.Y.Z` | `AUDITORYLAB_VERSION` | Choose a stable release; one leading `v` is allowed. |
| `--install-dir DIR` | `-InstallDir DIR` | `AUDITORYLAB_INSTALL_DIR` | Choose an absolute directory. |
| `--no-modify-path` | `-NoModifyPath` | `AUDITORYLAB_NO_MODIFY_PATH=1` | Do not change PATH or shell startup files. |
| `--uninstall` | `-Uninstall` | `AUDITORYLAB_UNINSTALL=1` (Windows) | Remove unchanged owned files. |
| `--replace-unmanaged` | `-ReplaceUnmanaged` | | Checkout-only install: back up and replace a receiptless ordinary executable when private installer state is absent. |
| `--help` | `-Help` | | Show usage. |

Pass Unix options to the saved script. In PowerShell use environment variables before
`irm ... | iex`, or parameters on the saved, reviewed script shown above.

`AUDITORYLAB_DOWNLOAD_BASE` selects an HTTPS release mirror or an absolute local
directory containing the release's archive and `SHA256SUMS`. HTTPS is required
at every redirect hop. A mirror is explicitly trusted as the release source;
its checksums do not authenticate its publisher. Mirror URL bases must be directory
URLs without credentials, queries or fragments; use an absolute local release
directory if authenticated downloads are handled separately. Versioned Auditory Lab mirror assets require an explicit stable version through --version or AUDITORYLAB_VERSION.

A PATH entry cannot represent a Unix colon or a Windows semicolon in a directory
name. The installer reports unsupported persistence rather than claiming PATH
was configured. Use the printed, correctly quoted full executable path or choose
another directory. Startup/registry write failures are reported.

## What the installer changes

- The executable at the selected install directory.
- The private `.auditorylab-install` ownership directory. Its receipt is data, not
  an executable shell configuration file.
- `LICENSE`, `README.txt`, `THIRD-PARTY-NOTICES.txt`, `COPYRIGHT-library.html` and
  `SOURCE.txt` are retained under the private directory in `legal/`. The archived
  README describes manual archive layout; use the installer's printed executable
  path for this installation. These files are not runtime dependencies.
  The installer does not install the desktop GUI.
- Only the exact startup entry or user PATH entry added by this installation.

The Unix lock is the sibling `.auditorylab-install.lock`; a normal run removes its
own lock. Locks serialize cooperating installer operations, not hostile processes.
Do not remove a lock while another installer is running. The CLI does not require
installer receipts to run.

## Trust and verification

Both installer and program are code from this repository's release. SHA-256
verification detects damaged or incomplete downloads, not a compromised repository
or mirror. A candidate is executed as the current user only after checksum
verification. A receipt prevents accidental replacement/removal of other files;
it is not protection from a hostile process with your account's permissions.

Published release provenance can be checked with `gh attestation verify <asset>
--repo resoltico/AuditoryLab`. That is a separate optional
verification step; the installer does not claim to authenticate attestations.

The scripts do not disable Gatekeeper, SmartScreen or managed-device policy.
Mac executables are ad hoc signed, without Developer ID or notarization; Windows
executables are unsigned. Browser downloads may carry internet markings and
system policy may block execution even after a matching checksum. Inspect the
candidate diagnostic shown by the installer before its temporary files are cleaned.

## Manual installation

For manual downloads, open [Releases](https://github.com/resoltico/AuditoryLab/releases),
choose the native CPU archive on the matching release tag,
verify its entry in `SHA256SUMS`, then extract into a fresh directory. Keep the
archive's legal material with any copy you redistribute.

## Troubleshooting

If an ownership check fails, inspect the selected path and private directory.
Choose a fresh directory or remove the prior files manually; a version string
is not proof that this installer owns a file. If a lock remains after a crash,
first establish that no installer is using it before manual cleanup.

If the verified candidate cannot run, use its reported diagnostic to check the
CPU, minimum OS/runtime and device policy. Do not bypass checksum verification.
If PATH was not changed, use the quoted full path printed by the installer or
open a new terminal after successful persistence. For shell shadowing use
`command -v auditorylab`; in PowerShell use `Get-Command auditorylab` and remove a
conflicting installation using the method that installed it.

## Archive names

Replace `VERSION` with the selected stable release. These are CLI archives only:

| System | Archive |
|---|---|
| macOS Apple Silicon | `auditorylab-VERSION-macos-aarch64.tar.gz` |
| macOS Intel | `auditorylab-VERSION-macos-x86_64.tar.gz` |
| Linux ARM64 | `auditorylab-VERSION-linux-aarch64.tar.gz` |
| Linux x86-64 | `auditorylab-VERSION-linux-x86_64.tar.gz` |
| Windows ARM64 | `auditorylab-VERSION-windows-aarch64.zip` |
| Windows x64 | `auditorylab-VERSION-windows-x86_64.zip` |

The checksum manifest also lists desktop archives. The CLI installer selects only
its exact CLI filename and requires one valid checksum for it.
