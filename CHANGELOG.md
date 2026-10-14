# Changelog

Notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Download instructions use commands that work directly from the extracted folder and provide complete short examples with explicit UTF-8 preset saving on Windows.

### Changed

- Binary packages retain libm's complete license and source notices in a compact text bundle instead of including its implementation and test source files. Other applicable license/runtime notices remain included.
- Windows instructions and verification use PowerShell 7 or later (`pwsh`), with version checks before qualification and extraction. The standalone executable has no PowerShell dependency.

### Internal

- Archive verification checks the complete dependency notice set and exact legal bytes against locked sources, and rejects extra root payloads, conflicting filesystem paths, inconsistent member types, unsafe permissions and files without a notice role, even when file inventories are regenerated. Linux archives use neutral ownership metadata.

## [0.1.0] - 2026-10-14

- First release.
