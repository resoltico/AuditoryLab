#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Release targets, release notes and the identity a release is built from."""

import argparse
import datetime
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

# Each target is built and tested natively on its runner. Archive names are
# derived from `platform`; Windows and macOS archives are ZIPs, Linux tar.gz.
# A macOS `minimum` is the deployment target the Rust target builds for; the
# archive README states it and packaging checks the executable declares it.
TARGETS = (
    {"target": "x86_64-pc-windows-gnullvm", "runner": "windows-2022", "platform": "windows-x86_64"},
    {"target": "aarch64-pc-windows-gnullvm", "runner": "windows-11-arm", "platform": "windows-aarch64"},
    {"target": "x86_64-unknown-linux-gnu", "runner": "ubuntu-22.04", "platform": "linux-x86_64"},
    {"target": "aarch64-unknown-linux-gnu", "runner": "ubuntu-22.04-arm", "platform": "linux-aarch64"},
    {"target": "x86_64-apple-darwin", "runner": "macos-15-intel", "platform": "macos-x86_64", "minimum": "10.12"},
    {"target": "aarch64-apple-darwin", "runner": "macos-15", "platform": "macos-aarch64", "minimum": "11.0"},
)


def archive_name(platform, version):
    suffix = "tar.gz" if platform.startswith("linux-") else "zip"
    return f"auditory-lab-{version}-{platform}.{suffix}"


def windows_toolchain(target, root=ROOT):
    """The pinned native GNU LLVM host toolchain for a supported Windows target."""
    if target not in {entry["target"] for entry in TARGETS if entry["platform"].startswith("windows-")}:
        raise ValueError(f"unsupported Windows target: {target}")
    match = re.search(r'^channel = "([^"\n]+)"$', (root / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)
    if match is None:
        raise ValueError("rust-toolchain.toml has no pinned channel")
    return f"{match[1]}-{target}"


def workspace_version(root=ROOT):
    """The version Cargo reports for the CLI package."""
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root, text=True, encoding="utf-8"))
    return next(package["version"] for package in metadata["packages"] if package["name"] == "auditory-lab")


def release_notes(changelog, version):
    """Return the body of the dated `## [version] - YYYY-MM-DD` section."""
    lines = changelog.splitlines(keepends=True)
    selected, sections, fence = None, [], None
    for index, line in enumerate(lines):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line.rstrip("\r\n"))
        if fence:
            if marker and marker[1][0] == fence[0] and len(marker[1]) >= len(fence) and not marker[2].strip():
                fence = None
            continue
        if marker and (marker[1][0] == "~" or "`" not in marker[2]):
            fence = marker[1]
            continue
        if line.startswith("## "):
            if selected is not None:
                sections.append((selected, index))
                selected = None
            heading = re.fullmatch(rf"## \[{re.escape(version)}\] - (\d{{4}}-\d{{2}}-\d{{2}})\s*", line)
            if heading:
                if datetime.date.fromisoformat(heading[1]) > datetime.date.today():
                    raise ValueError(f"The {version} section is dated in the future.")
                selected = index + 1
    if fence is not None:
        raise ValueError("CHANGELOG.md has an unclosed code fence.")
    if selected is not None:
        sections.append((selected, len(lines)))
    if len(sections) != 1:
        raise ValueError(f"CHANGELOG.md needs exactly one dated section for {version}.")
    start, end = sections[0]
    while start < end and not lines[start].strip():
        start += 1
    while end > start and not lines[end - 1].strip():
        end -= 1
    if start == end:
        raise ValueError(f"The {version} section is empty.")
    body = "".join(lines[start:end])
    return body if body.endswith("\n") else body + "\n"


def check(version, root=ROOT):
    """Check version, notes and a committed checkout; return its HEAD identity."""
    if workspace_version(root) != version:
        raise ValueError(f"Cargo.toml's workspace version is not {version}.")
    release_notes((root / "CHANGELOG.md").read_text(encoding="utf-8"), version)
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
    if git("status", "--porcelain", "--untracked-files=all"):
        raise ValueError("The checkout has uncommitted changes.")
    return git("rev-parse", "HEAD")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("matrix", help="print the build matrix as JSON")
    commands.add_parser("version", help="print the workspace version")
    commands.add_parser("toolchain", help="print the pinned Windows host toolchain").add_argument("--target", required=True)
    for name in ("notes", "check"):
        commands.add_parser(name).add_argument("--version", required=True)
    args = parser.parse_args()
    try:
        if args.command == "matrix":
            print(json.dumps({"include": list(TARGETS)}, separators=(",", ":")))
        elif args.command == "toolchain":
            print(windows_toolchain(args.target))
        elif args.command == "version":
            print(workspace_version())
        elif args.command == "notes":
            print(release_notes((ROOT / "CHANGELOG.md").read_text(encoding="utf-8"), args.version), end="")
        else:
            print(check(args.version))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"release: {error}")


if __name__ == "__main__":
    main()
