#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Validate release identity and extract the selected changelog body."""

import argparse
import datetime
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[1]


def validate_version(value):
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", value):
        raise ValueError("Release version must contain three canonical nonnegative integers.")
    return value


def validate_repository(value):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+", value):
        raise ValueError("Repository must be an OWNER/REPOSITORY name.")
    return value


def validate_source(value):
    if not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("Source must be a full lowercase Git commit SHA.")
    return value


def release_notes(text, version):
    """Return one dated release body, ignoring headings inside fenced examples."""
    validate_version(version)
    headings = []
    fence = None
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if marker:
            run, trailing = marker.groups()
            if fence is None:
                fence = (run[0], len(run))
            elif run[0] == fence[0] and len(run) >= fence[1] and not trailing.strip():
                fence = None
            continue
        if fence is None and line.startswith("## "):
            headings.append((index, line.strip()))
    if fence is not None:
        raise ValueError("Changelog has an unclosed fenced block.")
    unreleased = [i for i, heading in headings if heading == "## [Unreleased]"]
    if len(unreleased) != 1:
        raise ValueError("Changelog requires exactly one Unreleased heading.")
    prefix = f"## [{version}]"
    found = [(i, heading) for i, heading in headings if heading.startswith(prefix)]
    if len(found) != 1:
        raise ValueError("Changelog requires exactly one selected release heading.")
    start, heading = found[0]
    match = re.fullmatch(re.escape(prefix) + r" - (\d{4}-\d{2}-\d{2})", heading)
    if match is None or start <= unreleased[0]:
        raise ValueError("Selected release must be dated and follow Unreleased.")
    date = datetime.date.fromisoformat(match[1])
    if date > datetime.date.today():
        raise ValueError("Selected release date is in the future.")
    stop = next((i for i, _ in headings if i > start), len(lines))
    body_lines = lines[start + 1:stop]
    while body_lines and not body_lines[0].strip():
        body_lines.pop(0)
    while body_lines and not body_lines[-1].strip():
        body_lines.pop()
    if not body_lines:
        raise ValueError("Selected release body is empty.")
    return "".join(body_lines).rstrip("\r\n") + "\n"


def prepare(version, repository, root=ROOT):
    validate_version(version)
    validate_repository(repository)
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    if workspace["workspace"]["package"]["version"] != version:
        raise ValueError("Requested version differs from the Cargo workspace version.")
    channel = tomllib.loads((root / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]["channel"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", channel):
        raise ValueError("Release compiler must be pinned to an exact Rust version.")
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    validate_source(source)
    if os.environ.get("GITHUB_ACTIONS") == "true":
        is_main = os.environ.get("GITHUB_REF") == "refs/heads/main"
        if os.environ.get("GITHUB_SHA") != source:
            raise ValueError("Checkout differs from the dispatched source commit.")
    else:
        branch = subprocess.check_output(["git", "symbolic-ref", "--short", "HEAD"], cwd=root, text=True).strip()
        is_main = branch == "main"
    if not is_main:
        raise ValueError("Release qualification must run from main.")
    status = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=root, text=True)
    if status:
        raise ValueError("Release qualification requires a clean committed source tree.")
    notes = release_notes((root / "CHANGELOG.md").read_text(encoding="utf-8"), version)
    from release_archive import matrix_for
    return {"version": version, "source": source, "channel": channel,
            "matrix": {"include": matrix_for(version)}, "notes": notes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    command = sub.add_parser("prepare")
    command.add_argument("--version", required=True)
    command.add_argument("--repository", required=True)
    command.add_argument("--github-output", type=Path)
    command = sub.add_parser("notes")
    command.add_argument("--version", required=True)
    command.add_argument("--changelog", type=Path, default=ROOT / "CHANGELOG.md")
    args = parser.parse_args()
    try:
        if args.command == "notes":
            print(release_notes(args.changelog.read_text(encoding="utf-8"), args.version), end="")
            return
        result = prepare(args.version, args.repository)
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8") as output:
                for key in ("version", "source", "channel", "matrix"):
                    value = json.dumps(result[key], separators=(",", ":")) if key == "matrix" else result[key]
                    output.write(f"{key}={value}\n")
        print(json.dumps(result, indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release preparation failed: {error}\n")


if __name__ == "__main__":
    main()
