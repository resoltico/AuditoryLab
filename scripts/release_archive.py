#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Build and qualify one native CLI release archive."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shlex
import shutil
import stat
import struct
import subprocess
import tarfile
import tomllib
import zipfile

from release import validate_repository, validate_source, validate_version

ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    {"target": "x86_64-pc-windows-msvc", "runner": "windows-2022", "suffix": "windows-x86_64.zip", "artifact": "windows-x86_64", "windows": True, "macos": False},
    {"target": "aarch64-pc-windows-msvc", "runner": "windows-11-arm", "suffix": "windows-aarch64.zip", "artifact": "windows-aarch64", "windows": True, "macos": False},
    {"target": "x86_64-unknown-linux-gnu", "runner": "ubuntu-22.04", "suffix": "linux-x86_64.tar.gz", "artifact": "linux-x86_64", "windows": False, "macos": False},
    {"target": "aarch64-unknown-linux-gnu", "runner": "ubuntu-22.04-arm", "suffix": "linux-aarch64.tar.gz", "artifact": "linux-aarch64", "windows": False, "macos": False},
    {"target": "x86_64-apple-darwin", "runner": "macos-15-intel", "suffix": "macos-x86_64.zip", "artifact": "macos-x86_64", "windows": False, "macos": True},
    {"target": "aarch64-apple-darwin", "runner": "macos-15", "suffix": "macos-aarch64.zip", "artifact": "macos-aarch64", "windows": False, "macos": True},
)
MAX_MEMBER = 128 * 1024 * 1024
MAX_CONTENT = 256 * 1024 * 1024
LINUX_LIBRARIES = {"libasound.so.2", "libgcc_s.so.1", "libm.so.6", "libc.so.6", "libpthread.so.0", "libdl.so.2", "librt.so.1", "ld-linux-x86-64.so.2", "ld-linux-aarch64.so.1"}
WINDOWS_LIBRARIES = {"kernel32.dll", "ntdll.dll", "user32.dll", "advapi32.dll", "ole32.dll", "oleaut32.dll", "uuid.dll", "winmm.dll", "avrt.dll", "ws2_32.dll", "bcrypt.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "propsys.dll", "setupapi.dll", "powrprof.dll", "cfgmgr32.dll", "wintrust.dll", "crypt32.dll", "psapi.dll", "dbghelp.dll", "version.dll", "userenv.dll", "combase.dll", "mmdevapi.dll", "msvcrt.dll"}
MICROSOFT_TERMS = {
    17: {"family": "Visual Studio 2022", "distribution": "https://visualstudio.microsoft.com/license-terms/vs2022-ga-proenterprise/", "redistribution": "https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution", "runtime": "https://visualstudio.microsoft.com/license-terms/vs2022-cruntime/", "license_file": "microsoft-runtime-2022.txt", "text_sha256": "7e96c27c67ef3e7cabc55e1dee05727e67d261f3944155a816a07dbf9b78fb1d", "source_docx": "https://visualstudio.microsoft.com/wp-content/uploads/2021/09/Visual-C-Runtime-2015-2022-License-1.docx", "source_sha256": "f1e3d56ceb2ad68aae0711b910375009e651ac5530fa0760f0dea6e81e54fae1"},
    18: {"family": "Visual Studio 2026", "distribution": "https://visualstudio.microsoft.com/license-terms/vs2026-ga-pro-enterprise/", "redistribution": "https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution", "runtime": "https://visualstudio.microsoft.com/license-terms/vs2026-ga-visualcpp-v14-redist-runtime/", "license_file": "microsoft-runtime-2026.txt", "text_sha256": "3dfd057b91025067c08bd73c3cb0e6f712f76b6198b54413e14365d529f17ea6", "source_docx": "https://visualstudio.microsoft.com/wp-content/uploads/2025/10/Visual-C-V14-License-Redistributable_and_Runtime_ENU.docx", "source_sha256": "08651651a7602fc7c0e2763de0fde1ff9f868df2780597cd1775ee9d6441c783"},
}
MICROSOFT_PRODUCTS = {"Microsoft.VisualStudio.Product.Enterprise", "Microsoft.VisualStudio.Product.Professional"}


def matrix_for(version):
    validate_version(version)
    return [dict(entry, artifact="release-" + entry["artifact"], archive=f"auditory-lab-{version}-{entry['suffix']}", root=f"auditory-lab-{version}-{entry['artifact']}") for entry in TARGETS]


def target_entry(target, version):
    for entry in matrix_for(version):
        if entry["target"] == target:
            return entry
    raise ValueError(f"Unsupported native release target: {target}")


def digest(value):
    return hashlib.sha256(value).hexdigest()


def run(command, *, cwd=ROOT, timeout=120, input=None):
    result = subprocess.run([str(part) for part in command], cwd=cwd, input=input, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=timeout, check=False)
    if result.returncode:
        raise ValueError(f"Command failed ({result.returncode}): {command!r}\n{result.stdout[-20000:]}{result.stderr[-20000:]}")
    return result.stdout


def owned_path(path):
    path = Path(path).resolve()
    anchor = ROOT / "target"
    if anchor.is_symlink() or not path.is_relative_to(anchor.resolve()) or path == anchor.resolve():
        raise ValueError("Release output and scratch must be below this checkout's ordinary target/ directory.")
    return path


def pinned_channel():
    return tomllib.loads((ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]["channel"]


def compiler_info():
    lines = run(["rustc", "-vV"]).splitlines()
    info = dict(line.split(": ", 1) for line in lines[1:] if ": " in line)
    if info.get("release") != pinned_channel():
        raise ValueError("The active release compiler differs from rust-toolchain.toml.")
    return info


def check_build_flags(entry):
    target_key = entry["target"].upper().replace("-", "_")
    values = [os.environ.get(key, "") for key in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", f"CARGO_TARGET_{target_key}_RUSTFLAGS")]
    flags = [token for value in values for token in shlex.split(value.replace("\x1f", " "))]
    allowed = ["-C", "target-feature=+crt-static"] if entry["windows"] else []
    if flags != allowed:
        raise ValueError(f"Release flags must be exactly {allowed!r}; unexpected CPU/link/build flags: {flags!r}")
    if os.environ.get("CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS", "true").lower() not in ("true", "1"):
        raise ValueError("Release overflow checks must stay enabled.")
    if entry["macos"] and os.environ.get("MACOSX_DEPLOYMENT_TARGET") != "15.0":
        raise ValueError("Mac release builds require MACOSX_DEPLOYMENT_TARGET=15.0.")


def windows_runtime(target):
    required = ("VSINSTALLDIR", "VCToolsInstallDir", "VCToolsVersion", "WindowsSdkDir", "WindowsSDKVersion", "AUDITORY_LAB_VS_INFO")
    if any(not os.environ.get(key) for key in required):
        raise ValueError("Initialize the native vcvars environment and AUDITORY_LAB_VS_INFO before Windows qualification.")
    installation = json.loads(owned_path(os.environ["AUDITORY_LAB_VS_INFO"]).read_text(encoding="utf-8-sig"))
    if isinstance(installation, list) and len(installation) == 1:
        installation = installation[0]
    if not isinstance(installation, dict) or installation.get("isPrerelease") is not False:
        raise ValueError("The selected Visual Studio installation must explicitly be non-preview.")
    product = installation.get("productId")
    if product not in MICROSOFT_PRODUCTS:
        raise ValueError(f"Unreviewed Visual Studio producer license edition: {product!r}")
    version = installation.get("installationVersion", "")
    major = int(version.split(".")[0]) if re.fullmatch(r"\d+(?:\.\d+)+", version) else 0
    if major not in MICROSOFT_TERMS:
        raise ValueError(f"Unreviewed Visual Studio runtime family: {version!r}")
    vs = Path(os.environ["VSINSTALLDIR"]).resolve()
    if Path(installation.get("installationPath", "")).resolve() != vs:
        raise ValueError("Captured Visual Studio identity differs from the vcvars-selected installation.")
    tools = Path(os.environ["VCToolsInstallDir"]).resolve()
    tool_version = os.environ["VCToolsVersion"].strip("\\/")
    if not tools.is_relative_to(vs) or tools.name != tool_version or not re.fullmatch(r"\d+(?:\.\d+)+", tool_version):
        raise ValueError("Selected retail MSVC toolset directory/version is inconsistent.")
    arch = "arm64" if target.startswith("aarch64") else "x64"
    host = "HostARM64" if arch == "arm64" else "Hostx64"
    linker = tools / "bin" / host / arch / "link.exe"
    key = "CARGO_TARGET_" + target.upper().replace("-", "_") + "_LINKER"
    if not linker.is_file() or Path(os.environ.get(key, "")).resolve() != linker.resolve():
        raise ValueError(f"Set {key} to the selected native linker: {linker}")
    sdk = Path(os.environ["WindowsSdkDir"]).resolve()
    sdk_version = os.environ["WindowsSDKVersion"].strip("\\/")
    if not re.fullmatch(r"\d+(?:\.\d+)+", sdk_version):
        raise ValueError("Windows SDK version must identify a retail numeric version.")
    libraries = [tools / "lib" / arch / "libcmt.lib", tools / "lib" / arch / "libvcruntime.lib", sdk / "Lib" / sdk_version / "ucrt" / arch / "libucrt.lib"]
    if any(not path.is_file() for path in libraries):
        raise ValueError("The selected native retail CRT libraries are missing.")
    search = [Path(part).resolve() for part in os.environ.get("LIB", "").split(os.pathsep) if part]
    for library in libraries:
        selected = next((directory / library.name for directory in search if (directory / library.name).is_file()), None)
        if selected is None or selected.resolve() != library.resolve():
            raise ValueError("The linker's LIB search selects a different or missing retail CRT library.")
    return {"visual_studio_version": version, "product_id": product, "toolset_version": tool_version, "windows_sdk_version": sdk_version, "terms": MICROSOFT_TERMS[major], "libraries": [{"name": path.name, "sha256": digest(path.read_bytes())} for path in libraries]}


def assert_host(target):
    entry = target_entry(target, "0.1.0")
    info = compiler_info()
    machine = platform.machine().lower()
    arm = target.startswith("aarch64")
    machines = {"arm64", "aarch64"} if arm else {"x86_64", "amd64"}
    system = "Windows" if entry["windows"] else "Darwin" if entry["macos"] else "Linux"
    if platform.system() != system or machine not in machines or info.get("host") != target:
        raise ValueError(f"Native host mismatch: system={platform.system()}, machine={machine}, Rust host={info.get('host')}, target={target}")
    if entry["macos"]:
        translated = subprocess.run(["sysctl", "-in", "sysctl.proc_translated"], capture_output=True, text=True, check=False)
        if translated.stdout.strip() == "1":
            raise ValueError("A translated Rosetta process is not a native release host.")
    if entry["windows"] and os.environ.get("PROCESSOR_ARCHITEW6432"):
        raise ValueError("An emulated Windows process is not a native release host.")
    check_build_flags(entry)
    result = {"target": target, "machine": machine, "compiler": info}
    if entry["windows"]:
        result["microsoft_runtime"] = windows_runtime(target)
    return result


def unpack_from(data, fmt, offset):
    size = struct.calcsize(fmt)
    if offset < 0 or offset + size > len(data):
        raise ValueError("Truncated executable header.")
    return struct.unpack_from(fmt, data, offset)


def executable_header(data, target):
    entry = target_entry(target, "0.1.0")
    arm = target.startswith("aarch64")
    if entry["macos"]:
        if data[:4] != b"\xcf\xfa\xed\xfe":
            raise ValueError("Mac executable must be thin 64-bit little-endian Mach-O, never universal/fat.")
        cpu, _, kind, count, size = unpack_from(data, "<IIIII", 4)
        if cpu != (0x100000C if arm else 0x1000007) or kind != 2 or size > len(data) - 32:
            raise ValueError("Mach-O architecture/type/load-command size differs from the requested target.")
        offset = 32
        minos = None
        signature = False
        for _ in range(count):
            command, length = unpack_from(data, "<II", offset)
            if length < 8 or offset + length > 32 + size:
                raise ValueError("Invalid Mach-O load command.")
            if command == 0x32:
                plat, minimum = unpack_from(data, "<II", offset + 8)
                if plat != 1 or length < 24:
                    raise ValueError("Mach-O is not a macOS build.")
                minos = minimum
            elif command == 0x24:
                minos = unpack_from(data, "<I", offset + 8)[0]
            elif command == 0x1D:
                start, length_data = unpack_from(data, "<II", offset + 8)
                signature = length_data > 0 and start + length_data <= len(data)
            offset += length
        if offset != 32 + size or minos != 0xF0000 or not signature:
            raise ValueError("Mac executable must declare macOS 15.0 and contain its final signature.")
        return {"format": "Mach-O", "architecture": "aarch64" if arm else "x86_64", "minimum_macos": "15.0"}
    if entry["windows"]:
        if data[:2] != b"MZ":
            raise ValueError("Windows executable is not PE/COFF.")
        offset = unpack_from(data, "<I", 0x3C)[0]
        if data[offset:offset + 4] != b"PE\0\0":
            raise ValueError("Missing PE signature.")
        machine, sections, _, _, _, opt_size, flags = unpack_from(data, "<HHIIIHH", offset + 4)
        optional = offset + 24
        if machine != (0xAA64 if arm else 0x8664) or unpack_from(data, "<H", optional)[0] != 0x20B or not flags & 2 or flags & 0x2000 or opt_size < 128:
            raise ValueError("PE executable must be 64-bit and match the requested native architecture.")
        section_offset = optional + opt_size
        table = []
        for index in range(sections):
            virtual_size, address, raw_size, raw = unpack_from(data, "<IIII", section_offset + index * 40 + 8)
            if raw + raw_size > len(data):
                raise ValueError("PE section extends beyond the executable.")
            table.append((address, max(virtual_size, raw_size), raw, raw_size))
        def rva_offset(rva):
            for address, extent, raw, raw_size in table:
                if address <= rva < address + extent and rva - address < raw_size:
                    return raw + rva - address
            raise ValueError("PE import points outside backed executable sections.")
        import_rva, import_size = unpack_from(data, "<II", optional + 120)
        if import_size < 20 or not import_rva:
            raise ValueError("PE executable has no valid import directory.")
        imports = []
        base = rva_offset(import_rva)
        for index in range(min(import_size // 20, 4096)):
            descriptor = unpack_from(data, "<IIIII", base + index * 20)
            if not any(descriptor):
                break
            name_offset = rva_offset(descriptor[3])
            stop = data.find(b"\0", name_offset, name_offset + 256)
            if stop < 0:
                raise ValueError("Invalid PE DLL name.")
            imports.append(data[name_offset:stop].decode("ascii").lower())
        else:
            raise ValueError("PE import directory is unterminated.")
        if not imports:
            raise ValueError("PE imports unexpectedly empty.")
        check_windows_imports(imports)
        return {"format": "PE", "architecture": "aarch64" if arm else "x86_64", "imports": sorted(set(imports))}
    if data[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError("Linux executable must be ELF64 little-endian.")
    kind, machine = unpack_from(data, "<HH", 16)
    if kind not in (2, 3) or machine != (183 if arm else 62):
        raise ValueError("ELF executable architecture/type differs from the requested target.")
    offset = unpack_from(data, "<Q", 32)[0]
    entry_size, count = unpack_from(data, "<HH", 54)
    if entry_size != 56:
        raise ValueError("Invalid ELF64 program header size.")
    interpreter = None
    for index in range(count):
        kind = unpack_from(data, "<I", offset + index * entry_size)[0]
        if kind == 3:
            start = unpack_from(data, "<Q", offset + index * entry_size + 8)[0]
            length = unpack_from(data, "<Q", offset + index * entry_size + 32)[0]
            if start + length > len(data) or not length or data[start + length - 1] != 0:
                raise ValueError("Truncated ELF interpreter.")
            interpreter = data[start:start + length - 1].decode("ascii")
    wanted = "/lib/ld-linux-aarch64.so.1" if arm else "/lib64/ld-linux-x86-64.so.2"
    if interpreter != wanted:
        raise ValueError(f"ELF interpreter must be {wanted!r}, found {interpreter!r}.")
    return {"format": "ELF", "architecture": "aarch64" if arm else "x86_64", "interpreter": interpreter}


def check_windows_imports(imports):
    for library in imports:
        name = library.lower()
        if re.match(r"(?:vcruntime|msvcp|ucrtbase|concrt|vcomp)\d*", name) or name.startswith("api-ms-win-crt-"):
            raise ValueError(f"Static CRT binary unexpectedly imports a separately installed runtime: {name}")
        if name not in WINDOWS_LIBRARIES and not re.fullmatch(r"(?:api|ext)-ms-win-[a-z0-9-]+\.dll", name):
            raise ValueError(f"Unreviewed Windows dependency: {name}")


def check_macos_libraries(libraries):
    if not libraries or any(not (name.startswith("/usr/lib/") or name.startswith("/System/Library/Frameworks/")) for name in libraries):
        raise ValueError(f"Mac binary must use only Apple system dylibs/frameworks: {libraries!r}")


def check_linux_dependencies(libraries, versions):
    if not libraries or "libasound.so.2" not in libraries or any(name not in LINUX_LIBRARIES for name in libraries):
        raise ValueError(f"Unreviewed Linux runtime dependencies: {libraries!r}")
    if not versions or any(tuple(map(int, version.split("."))) > (2, 35) for version in versions):
        raise ValueError(f"Linux GLIBC requirements must not exceed 2.35: {versions!r}")


def native_binary_checks(binary, target):
    entry = target_entry(target, "0.1.0")
    header = executable_header(binary.read_bytes(), target)
    if entry["macos"]:
        run(["codesign", "--verify", "--strict", "--verbose=2", binary])
        signature = subprocess.run(["codesign", "-d", "--verbose=4", str(binary)], capture_output=True, text=True, check=False)
        if signature.returncode or "Signature=adhoc" not in signature.stderr:
            raise ValueError("Mac release executable must have its explicit ad hoc signature.")
        libraries = [line.strip().split(" (", 1)[0] for line in run(["otool", "-L", binary]).splitlines()[1:] if line.strip()]
        check_macos_libraries(libraries)
        header["libraries"] = libraries
    elif not entry["windows"]:
        dynamic = run(["readelf", "-W", "-d", binary])
        libraries = re.findall(r"\(NEEDED\).*?\[([^\]]+)\]", dynamic)
        versions = sorted(set(re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", run(["readelf", "-W", "--version-info", binary]))))
        check_linux_dependencies(libraries, versions)
        if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
            raise ValueError("Linux release executable must not depend on a runner-specific library search path.")
        header.update(libraries=libraries, glibc_versions=versions)
    return header


def dependency_closure(metadata):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    packages = {package["id"]: package for package in metadata["packages"]}
    roots = [package["id"] for package in metadata["packages"] if package["name"] == "auditory-lab" and package["id"] in metadata["workspace_members"]]
    if len(roots) != 1:
        raise ValueError("Metadata must contain one CLI workspace root.")
    seen = set()
    pending = roots.copy()
    while pending:
        package = pending.pop()
        if package in seen:
            continue
        seen.add(package)
        pending.extend(dep["pkg"] for dep in nodes[package]["deps"] if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"]))
    result = [packages[package] for package in seen if packages[package]["source"] is not None]
    if not result:
        raise ValueError("The CLI third-party dependency closure must not be empty.")
    return sorted(result, key=lambda package: (package["name"], package["version"]))


def license_files(directory, declared=None):
    """Published license filenames are not consistently uppercase."""
    files = {path for path in directory.iterdir() if path.is_file() and path.name.upper().startswith(("LICENSE", "LICENCE", "NOTICE", "COPYRIGHT"))}
    if declared:
        files.add(directory / declared)
    return files


def upstream_supplements():
    return json.loads((ROOT / "scripts" / "licenses" / "upstream-notices.json").read_text(encoding="utf-8"))


def supplemental_materials(package, directory):
    """Resolve only reviewed omissions, pinned to the published crate's VCS."""
    key = package["name"] + "-" + package["version"]
    expected = upstream_supplements().get(key)
    if expected is None:
        raise ValueError(f"Missing declared license material for {key}; no reviewed upstream supplement exists.")
    vcs_file = directory / ".cargo_vcs_info.json"
    if package.get("repository") != expected["repository"] or package.get("license") != expected["license"] or not vcs_file.is_file() or json.loads(vcs_file.read_text(encoding="utf-8")) != expected["vcs"]:
        raise ValueError(f"Upstream license supplement does not match the published repository/license/VCS identity: {key}")
    materials = []
    for item in expected["files"]:
        if Path(item["source_file"]).name != item["source_file"] or Path(item["archive_file"]).name != item["archive_file"]:
            raise ValueError("Unsafe upstream license material filename.")
        path = ROOT / "scripts" / "licenses" / item["source_file"]
        if path.is_symlink() or digest(path.read_bytes()) != item["sha256"]:
            raise ValueError(f"Reviewed upstream license source material is missing or altered: {key}")
        materials.append((path, Path(item["archive_file"])))
    if not materials:
        raise ValueError("Upstream license supplement cannot be empty.")
    return materials, expected


def copy_notices(destination, target, compiler, microsoft=None):
    metadata = json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target]))
    packages = dependency_closure(metadata)
    notice_root = destination / "third-party"
    notice_root.mkdir()
    inventory = {"format": 1, "target": target, "packages": [], "rust": {"version": compiler["release"], "file": "third-party/RUST-COPYRIGHT-library.html"}}
    for package in packages:
        if not re.fullmatch(r"[A-Za-z0-9_-]+", package["name"]) or not re.fullmatch(r"[A-Za-z0-9.+_-]+", package["version"]):
            raise ValueError("Unsafe dependency notice directory name.")
        directory = Path(package["manifest_path"]).parent
        source_files = license_files(directory, package.get("license_file"))
        if not package.get("license") and not package.get("license_file"):
            raise ValueError(f"Missing declared license material for {package['name']} {package['version']}.")
        supplement = None
        materials = []
        for path in sorted(source_files):
            if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(directory.resolve()):
                raise ValueError("Dependency license file escapes its package or is missing.")
            materials.append((path, path.relative_to(directory)))
        if not materials:
            materials, supplement = supplemental_materials(package, directory)
        folder = notice_root / f"{package['name']}-{package['version']}"
        folder.mkdir()
        copied = []
        for path, relative in materials:
            output = folder / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, output)
            copied.append(output.relative_to(destination).as_posix())
        if package["name"] == "libm":
            for path in sorted((directory / "src").rglob("*")):
                if path.is_symlink():
                    raise ValueError("libm source supplement contains a link.")
                if path.is_file():
                    output = folder / path.relative_to(directory)
                    output.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(path, output)
                    copied.append(output.relative_to(destination).as_posix())
            if not any(name.endswith("src/math/sin.rs") for name in copied):
                raise ValueError("libm source permission-header supplement is missing.")
        if package["name"] == "unicode-ident" and not any(name.endswith("LICENSE-UNICODE") for name in copied):
            raise ValueError("unicode-ident's required Unicode license is missing.")
        record = {"name": package["name"], "version": package["version"], "license": package.get("license"), "license_file": package.get("license_file"), "repository": package.get("repository"), "files": copied}
        if supplement is not None:
            record["upstream_supplement"] = supplement
        inventory["packages"].append(record)
    sysroot = Path(run(["rustc", "--print", "sysroot"]).strip())
    copyright_file = sysroot / "share" / "doc" / "rust" / "COPYRIGHT-library.html"
    if not copyright_file.is_file():
        raise ValueError("Pinned Rust toolchain's COPYRIGHT-library.html is missing; do not omit runtime notices.")
    shutil.copyfile(copyright_file, notice_root / "RUST-COPYRIGHT-library.html")
    if microsoft is not None:
        terms = microsoft["terms"]
        license_text = ROOT / "scripts" / "licenses" / terms["license_file"]
        if digest(license_text.read_bytes()) != terms["text_sha256"]:
            raise ValueError("The reviewed Microsoft runtime terms text differs from its source-material identity.")
        shutil.copyfile(license_text, notice_root / terms["license_file"])
        (notice_root / "MICROSOFT-RUNTIME.txt").write_text(f"Microsoft retail C runtime, statically linked.\nCopyright Microsoft Corporation. All rights reserved.\nSelected {terms['family']} {microsoft['visual_studio_version']} ({microsoft['product_id']}); MSVC {microsoft['toolset_version']}; Windows SDK {microsoft['windows_sdk_version']}.\nRuntime portions have separate Microsoft Software License Terms, not the project's MPL source license. Read the complete paragraph text in {terms['license_file']}; use of those portions requires accepting those terms.\nOfficial runtime terms: {terms['runtime']}\nOriginal formatted terms: {terms['source_docx']}\nOriginal DOCX SHA256: {terms['source_sha256']}\nParagraph text SHA256: {terms['text_sha256']}\nThe producer's distributable-code terms and reference:\n{terms['distribution']}\n{terms['redistribution']}\nRetail runtime object code is embedded in the executable; Windows OS DLLs are external. These references do not license redistribution of the Visual Studio tools themselves. Redistributors must preserve the Microsoft runtime terms and require acceptance for those portions.\n", encoding="utf-8")
        inventory["microsoft_runtime"] = microsoft
    (notice_root / "index.json").write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return inventory


def install_text(entry, version, repository, microsoft=None):
    filename = entry["archive"]
    source = f"https://github.com/{repository}/tree/v{version}"
    common = f"Auditory Lab {version}\n\nDownload this archive only from https://github.com/{repository}/releases/tag/v{version}.\nArchive: {filename}\nTarget: {entry['target']}\nCompare its SHA-256 with the exact filename in that release's SHA256SUMS before extraction. SHA256SUMS identifies the release's bytes; an ad hoc Mac signature does not identify the publisher.\n\n"
    if entry["macos"]:
        platform_text = f"Requires macOS 15.0 or later. Native qualification runs on macOS 15; the minimum version is not an executed CI test.\nApple menu > About This Mac: a Chip entry identifying Apple silicon needs macos-aarch64; an Intel Processor entry needs macos-x86_64. These are separate native downloads, not universal binaries.\n\nIn Terminal in the download directory:\n  shasum -a 256 {filename}\nAfter comparing the digest, extract the ZIP with Finder, then open Terminal in {entry['root']} and run:\n  ./auditory-lab --version\n\nThe executable is ad hoc signed and is not Developer ID signed or notarized. If macOS blocks the launch, first verify the download, then run in the extracted directory:\n  codesign --verify --strict --verbose=2 ./auditory-lab\nA successful signature check confirms integrity, not Developer ID or Gatekeeper approval. Then use the app-specific System Settings > Privacy & Security > Open Anyway option if macOS offers it after that blocked attempt. Managed policies may prohibit exceptions. An invalid or modified signature requires a fresh verified download, not an exception. Do not disable Gatekeeper/SIP globally, routinely remove quarantine, or resign this executable.\nOptional user installation: create ~/.local/bin, copy auditory-lab there, and put that directory on your shell PATH. No app bundle, Python or Rust runtime is needed.\n"
    elif entry["windows"]:
        minimum = "Windows 11 ARM64" if entry["target"].startswith("aarch64") else "Windows 10 x64 or Windows Server 2016 x64"
        terms = microsoft["terms"]["runtime"] if microsoft else "https://visualstudio.microsoft.com/license-terms/"
        platform_text = f"Requires {minimum} or later. Native CI runs on {entry['runner']}; it does not execute every stated minimum version. Choose the architecture matching your Windows system type in Settings > System > About.\n\nIn PowerShell in the download directory:\n  Get-FileHash -Algorithm SHA256 -LiteralPath .\\{filename}\nAfter comparing the digest, use Extract All, open PowerShell in {entry['root']} and run:\n  .\\auditory-lab.exe --version\n\nNo Rust/Python or separately installed Visual C++ runtime is required. Retail Microsoft CRT portions are statically linked and are subject to separate Microsoft terms; before using the executable, read and accept the complete runtime terms in third-party/{microsoft['terms']['license_file'] if microsoft else 'MICROSOFT-RUNTIME.txt'}. Use of Microsoft runtime portions accepts those terms; the official reference is:\n{terms}\nSee third-party/MICROSOFT-RUNTIME.txt for the selected runtime and producer's redistribution references. These terms apply to Microsoft runtime portions, not Auditory Lab's MPL-covered source. Keep all notices when redistributing this archive.\nWindows may show publisher/reputation warnings because this release has no Microsoft Authenticode publisher certificate. Verify the official archive digest before choosing any app-specific exception; managed policy can prevent execution.\n"
    else:
        platform_text = f"Built and natively tested on Ubuntu 22.04 for {entry['target'].split('-')[0]}, with glibc 2.35 and the system ALSA runtime (libasound.so.2). Other distributions are not individually qualified. Requires compatible 64-bit glibc/ALSA libraries and a matching CPU. On Ubuntu, install the runtime with: sudo apt-get install libasound2\n\nIn a terminal in the download directory:\n  sha256sum {filename}\nAfter comparing the digest:\n  tar -xzf {filename}\n  cd {entry['root']}\n  ./auditory-lab --version\n\nNo Rust/Python runtime is needed. Optional user installation: copy auditory-lab to ~/.local/bin and put that directory on your shell PATH.\n"
    return common + platform_text + f"\nRead USAGE.txt for list/show/inspect and a short, silent export. Playback is opt-in: lower the output volume first; digital levels do not limit sound pressure. Headphones preserve intended ear routing.\n\nProject source and matching license: {source}\nMPL-2.0-covered source is available at that exact tag. LICENSE applies to project source; third-party notices retain their respective terms. BUILD_INFO.json records build identity.\n"


def usage_text(binary):
    text = "Auditory Lab command reference\n\n"
    for args in (["--help"], ["list", "--help"], ["show", "--help"], ["inspect", "--help"], ["export", "--help"], ["play", "--help"]):
        text += run([binary, *args], cwd=binary.parent, timeout=30).rstrip() + "\n\n"
    prefix = ".\\auditory-lab.exe" if binary.suffix == ".exe" else "./auditory-lab"
    return text + f"Short silent export\nSave the following as silence.toml (UTF-8):\n\n  schema_version = 1\n  duration_seconds = 0.01\n  fade_seconds = 0.0\n\nThen run:\n  {prefix} list\n  {prefix} inspect --sample-rate 48000 ./silence.toml\n  {prefix} export --sample-rate 48000 --output silence.wav ./silence.toml\nExisting output files are never overwritten. This example contains no components and makes a small silent stereo float WAV.\n\nThe built-in catalog is embedded in the executable. Use show NAME to save and edit a recipe; inspect it before export or optional playback. Scientific metadata documents adaptations, not clinical efficacy. A WAV export does not play sound. Playback opens the default output device; set volume low before opting in.\n"


def member_name(name, root):
    if not name or any(ord(char) < 32 for char in name) or "\\" in name or ":" in name or name.startswith("/") or any(part in ("", ".", "..") for part in name.rstrip("/").split("/")):
        raise ValueError(f"Unsafe archive member path: {name!r}")
    path = PurePosixPath(name.rstrip("/"))
    if path.parts[0] != root:
        raise ValueError("Archive has an unexpected root directory.")
    return path.as_posix()


def archive_members(path, root):
    files = {}
    modes = {}
    seen = set()
    total = 0
    if str(path).endswith(".zip"):
        with zipfile.ZipFile(path) as archive:
            for member in archive.infolist():
                name = member_name(member.filename, root)
                mode = member.external_attr >> 16
                if name in seen or member.flag_bits & 1 or stat.S_ISLNK(mode) or (stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR)):
                    raise ValueError("Archive has duplicate, encrypted, linked or special members.")
                seen.add(name)
                if member.is_dir():
                    continue
                total += member.file_size
                if member.file_size > MAX_MEMBER or total > MAX_CONTENT:
                    raise ValueError("Archive content exceeds the release verification size limit.")
                files[name] = archive.read(member)
                modes[name] = stat.S_IMODE(mode)
    else:
        with tarfile.open(path, "r:gz") as archive:
            for member in archive:
                name = member_name(member.name, root)
                if name in seen or not (member.isdir() or member.isfile()):
                    raise ValueError("Archive has duplicate, linked or special members.")
                seen.add(name)
                if member.isdir():
                    continue
                total += member.size
                if member.size > MAX_MEMBER or total > MAX_CONTENT:
                    raise ValueError("Archive content exceeds the release verification size limit.")
                files[name] = archive.extractfile(member).read()
                modes[name] = member.mode
    if any(not any(filename.startswith(name + "/") for filename in files) for name in seen - files.keys()):
        raise ValueError("Archive contains an unexpected empty directory.")
    if len({name.casefold() for name in seen}) != len(seen):
        raise ValueError("Archive members collide on a case-insensitive filesystem.")
    return files, modes


def verify_archive(path, target, version, source, repository):
    """Verify archive bytes/identity without extracting or executing anything."""
    validate_source(source)
    validate_repository(repository)
    entry = target_entry(target, version)
    path = Path(path)
    if path.name != entry["archive"]:
        raise ValueError("Archive filename differs from the exact matrix asset.")
    files, modes = archive_members(path, entry["root"])
    prefix = entry["root"] + "/"
    relative = {name[len(prefix):]: data for name, data in files.items() if name.startswith(prefix)}
    required = {"auditory-lab.exe" if entry["windows"] else "auditory-lab", "INSTALL.txt", "USAGE.txt", "LICENSE", "BUILD_INFO.json", "third-party/index.json", "third-party/RUST-COPYRIGHT-library.html"}
    if not required <= relative.keys() or any(name not in required and not name.startswith("third-party/") for name in relative):
        raise ValueError("Archive is missing required content or has unexpected top-level files.")
    info = json.loads(relative["BUILD_INFO.json"])
    for key, value in {"format": 1, "target": target, "version": version, "source": source, "repository": repository}.items():
        if info.get(key) != value:
            raise ValueError(f"Archive build identity mismatch: {key}")
    if info.get("files") != {name: digest(data) for name, data in relative.items() if name != "BUILD_INFO.json"}:
        raise ValueError("Archive content hashes/inventory differ from BUILD_INFO.json.")
    if info.get("compiler", {}).get("release") != pinned_channel() or info.get("compiler", {}).get("host") != target:
        raise ValueError("Archive compiler/host differs from the pinned native target.")
    settings = info.get("settings", {})
    if settings.get("overflow_checks") is not True or settings.get("cpu") != "portable-default" or settings.get("crt") != ("static-retail" if entry["windows"] else None) or settings.get("macos_deployment_target") != ("15.0" if entry["macos"] else None):
        raise ValueError("Archive build settings do not match the release contract.")
    exe = "auditory-lab.exe" if entry["windows"] else "auditory-lab"
    header = executable_header(relative[exe], target)
    if any(info.get("platform", {}).get(key) != value for key, value in header.items()):
        raise ValueError("Executable header differs from its build metadata.")
    if not entry["windows"] and modes[prefix + exe] != 0o755:
        raise ValueError("Unix executable archive mode must be exactly 0755.")
    if entry["macos"]:
        check_macos_libraries(info.get("platform", {}).get("libraries", []))
    elif not entry["windows"]:
        check_linux_dependencies(info.get("platform", {}).get("libraries", []), info.get("platform", {}).get("glibc_versions", []))
    inventory = json.loads(relative["third-party/index.json"])
    if inventory.get("format") != 1 or inventory.get("target") != target or not inventory.get("packages"):
        raise ValueError("Third-party inventory must describe this target's nonempty closure.")
    listed = {"third-party/index.json", inventory.get("rust", {}).get("file")}
    pairs = set()
    supplements = upstream_supplements()
    for package in inventory["packages"]:
        pair = (package["name"], package["version"])
        if pair in pairs or not package.get("files") or not (package.get("license") or package.get("license_file")):
            raise ValueError("Duplicate dependency or incomplete licensing inventory.")
        pairs.add(pair)
        listed.update(package["files"])
        key = package["name"] + "-" + package["version"]
        expected = supplements.get(key)
        if expected is not None or "upstream_supplement" in package:
            if expected is None or package.get("upstream_supplement") != expected or package.get("repository") != expected["repository"] or package.get("license") != expected["license"]:
                raise ValueError("Archive upstream license supplement has an unreviewed or mismatched identity.")
            for item in expected["files"]:
                name = f"third-party/{key}/{item['archive_file']}"
                if name not in package["files"] or name not in relative or digest(relative[name]) != item["sha256"]:
                    raise ValueError("Archive upstream license material is missing or altered.")
        if package["name"] == "unicode-ident" and not any(name.endswith("/LICENSE-UNICODE") for name in package["files"]):
            raise ValueError("Unicode license missing from archive.")
        if package["name"] == "libm" and not any(name.endswith("/src/math/sin.rs") for name in package["files"]):
            raise ValueError("libm source permission headers missing from archive.")
    if entry["windows"]:
        runtime = inventory.get("microsoft_runtime", {})
        version_vs = runtime.get("visual_studio_version", "")
        major = int(version_vs.split(".")[0]) if re.fullmatch(r"\d+(?:\.\d+)+", version_vs) else 0
        if major not in MICROSOFT_TERMS or runtime.get("product_id") not in MICROSOFT_PRODUCTS or runtime.get("terms") != MICROSOFT_TERMS[major] or {item.get("name") for item in runtime.get("libraries", [])} != {"libcmt.lib", "libvcruntime.lib", "libucrt.lib"}:
            raise ValueError("Windows runtime licensing identity is incomplete or unreviewed.")
        listed.add("third-party/MICROSOFT-RUNTIME.txt")
        terms_file = "third-party/" + runtime["terms"]["license_file"]
        if terms_file not in relative or digest(relative[terms_file]) != runtime["terms"]["text_sha256"]:
            raise ValueError("Complete reviewed Microsoft runtime terms are missing or altered.")
        listed.add(terms_file)
    if listed != {name for name in relative if name.startswith("third-party/")}:
        raise ValueError("Third-party inventory and actual notice files differ.")
    for name in ("INSTALL.txt", "USAGE.txt"):
        text = relative[name].decode("utf-8")
        if "{{" in text or "}}" in text or "@@" in text:
            raise ValueError("An archive user guide contains unresolved template markers.")
    if f"https://github.com/{repository}/tree/v{version}" not in relative["INSTALL.txt"].decode("utf-8") or "Mozilla Public License" not in relative["LICENSE"].decode("utf-8"):
        raise ValueError("Archive is missing its matching MPL source/license pointer.")
    return info


def extract_native(archive, entry, scratch):
    destination = scratch / "extracted"
    if destination.exists():
        raise ValueError("Extraction scratch already exists; use a fresh owned target directory.")
    destination.mkdir(parents=True)
    if entry["macos"]:
        run(["ditto", "-x", "-k", archive, destination])
    elif entry["windows"]:
        quote = lambda value: "'" + str(value).replace("'", "''") + "'"
        run(["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", f"$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath {quote(archive)} -DestinationPath {quote(destination)}"])
    else:
        run(["tar", "-xzf", archive, "-C", destination])
    binary = destination / entry["root"] / ("auditory-lab.exe" if entry["windows"] else "auditory-lab")
    if not entry["windows"] and stat.S_IMODE(binary.stat().st_mode) != 0o755:
        raise ValueError("Native extraction failed to preserve executable mode 0755.")
    return binary


def validate_silent_wav(data):
    """The complete RIFF/fmt/fact/data bytes for one silent stereo f32 frame."""
    expected = (b"RIFF" + struct.pack("<I", 58) + b"WAVEfmt "
                + struct.pack("<IHHIIHHH", 18, 3, 2, 48000, 384000, 8, 32, 0)
                + b"fact" + struct.pack("<II", 4, 1)
                + b"data" + struct.pack("<I", 8) + b"\0" * 8)
    if data != expected:
        raise ValueError("Packaged silent export differs from the independent complete stereo-float WAV oracle.")


def smoke(binary, version, scratch):
    if run([binary, "--version"], cwd=scratch, timeout=30).strip() != f"auditory-lab {version}":
        raise ValueError("Packaged executable reports the wrong version.")
    if "Usage:" not in run([binary, "--help"], cwd=scratch, timeout=30):
        raise ValueError("Packaged executable help is missing.")
    catalog = sorted((ROOT / "crates" / "auditory-core" / "catalog").glob("*.toml"))
    listing = run([binary, "list"], cwd=scratch, timeout=30)
    names = [line.split()[0] for line in listing.splitlines() if line.strip()]
    if not catalog or names != [path.stem for path in catalog]:
        raise ValueError("Packaged embedded catalog differs from the source catalog.")
    for path in catalog:
        shown = run([binary, "show", path.stem], cwd=scratch, timeout=30)
        if shown != path.read_text(encoding="utf-8"):
            raise ValueError(f"Packaged embedded preset differs from source: {path.stem}")
        tomllib.loads(shown)
    shown_path = scratch / "shown-catalog.toml"
    shown_path.write_text(catalog[0].read_text(encoding="utf-8"), encoding="utf-8")
    builtin = run([binary, "inspect", "--sample-rate", "48000", catalog[0].stem], cwd=scratch, timeout=30)
    from_file = run([binary, "inspect", "--sample-rate", "48000", shown_path], cwd=scratch, timeout=30)
    from_stdin = run([binary, "inspect", "--sample-rate", "48000", "-"], input=shown_path.read_text(encoding="utf-8"), cwd=scratch, timeout=30)
    # Only the first line identifies the input origin; resolved domain values
    # and provenance must agree for the embedded, saved and stdin recipes.
    if "sample rate: 48000 Hz" not in builtin or builtin.splitlines()[1:] != from_file.splitlines()[1:] or builtin.splitlines()[1:] != from_stdin.splitlines()[1:]:
        raise ValueError("Packaged builtin/file/stdin inspection roundtrip differs.")
    fixture = scratch / "silence.toml"
    fixture.write_text("schema_version = 1\nduration_seconds = 0.000020833333333333333\nfade_seconds = 0.0\n", encoding="utf-8")
    inspected = run([binary, "inspect", "--sample-rate", "48000", fixture], cwd=scratch, timeout=30)
    if "(1 frames)" not in inspected or "components: none (silence)" not in inspected:
        raise ValueError("Packaged one-frame silent preset inspection failed.")
    output = scratch / "silence.wav"
    run([binary, "export", "--sample-rate", "48000", "--output", output, fixture], cwd=scratch, timeout=30)
    data = output.read_bytes()
    validate_silent_wav(data)
    rejected = subprocess.run([str(binary), "export", "--sample-rate", "48000", "--output", str(output), str(fixture)], cwd=scratch, capture_output=True, check=False, timeout=30)
    if rejected.returncode == 0 or output.read_bytes() != data:
        raise ValueError("Packaged export overwrote an existing output.")


def package(target, version, source, repository, binary, output_directory, scratch):
    validate_source(source)
    validate_repository(repository)
    entry = target_entry(target, version)
    host = assert_host(target)
    output_directory = owned_path(output_directory)
    scratch = owned_path(scratch)
    output_directory.mkdir(parents=True, exist_ok=True)
    archive = output_directory / entry["archive"]
    if archive.is_symlink() or archive.exists():
        raise ValueError("An existing release archive or symbolic link is never overwritten.")
    scratch.mkdir(parents=True, exist_ok=True)
    stage = scratch / "stage" / entry["root"]
    if stage.exists():
        raise ValueError("Package staging already exists; use a fresh owned target directory.")
    stage.mkdir(parents=True)
    executable = stage / ("auditory-lab.exe" if entry["windows"] else "auditory-lab")
    binary = Path(binary).resolve()
    if not binary.is_file():
        raise ValueError("Release executable is missing.")
    shutil.copyfile(binary, executable)
    executable.chmod(0o755)
    if entry["macos"]:
        run(["codesign", "--force", "--sign", "-", "--timestamp=none", executable])
    platform_info = native_binary_checks(executable, target)
    microsoft = host.get("microsoft_runtime")
    copy_notices(stage, target, host["compiler"], microsoft)
    shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
    (stage / "INSTALL.txt").write_text(install_text(entry, version, repository, microsoft), encoding="utf-8")
    (stage / "USAGE.txt").write_text(usage_text(executable), encoding="utf-8")
    files = {path.relative_to(stage).as_posix(): digest(path.read_bytes()) for path in sorted(stage.rglob("*")) if path.is_file()}
    info = {"format": 1, "target": target, "version": version, "source": source, "repository": repository, "compiler": host["compiler"], "settings": {"overflow_checks": True, "cpu": "portable-default", "crt": "static-retail" if entry["windows"] else None, "macos_deployment_target": "15.0" if entry["macos"] else None}, "platform": platform_info, "files": files}
    (stage / "BUILD_INFO.json").write_text(json.dumps(info, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if archive.suffix == ".zip":
        if entry["macos"]:
            run(["ditto", "-c", "-k", "--norsrc", "--keepParent", stage, archive])
        else:
            with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
                for path in sorted(stage.rglob("*")):
                    if path.is_file():
                        output.write(path, (Path(entry["root"]) / path.relative_to(stage)).as_posix())
    else:
        with tarfile.open(archive, "x:gz") as output:
            output.add(stage, arcname=entry["root"])
    verify_archive(archive, target, version, source, repository)
    extracted = extract_native(archive, entry, scratch)
    if extracted.read_bytes() != executable.read_bytes():
        raise ValueError("Native extraction changed the executable bytes.")
    native_binary_checks(extracted, target)
    smoke(extracted, version, scratch)
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    command = commands.add_parser("host")
    command.add_argument("--target", required=True)
    for name in ("package", "verify"):
        command = commands.add_parser(name)
        for option in ("target", "version", "source", "repository"):
            command.add_argument("--" + option, required=True)
        if name == "package":
            for option in ("binary", "output-directory", "scratch"):
                command.add_argument("--" + option, type=Path, required=True)
        else:
            command.add_argument("--archive", type=Path, required=True)
            command.add_argument("--scratch", type=Path)
            command.add_argument("--native", action="store_true")
    args = parser.parse_args()
    try:
        if args.command == "host":
            print(json.dumps(assert_host(args.target), indent=2))
        elif args.command == "package":
            print(package(args.target, args.version, args.source, args.repository, args.binary, args.output_directory, args.scratch))
        else:
            info = verify_archive(args.archive, args.target, args.version, args.source, args.repository)
            if args.native:
                if args.scratch is None:
                    raise ValueError("Native verification requires fresh target-owned --scratch.")
                assert_host(args.target)
                scratch = owned_path(args.scratch)
                scratch.mkdir(parents=True, exist_ok=True)
                binary = extract_native(args.archive.resolve(), target_entry(args.target, args.version), scratch)
                native_binary_checks(binary, args.target)
                smoke(binary, args.version, scratch)
            print(json.dumps(info, indent=2))
    except (ValueError, OSError, KeyError, TypeError, UnicodeError, struct.error, json.JSONDecodeError, zipfile.BadZipFile, tarfile.TarError, subprocess.SubprocessError) as error:
        parser.exit(1, f"release archive: {error}\n")


if __name__ == "__main__":
    main()
