# Install, update and uninstall the CLI

The installer downloads Auditory Lab for the current user, verifies the archive's
SHA-256 checksum and test-runs the program before replacing an owned installation.
No administrator account, background updater, scheduled task or file association
is needed. The CLI's inputs and outputs are outside the installer's scope.

## Existing executable without an installer receipt

`Existing executable has no installer receipt` means a file already exists at
the printed path, but this installer has no record of managing it. This does not
identify how the file was installed. The refusal changes no program or PATH entry.

Choose whether to replace that executable or keep it:

- **Replace it:** use the complete command printed by the installer. It keeps
  the selected directory, version pin and choice to leave PATH unchanged. It
  authorizes backing up and replacing only the named executable after the new
  candidate verifies.
- **Keep it:** install into another directory and use that installation's full
  executable path. Avoid adding a second command with the same name to PATH
  until you have chosen which installation should be used.

For the default directory and a saved installer, explicit replacement is:

```sh
sh ./install-auditorylab.sh --install-dir "$HOME/.local/bin" --replace-unmanaged
```

```powershell
powershell -NoProfile -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1') -InstallDir "$env:LOCALAPPDATA\Programs\AuditoryLab" -ReplaceUnmanaged
if ($LASTEXITCODE -ne 0) { throw "Installer failed ($LASTEXITCODE)." }
```

