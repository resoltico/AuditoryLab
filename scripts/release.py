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
MACOS_MINIMUM = "15.0"

# Each target is built and tested natively on its runner. Archive names are
# derived from `platform`; Windows and macOS archives are ZIPs, Linux tar.gz.
# MACOS_MINIMUM is shared by every macOS product and architecture; native
# packaging verifies the declared deployment target and derives bundled text.
TARGETS = (
    {"target": "x86_64-pc-windows-gnullvm", "runner": "windows-2022", "platform": "windows-x86_64"},
    {"target": "aarch64-pc-windows-gnullvm", "runner": "windows-11-arm", "platform": "windows-aarch64"},
    {"target": "x86_64-unknown-linux-gnu", "runner": "ubuntu-22.04", "platform": "linux-x86_64"},
    {"target": "aarch64-unknown-linux-gnu", "runner": "ubuntu-22.04-arm", "platform": "linux-aarch64"},
    {"target": "x86_64-apple-darwin", "runner": "macos-15-intel", "platform": "macos-x86_64"},
    {"target": "aarch64-apple-darwin", "runner": "macos-15", "platform": "macos-aarch64"},
)


# Product identity is checked against Cargo, not inferred from executable filenames.
PRODUCTS = {
    "cli": {"package": "auditory-cli", "bin": "auditorylab"},
    "gui": {"package": "auditory-gui", "bin": "auditorylab-gui"},
}


def validate_build_request(build_kind, source_revision):
    if build_kind not in {"local", "candidate", "release"}:
        raise ValueError(f"unsupported build kind: {build_kind}")
    if build_kind == "local":
        if source_revision is not None:
            raise ValueError("local builds cannot claim a source revision")
    elif not isinstance(source_revision, str) or re.fullmatch(r"[0-9a-f]{40}", source_revision) is None:
        raise ValueError("candidate/release builds require the full lowercase 40-hex source revision")


def archive_name(platform, version, product="cli", build_kind="release", source_revision=None):
    suffix = "tar.gz" if platform.startswith("linux-") else "zip"
    binary = PRODUCTS[product]["bin"]
    marker = ""
    if build_kind != "release":
        validate_build_request(build_kind, source_revision)
        marker = "-dev-" + source_revision[:12] if build_kind == "candidate" else "-local"
    return f"{binary}-{version}{marker}-{platform}.{suffix}"


def artifact_name(product, platform, build_kind):
    return f"{build_kind}-{product}-{platform}"


def windows_toolchain(target, root=ROOT):
    """The pinned native GNU LLVM host toolchain for a supported Windows target."""
    if target not in {entry["target"] for entry in TARGETS if entry["platform"].startswith("windows-")}:
        raise ValueError(f"unsupported Windows target: {target}")
    match = re.search(r'^channel = "([^"\n]+)"$', (root / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)
    if match is None:
        raise ValueError("rust-toolchain.toml has no pinned channel")
    return f"{match[1]}-{target}"


def cargo_identity(metadata=None, root=ROOT, product="cli"):
    """Require checked distribution products, using exactly the supplied graph."""
    return cargo_identities(metadata, root)[product]


def cargo_identities(metadata=None, root=ROOT):
    if metadata is None:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=root, text=True, encoding="utf-8"))
    members = set(metadata["workspace_members"])
    result = {}
    for product, definition in PRODUCTS.items():
        roots = [package for package in metadata["packages"]
                 if package["id"] in members and package["name"] == definition["package"]]
        if len(roots) != 1:
            raise ValueError(f"Cargo workspace must contain exactly one {definition['package']} package; "
                             "check workspace membership and package names")
        package = roots[0]
        bins = [target for target in package["targets"] if "bin" in target["kind"]]
        if len(bins) != 1 or bins[0]["name"] != definition["bin"]:
            raise ValueError(f"{definition['package']} must declare exactly one bin named {definition['bin']}; "
                             "remove executable aliases and disable automatic bins")
        result[product] = {"id": package["id"], "name": package["name"],
                           "version": package["version"], "bin": bins[0]["name"]}
    known_ids = {item["id"] for item in result.values()}
    if any(package["id"] in members and package["id"] not in known_ids and
           any("bin" in target["kind"] for target in package["targets"])
           for package in metadata["packages"]):
        raise ValueError("extra workspace distributable binary: add a reviewed product definition")
    if len({item["version"] for item in result.values()}) != 1:
        raise ValueError("distribution products must share the workspace version")
    return result


