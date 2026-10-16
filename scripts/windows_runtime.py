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

from release import ROOT, PRODUCTS, cargo_identity

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


def policy(target, product="cli"):
    reviewed = json.loads(MANIFEST.read_text(encoding="utf-8"))
    pinned = re.search(r'^channel = "([^"]+)"',
                       (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)[1]
    if reviewed["producer"]["rust"] != pinned:
        raise ValueError("Windows native runtime material requires review for this Rust version")
    if target not in reviewed["targets"]:
        raise ValueError(f"no reviewed Windows native runtime inputs for {target}")
    inputs = dict(reviewed["targets"][target]["inputs"])
    if product == "gui":
        inputs = {name: dict(entry, members=entry["members"] + entry.get("gui_members", []))
                  for name, entry in inputs.items()}
        imports = import_policy()
        imported = imports["targets"][target]
        inputs[Path(imported["relative_path"]).name] = imported
        for name in imports["library_views"]:
            inputs["lib" + name + ".a"] = dict(imported, library_view=name)
    return pinned, inputs


def import_policy():
    return json.loads((MANIFEST.parent / "windows-import-libraries.json").read_text(encoding="utf-8"))


def import_directory(target):
    if target not in import_policy()["targets"]:
        raise ValueError(f"no reviewed Windows GUI imports for {target}")
    directory = ROOT / "target/windows-imports" / target
    for path in (ROOT / "target", directory.parent, directory):
        if path.is_symlink():
            raise ValueError("Windows GUI import preparation cannot use a symlink")
    directory.resolve().relative_to(ROOT.resolve())
    return directory


def checked_import_views(target):
    """Accept only the fixed, byte-identical views of the reviewed producer."""
    imports = import_policy()
    entry = imports["targets"][target]
    original = native_input_path(entry)
    if original.is_symlink() or sha256(original) != entry["sha256"]:
        raise ValueError("Windows GUI import producer differs from reviewed bytes")
    directory = import_directory(target)
    names = {"lib" + name + ".a" for name in imports["library_views"]}
    if not directory.is_dir() or {path.name for path in directory.iterdir()} != names:
        raise ValueError("Windows GUI imports are missing or unexpected; inspect/remove only "
                         + str(directory) + ", then run python scripts/windows_runtime.py prepare --target " + target)
    for path in directory.iterdir():
        if path.is_symlink() or not path.is_file() or sha256(path) != entry["sha256"]:
            raise ValueError("Windows GUI import view differs from reviewed producer bytes")
    return {name: directory / name for name in names}


def prepare_imports(target):
    """Prepare the finite library-name views needed by ordinary GUI Cargo builds.

    MinGW's -l search requires each name to exist. These are full, unchanged
    import archives, not per-DLL subsets or Windows implementation libraries.
    Existing invalid material is rejected rather than silently repaired.
    """
    imports = import_policy()
    directory = import_directory(target)
    entry = imports["targets"][target]
    original = native_input_path(entry)
    if original.is_symlink() or sha256(original) != entry["sha256"]:
        raise ValueError("Windows GUI import producer differs from reviewed bytes")
    if not directory.exists():
        directory.parent.mkdir(parents=True, exist_ok=True)
        data = original.read_bytes()
        if hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise ValueError("Windows GUI import producer differs from reviewed bytes")
        with tempfile.TemporaryDirectory(prefix=target + "-", dir=directory.parent) as staging:
            staged = Path(staging) / "imports"
            staged.mkdir()
            for name in imports["library_views"]:
                with (staged / ("lib" + name + ".a")).open("xb") as output:
                    output.write(data)
            staged.rename(directory)
    return checked_import_views(target)


def native_input_path(entry):
    if "crate" not in entry:
        return None
    graph = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"],
                      cwd=ROOT, text=True, encoding="utf-8"))
    packages = [p for p in graph["packages"] if p["name"] == entry["crate"] and p["version"] == entry["version"] and p["source"] is not None]
    if len(packages) != 1:
        raise ValueError("reviewed Windows system import producer is missing or ambiguous")
    directory = Path(packages[0]["manifest_path"]).parent
    vcs = json.loads((directory / ".cargo_vcs_info.json").read_text(encoding="utf-8"))["git"]["sha1"]
    if vcs != entry["vcs"]:
        raise ValueError("Windows system import producer differs from reviewed published identity")
    return directory / entry["relative_path"]


def check_trace(text, executable, target, sysroot, inputs, product="cli"):
    """Check actual rust-lld inputs, including archive member selection."""
    libraries = (sysroot / "lib" / "rustlib" / target / "lib").resolve()
    bundled = libraries / "self-contained"
    dependencies = executable.parent / "deps"
    archives, seen, selected = {}, set(), set()
    reading = 0
    views = None
    # Other Unicode line separators are valid filename characters, not records.
    for line in text.replace("\r\n", "\n").split("\n"):
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
            if "library_view" in inputs[path.name]:
                if product != "gui":
                    raise ValueError("Windows GUI import views cannot be used by the CLI")
                if views is None:
                    views = checked_import_views(target)
                expected = views[path.name]
            else:
                expected = native_input_path(inputs[path.name]) or bundled / path.name
            if path != expected.resolve() or expected.is_symlink():
                raise ValueError(f"Windows native input is outside the bundled runtime: {item}")
            if sha256(expected) != inputs[path.name]["sha256"]:
                raise ValueError(f"Windows native input differs from its reviewed component: {item}")
            seen.add(path.name)
        elif path.suffix == ".rlib" and path.parent in (libraries, dependencies):
            pass
        elif (path.name.endswith(".rcgu.o") and path.parent == dependencies
              and path.name.startswith(PRODUCTS[product]["bin"].replace("-", "_") + "-")):
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