These examples explicitly select the usual default directories. If
`XDG_BIN_HOME`, another directory, a version pin or disabled PATH changes were
selected, use the printed options. If the quick installer ran through stdin
and has no saved file, [download and review the installer](#install), then apply
those options to your saved script. On Windows, quick install retains the
temporary script on installer failure so its printed retry command can use it.
After you finish retrying, you can delete that temporary script and its directory.
Keep mirror configuration in the environment; recovery commands do not print it.

To keep the existing executable, choose a fresh directory, for example:

```sh
sh ./install-auditorylab.sh --install-dir "$HOME/auditorylab-cli" --no-modify-path
```

```powershell
powershell -NoProfile -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1') -InstallDir "$env:LOCALAPPDATA\AuditoryLab-CLI" -NoModifyPath
if ($LASTEXITCODE -ne 0) { throw "Installer failed ($LASTEXITCODE)." }
```

Retain any selected version option when changing directories. After a successful
replacement, keep the original backup at its exact reported path until you have
verified the new executable. Uninstall does not remove this backup. Run the
printed full executable path with `--version` and `--help`; this checks the
installation you just selected rather than another command on PATH. For the
default directories:

```sh
"$HOME/.local/bin/auditorylab" --version
"$HOME/.local/bin/auditorylab" --help
```

```powershell
& "$env:LOCALAPPDATA\Programs\AuditoryLab\auditorylab.exe" --version
& "$env:LOCALAPPDATA\Programs\AuditoryLab\auditorylab.exe" --help
```

To restore the original after a successful replacement, first [uninstall](#uninstall)
the new managed copy from the same directory. Continue only after uninstall
succeeds; then move the exact reported backup to the original executable path.
Review PATH separately: pre-existing entries were not adopted, and entries added
by the new installation may have been removed. If replacement or restoration
failed, preserve the reported recovery files and follow
[ownership and unmanaged installations](#ownership-and-unmanaged-installations)
before retrying. The replacement option cannot bypass changed managed files,
corrupt state, symlinks, nonregular targets or access failures.

If this error occurs during **uninstall**, the installer cannot remove that
untracked executable or its PATH entries. Use the method that installed it, or
inspect and preserve the file before deciding on manual removal. Replacement is
an installation action, not a way to uninstall an untracked executable.

Published installers link Help to their source commit, so it stays matched to
that installer. Scripts run from a source checkout link to the current `main`
guide. Keep the saved installer for later update or offline removal.

## Install

From a reviewed source checkout, use `sh install/install.sh` or
`powershell -ExecutionPolicy ByPass -File .\install\install.ps1`. These scripts
download a CLI archive without compiling the application. For a quick install
from a release, use the [README commands](../README.md#download). They run the
installer after a successful download and leave no installer script after success.
To review the installer, choose custom options or keep a copy for offline removal,
save it using the commands below.

On macOS or Linux:

```sh
curl --proto '=https' --proto-redir '=https' -fsSL -o install-auditorylab.sh https://github.com/resoltico/AuditoryLab/releases/latest/download/install.sh &&
  less ./install-auditorylab.sh &&
  sh ./install-auditorylab.sh
```

If curl is unavailable, use `wget --https-only -O install-auditorylab.sh URL`
with the same URL and the same `&&` chain. Run the script only after a
successful download and review.
The installer itself requires standard shell tools, tar and a SHA-256 tool
(`sha256sum`, `shasum` or `openssl`).

On Windows, save `install-auditorylab.ps1` in your home directory and display it
for review.

```powershell
& {
    $ErrorActionPreference = 'Stop'
    $installer = Join-Path $HOME 'install-auditorylab.ps1'
    Invoke-WebRequest -UseBasicParsing -Uri https://github.com/resoltico/AuditoryLab/releases/latest/download/install.ps1 -OutFile $installer
    Get-Content -LiteralPath $installer
}
```

After the download succeeds and you review the script, run it in a child session.

```powershell
powershell -NoProfile -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1')
if ($LASTEXITCODE -ne 0) { throw "Installer failed ($LASTEXITCODE)." }
```

If you save the reviewed installer elsewhere, substitute its full path in the
Windows examples that reuse it.

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

## Update and choose a release

Run the same install command again. The installer resolves latest once to a stable
`vX.Y.Z` tag, then downloads both archive and `SHA256SUMS` from that release.
A failed download, checksum or candidate test happens before replacing the active
program. An update requires intact verified ownership metadata and every remaining owned file to match its receipt. The installer also permits repairing or uninstalling an intact receipt whose managed executable is absent; a replacement or changed file is refused.

Select a release explicitly, for example:

```sh
curl --proto '=https' --proto-redir '=https' -fsSL -o install-auditorylab.sh https://github.com/resoltico/AuditoryLab/releases/download/v0.14.0/install.sh &&
  less ./install-auditorylab.sh &&
  sh ./install-auditorylab.sh --version 0.14.0
```

```powershell
& {
    $ErrorActionPreference = 'Stop'
    $installer = Join-Path $HOME 'install-auditorylab.ps1'
    Invoke-WebRequest -UseBasicParsing -Uri https://github.com/resoltico/AuditoryLab/releases/download/v0.14.0/install.ps1 -OutFile $installer
    Get-Content -LiteralPath $installer
}
```

After you review the downloaded Windows script, run it with the same version.

```powershell
powershell -NoProfile -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1') -Version 0.14.0
if ($LASTEXITCODE -ne 0) { throw "Installer failed ($LASTEXITCODE)." }
```

These examples pin both installer and payload to 0.14.0. Substitute the same chosen stable release in both places. If instead you use `AUDITORYLAB_VERSION`, that environment pin remains set in the session; clear it to return to latest:

```sh
unset AUDITORYLAB_VERSION
```

```powershell
Remove-Item Env:AUDITORYLAB_VERSION -ErrorAction SilentlyContinue
```

For offline removal after quick install, save and review the installer from the
release tag matching the installed executable's version before going offline.
Use the download step above and substitute the installed version. Fetching the
current latest script later may give you a different ownership protocol.

The installer stages files privately on the destination filesystem and uses a
per-product, per-directory exclusive lock. A held lock fails promptly; there are
no automatic lock retries. It attempts to restore the prior owned state if
activation fails. Errors describe any remaining state; an interruption or failed
PATH write is not a promise that nothing was installed. Inspect the reported
paths before trying again.

On Windows an update can retain one uniquely named, receipt-owned running
executable. Close processes using it before another update or uninstall if
cleanup is refused. The installer never adopts or deletes arbitrary `.old` files.

Unix uninstall and rollback preserve private recovery images before rewriting startup files in place, retaining their symlink and hardlink identities. A failed write or handled interruption attempts to restore only attributable changes. If restoration is unsafe or fails, retain the reported staging directory and privately inspect its `rewrite.N` paths, identities, exact `preimage`/`postimage` and available `observed`/`restore-image` files against the current startup file. These files may contain private shell configuration. Retry refuses retained staging until recovery is reviewed; do not remove evidence or overwrite later user edits blindly.

## Ownership and unmanaged installations

The installer uses `.auditorylab-install` beside the executable
for ownership receipts and managed data. It verifies managed file hashes before
update or removal and refuses unowned files by default and changed managed files. A matching
filename, version output or PATH entry does not establish ownership.

For a receiptless executable, follow the [replace or keep decision and commands](#existing-executable-without-an-installer-receipt).

The option authorizes replacement of only the named ordinary executable. The installer never runs that unowned executable or infers ownership from a version/hash. It verifies the candidate first, locks and rechecks the inspected target immediately before mutation, then preserves the original bytes in a unique recovery backup in the install directory's parent. It reports the exact backup path. A successful install retains the backup; uninstall never deletes it. Existing PATH entries and directory ownership are not adopted. Windows refuses replacement at a filesystem-root install directory because an outside-directory recovery backup cannot be created there.

The flag is install-only. It cannot bypass changed owned payloads, corrupt/incomplete metadata, nonregular targets or access failures. Precise errors name the affected executable, receipt/state directory, lock or retained recovery stage. Preserve those paths and inspect the stated cause; do not edit receipts or hashes, copy ownership metadata to a new directory, or remove an entire shared bin directory.

A normal activation failure restores the prior executable and receipt, preserving prior absence when repairing a missing payload. PowerShell cancellation can bypass `catch`, so recovery is planned before the first PATH/file mutation and runs from the outer `finally`, validating actual backup slots and receipt hashes rather than trusting a flag after each move. Rollback and uninstall preserve the actual registry PATH value kind; removing the owned entry does not convert a later user-edited `String` container into absence. If rollback is incomplete, the installer retains recovery evidence, reports exact paths and refuses to proceed on retry until that evidence is inspected. Never assume restoration succeeded after an incomplete-rollback error. For restoring a retained original after successful replacement, follow the [recovery steps](#existing-executable-without-an-installer-receipt).

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
powershell -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1') -Uninstall
```

Pass the same custom install directory used for installation, for example
`sh ./install-auditorylab.sh --uninstall --install-dir '/absolute/my tools'` or
`powershell -ExecutionPolicy ByPass -File (Join-Path $HOME 'install-auditorylab.ps1') -Uninstall -InstallDir 'C:\My Tools'`.
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
| `--replace-unmanaged` | `-ReplaceUnmanaged` | | Install: back up and replace a receiptless ordinary executable when private installer state is absent. |
| `--help` | `-Help` | | Show usage. |

Pass options to the saved, reviewed script shown above. In PowerShell, set any
environment variables before launching that script.

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
- `LICENSE`, `PROJECT-NOTICE.md`, `LICENSE-COMMERCIAL.md`, `README.txt`,
  `THIRD-PARTY-NOTICES.txt`, `COPYRIGHT-library.html` and `SOURCE.txt` are
  retained under the private directory in `legal/`. The archived
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
Choose a fresh directory, or use explicit backed-up replacement for an ordinary
receiptless executable. Do not remove ownership or recovery metadata to bypass
a refusal; a version string is not ownership proof. If a lock remains after a crash,
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