def expected_identity(product, version, target, build_kind, source_revision):
    validate_build_request(build_kind, source_revision)
    definition = PRODUCTS[product]
    return {"format_version": 1, "product": product, "package": definition["package"],
            "bin": definition["bin"], "version": version, "build_kind": build_kind,
            "source_revision": source_revision, "target": target}


def check_build_identity(actual, product, version, target, build_kind, source_revision):
    expected = expected_identity(product, version, target, build_kind, source_revision)
    if not isinstance(actual, dict) or type(actual.get("format_version")) is not int or actual != expected:
        raise ValueError(f"executable build identity differs: expected {expected}, found {actual}")


def workspace_version(root=ROOT):
    """The version Cargo reports for the checked CLI package."""
    return cargo_identity(root=root)["version"]


def validate_artifacts(directory, version, build_kind="release", source_revision=None):
    """Validate product assignments and archive identity before flattening."""
    validate_build_request(build_kind, source_revision)
    directory = Path(directory)
    expected = {artifact_name(product, item["platform"], build_kind):
                (archive_name(item["platform"], version, product, build_kind, source_revision), product, item)
                for product in PRODUCTS for item in TARGETS}
    actual = {path.name for path in directory.iterdir()}
    if actual != set(expected):
        raise ValueError(f"release artifact directories differ: missing {sorted(set(expected) - actual)}, "
                         f"unexpected {sorted(actual - set(expected))}")
    archives = []
    from package import members, expected_members, validate_bundle
    for name, (filename, product, target) in expected.items():
        folder = directory / name
        if folder.is_symlink() or not folder.is_dir():
            raise ValueError(f"release artifact must be a real directory: {folder}")
        children = list(folder.iterdir())
        if (len(children) != 1 or children[0].name != filename
                or children[0].is_symlink() or not children[0].is_file()):
            raise ValueError(f"{folder} must contain exactly its assigned regular archive {filename}")
        archive = children[0]
        root = filename.removesuffix(".zip").removesuffix(".tar.gz")
        contents = members(archive)
        if set(contents) != {root + "/" + path for path in expected_members(product, target["target"])}:
            raise ValueError(f"{archive} has wrong product/archive members")
        if product == "gui" and "apple" in target["target"]:
            validate_bundle(contents[root + "/Auditory Lab.app/Contents/Info.plist"][1], version)
        readme = contents[root + "/README.txt"][1].decode("utf-8")
        identities = re.findall(r"^Build identity: (.+)$", readme, re.M)
        if len(identities) != 1:
            raise ValueError(f"{archive} needs exactly one build identity record")
        check_build_identity(json.loads(identities[0]), product, version, target["target"], build_kind, source_revision)
        archives.append(archive)
    return archives


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
    validator = commands.add_parser("validate-artifacts", help="validate downloads before flattening")
    validator.add_argument("--directory", type=Path, required=True)
    validator.add_argument("--version", required=True)
    validator.add_argument("--build-kind", choices=("candidate", "release", "local"), default="release")
    validator.add_argument("--source-revision")
    args = parser.parse_args()
    try:
        if args.command == "matrix":
            print(json.dumps({"include": list(TARGETS)}, separators=(",", ":")))
        elif args.command == "toolchain":
            print(windows_toolchain(args.target))
        elif args.command == "version":
            print(workspace_version())
        elif args.command == "validate-artifacts":
            validate_artifacts(args.directory, args.version, args.build_kind, args.source_revision)
        elif args.command == "notes":
            print(release_notes((ROOT / "CHANGELOG.md").read_text(encoding="utf-8"), args.version), end="")
        else:
            print(check(args.version))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"release: {error}")


if __name__ == "__main__":
    main()
