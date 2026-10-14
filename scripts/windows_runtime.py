#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Build and verify the reviewed self-contained Windows native runtime inputs.

The receipt binds a successful final application link trace to its output bytes.
It detects stale evidence; it is not an authenticity or legal-review proof.
Rust objects and rlibs remain covered by the Rust/crate notice policy. Other
native inputs must match the reviewed Rust distribution components and selected
members in licenses/windows-runtime.json.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

from release import ROOT

MANIFEST = ROOT / "scripts" / "licenses" / "windows-runtime.json"
PREFIX = re.compile(r"^\s*rust-lld(?:\.exe)?: (Reading|Loaded) (.+)$")
ARCHIVE = re.compile(r"(.+\.(?:a|rlib|lib))\(([^()]+)\)")


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def toolchain():
    identity = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True, encoding="utf-8")
    version = re.search(r"^release: (.+)$", identity, re.M)[1]
    host = re.search(r"^host: (.+)$", identity, re.M)[1]
    sysroot = Path(subprocess.check_output(
        ["rustc", "--print", "sysroot"], cwd=ROOT, text=True, encoding="utf-8").strip()).resolve()
    return version, host, sysroot


def policy(target):
    reviewed = json.loads(MANIFEST.read_text(encoding="utf-8"))
    pinned = re.search(r'^channel = "([^"]+)"',
                       (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)[1]
    if reviewed["producer"]["rust"] != pinned:
        raise ValueError("Windows native runtime material requires review for this Rust version")
    if target not in reviewed["targets"]:
        raise ValueError(f"no reviewed Windows native runtime inputs for {target}")
    return pinned, reviewed["targets"][target]["inputs"]


def check_trace(text, executable, target, sysroot, inputs):
    """Check actual rust-lld inputs, including archive member selection."""
    libraries = (sysroot / "lib" / "rustlib" / target / "lib").resolve()
    bundled = libraries / "self-contained"
    dependencies = executable.parent / "deps"
    archives, seen, selected = {}, set(), set()
    reading = 0
    for line in text.splitlines():
        match = PREFIX.fullmatch(line)
        if not match:
            continue
        action, item = match.groups()
        if action == "Loaded":
            loaded = re.fullmatch(r"(.+\.(?:a|rlib|lib)\([^()]+\)) for .+", item)
            if not loaded:
                raise ValueError(f"unrecognized loaded Windows input: {item}")
            item = loaded[1]
        member = ARCHIVE.fullmatch(item)
        if member:
            name, object_name = member.groups()
            # lld names selected members by archive basename, after reading the
            # archive's full path. Do not infer that path from the basename alone.
            archive = archives.get(Path(name).name)
            if archive is None or (name != Path(name).name and Path(name).resolve() != archive):
                raise ValueError(f"archive member has no qualified input: {item}")
            if archive.name in inputs:
                if object_name not in inputs[archive.name]["members"]:
                    raise ValueError(f"unreviewed Windows native member: {item}")
                selected.add(archive.name)
            elif not (object_name.endswith(".rcgu.o") or object_name.lower().endswith(".dll")):
                raise ValueError(f"unexpected native object in Rust archive: {item}")
            continue
        if action != "Reading":
            raise ValueError(f"unrecognized loaded Windows input: {item}")
        reading += 1
        path = Path(item).resolve()
        if path.name in inputs:
            expected = bundled / path.name
            if path != expected.resolve() or expected.is_symlink():
                raise ValueError(f"Windows native input is outside the bundled runtime: {item}")
            if sha256(expected) != inputs[path.name]["sha256"]:
                raise ValueError(f"Windows native input differs from its reviewed component: {item}")
            seen.add(path.name)
        elif path.suffix == ".rlib" and path.parent in (libraries, dependencies):
            pass
        elif (path.name.endswith(".rcgu.o") and path.parent == dependencies
              and path.name.startswith("auditory_lab-")):
            pass
        elif (path.name == "symbols.o" and path.parent.parent == dependencies
              and path.parent.name.startswith("rustc")):
            pass
        else:
            raise ValueError(f"unexpected Windows native link input: {item}")
        if path.suffix in (".a", ".rlib"):
            previous = archives.setdefault(path.name, path)
            if previous != path:
                raise ValueError(f"ambiguous Windows archive input: {item}")
    required = {"crt2.o", "libunwind.a", "libmingw32.a", "libmingwex.a", "libmsvcrt.a"}
    if not reading or not required <= seen or not required - {"crt2.o"} <= selected:
        raise ValueError("Windows link trace lacks the reviewed startup and static runtime inputs")


def verify(executable, target, trace, sysroot=None):
    """Verify a binary-bound build receipt; sysroot injection is for cross-probes/tests."""
    executable, trace = Path(executable).resolve(), Path(trace)
    pinned, inputs = policy(target)
    version, _, installed = toolchain()
    if version != pinned:
        raise ValueError(f"Windows runtime verification requires Rust {pinned}, found {version}")
    sysroot = Path(sysroot).resolve() if sysroot is not None else installed
    receipt = json.loads(trace.read_text(encoding="utf-8"))
    fields = ("target", "rust", "sysroot", "sha256", "map", "map_sha256",
              "executable", "linker_output")
    if not isinstance(receipt, dict) or any(not isinstance(receipt.get(field), str) for field in fields):
        raise ValueError("Windows link receipt is incomplete or has invalid field types")
    if (receipt["target"] != target or receipt["rust"] != pinned
            or Path(receipt["sysroot"]).resolve() != sysroot):
        raise ValueError("Windows link receipt target/toolchain does not match this build")
    if receipt["sha256"] != sha256(executable):
        raise ValueError("Windows link receipt does not match the executable bytes")
    if sha256(Path(receipt["map"])) != receipt["map_sha256"]:
        raise ValueError("Windows link map differs from the recorded build")
    map_lines = Path(receipt["map"]).read_text(encoding="utf-8").splitlines()
    if not map_lines or map_lines[0].split() != ["Address", "Size", "Align", "Out", "In", "Symbol"]:
        raise ValueError("Windows link map lacks the pinned lld map header")
    startup = (sysroot / "lib" / "rustlib" / target / "lib" / "self-contained" / "crt2.o").resolve()
    records = (re.fullmatch(r"\s*[0-9a-fA-F]+\s+[0-9a-fA-F]+\s+\d+\s+(.+):\(\.text\)\s*", line)
               for line in map_lines[1:])
    if not any(record and Path(record[1]).resolve() == startup for record in records):
        raise ValueError("Windows link map lacks a bundled startup text record")
    check_trace(receipt["linker_output"], Path(receipt["executable"]).resolve(),
                target, sysroot, inputs)


def build(target, trace, sysroot=None):
    """Force a final application relink and record its actual artifact and evidence."""
    pinned, _ = policy(target)
    version, host, installed = toolchain()
    if version != pinned:
        raise ValueError(f"Windows runtime build requires Rust {pinned}, found {version}")
    trace = Path(trace).absolute()
    if trace.is_symlink():
        raise ValueError("the Windows link receipt must not be a symlink")
    trace.resolve().relative_to((ROOT / "target").resolve())
    trace.parent.mkdir(parents=True, exist_ok=True)
    # Invalidate stale success before invoking a build that may fail.
    trace.unlink(missing_ok=True)
    evidence = Path(tempfile.mkdtemp(prefix=target + "-", dir=trace.parent))
    map_path = evidence / "application.map"
    linker = installed / "lib" / "rustlib" / host / "bin" / ("rust-lld.exe" if os.name == "nt" else "rust-lld")
    effective = Path(sysroot).resolve() if sysroot is not None else installed
    # The unique retained map path changes the final rustc invocation even when
    # Cargo would otherwise reuse its cached executable and provide no link trace.
    flags = ["-C", f"linker={linker}", "-C", "linker-flavor=ld.lld",
             "-C", "link-self-contained=yes", "-C", "target-feature=+crt-static",
             "-C", "link-arg=--verbose", "-C", f"link-arg=-Map={map_path}"]
    environment = os.environ.copy()
    if sysroot is not None:
        # Cross-probes need the same prepared std component for dependencies and
        # the final binary. Encoded flags preserve sysroot paths containing spaces.
        environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
            "--sysroot", str(effective), "-C", "target-feature=+crt-static"])
    command = ["cargo", "rustc", "--release", "--locked", "-p", "auditory-lab",
               "--bin", "auditory-lab", "--target", target,
               "--message-format=json", "--"] + flags
    result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True, encoding="utf-8")
    (evidence / "cargo.stdout").write_text(result.stdout, encoding="utf-8")
    (evidence / "cargo.stderr").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        # Keep full UTF-8 logs above; diagnostics must also work on a strict
        # legacy console without hiding the original failure or its evidence.
        diagnostic = (result.stdout + result.stderr).encode("ascii", "backslashreplace").decode("ascii")
        sys.stderr.write(diagnostic)
        raise ValueError(f"Windows build failed; evidence retained in {str(evidence)!a}")
    artifacts, diagnostics = [], [result.stderr]
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact" and message.get("executable")
                and message.get("target", {}).get("name") == "auditory-lab"):
            artifacts.append(Path(message["executable"]).resolve())
        if message.get("reason") == "compiler-message":
            diagnostics.append(message["message"].get("rendered") or "")
    if len(artifacts) != 1:
        raise ValueError(f"Windows build did not report one application executable; see {evidence}")
    executable = artifacts[0]
    receipt = {"target": target, "rust": pinned, "sysroot": str(effective),
               "executable": str(executable), "sha256": sha256(executable),
               "map": str(map_path), "map_sha256": sha256(map_path),
               "linker_output": "\n".join(diagnostics)}
    # Write a candidate next to the final receipt and verify it first; an invalid
    # trace or binary never leaves a success receipt for the packager to consume.
    candidate = evidence / "receipt.json"
    candidate.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    verify(executable, target, candidate, sysroot=effective)
    candidate.replace(trace)
    print(f"verified Windows native inputs: {str(executable)!a}; receipt: {str(trace)!a}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build", help="relink the release CLI and verify native inputs")
    builder.add_argument("--target", required=True)
    builder.add_argument("--trace", type=Path, required=True, help="receipt path under target/")
    args = parser.parse_args()
    try:
        build(args.target, args.trace)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f"windows runtime: {error}")


if __name__ == "__main__":
    main()