def verify(executable, target, trace, sysroot=None, product="cli"):
    """Verify a binary-bound build receipt; sysroot injection is for cross-probes/tests."""
    executable, trace = Path(executable).resolve(), Path(trace)
    pinned, inputs = policy(target, product)
    version, _, installed = toolchain()
    if version != pinned:
        raise ValueError(f"Windows runtime verification requires Rust {pinned}, found {version}")
    sysroot = Path(sysroot).resolve() if sysroot is not None else installed
    receipt = json.loads(trace.read_text(encoding="utf-8"))
    fields = ("product", "target", "rust", "sysroot", "sha256", "map", "map_sha256",
              "executable", "linker_output")
    if not isinstance(receipt, dict) or any(not isinstance(receipt.get(field), str) for field in fields):
        raise ValueError("Windows link receipt is incomplete or has invalid field types")
    if (receipt["product"] != product or receipt["target"] != target or receipt["rust"] != pinned
            or Path(receipt["sysroot"]).resolve() != sysroot):
        raise ValueError("Windows link receipt target/toolchain does not match this build")
    if receipt["sha256"] != sha256(executable):
        raise ValueError("Windows link receipt does not match the executable bytes")
    if sha256(Path(receipt["map"])) != receipt["map_sha256"]:
        raise ValueError("Windows link map differs from the recorded build")
    map_lines = Path(receipt["map"]).read_text(encoding="utf-8").split("\n")
    if not map_lines or map_lines[0].split() != ["Address", "Size", "Align", "Out", "In", "Symbol"]:
        raise ValueError("Windows link map lacks the pinned lld map header")
    startup = (sysroot / "lib" / "rustlib" / target / "lib" / "self-contained" / "crt2.o").resolve()
    records = (re.fullmatch(r"\s*[0-9a-fA-F]+\s+[0-9a-fA-F]+\s+\d+\s+(.+):\(\.text\)\s*", line)
               for line in map_lines[1:])
    if not any(record and Path(record[1]).resolve() == startup for record in records):
        raise ValueError("Windows link map lacks a bundled startup text record")
    check_trace(receipt["linker_output"], Path(receipt["executable"]).resolve(),
                target, sysroot, inputs, product)


def build(target, trace, sysroot=None, product="cli"):
    """Force a final application relink and record its actual artifact and evidence."""
    if product == "gui":
        prepare_imports(target)
    identity = cargo_identity(product=product)
    pinned, _ = policy(target, product)
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
    command = ["cargo", "rustc", "--release", "--locked", "-p", identity["name"],
               "--bin", identity["bin"], "--target", target,
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
    for line in result.stdout.split("\n"):
        if not line:
            continue
        message = json.loads(line)
        if (message.get("reason") == "compiler-artifact" and message.get("executable")
                and message.get("package_id") == identity["id"]
                and "bin" in message.get("target", {}).get("kind", [])
                and message.get("target", {}).get("name") == identity["bin"]):
            artifacts.append(Path(message["executable"]).resolve())
        if message.get("reason") == "compiler-message":
            # Display rendering can replace CR with a glyph and change path identity.
            diagnostic = message["message"]
            diagnostics.append(diagnostic["message"])
            diagnostics.extend(child["message"] for child in diagnostic.get("children", []))
    if len(artifacts) != 1:
        raise ValueError(f"Windows build did not report one application executable; see {evidence}")
    executable = artifacts[0]
    receipt = {"product": product, "target": target, "rust": pinned, "sysroot": str(effective),
               "executable": str(executable), "sha256": sha256(executable),
               "map": str(map_path), "map_sha256": sha256(map_path),
               "linker_output": "\n".join(diagnostics)}
    # Write a candidate next to the final receipt and verify it first; an invalid
    # trace or binary never leaves a success receipt for the packager to consume.
    candidate = evidence / "receipt.json"
    candidate.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    verify(executable, target, candidate, sysroot=effective, product=product)
    candidate.replace(trace)
    print(f"verified Windows native inputs: {str(executable)!a}; receipt: {str(trace)!a}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build", help="relink the release product and verify native inputs")
    builder.add_argument("--target", required=True)
    builder.add_argument("--trace", type=Path, required=True, help="receipt path under target/")
    builder.add_argument("--product", choices=PRODUCTS, default="cli")
    preparer = commands.add_parser("prepare", help="prepare reviewed import material for Windows GUI Cargo builds/tests")
    preparer.add_argument("--target", required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare_imports(args.target)
        else:
            build(args.target, args.trace, product=args.product)
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f"windows runtime: {error}")


if __name__ == "__main__":
    main()
