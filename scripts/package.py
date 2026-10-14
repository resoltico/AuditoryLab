#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Package a natively built release executable and test the packaged copy.

The archive holds one folder with four files: the executable, README.txt,
LICENSE and THIRD-PARTY-NOTICES.txt. After writing it, this script reads the
archive back, checks the executable's runtime dependencies, extracts it to a
fresh directory and runs the extracted executable.
"""

import argparse
import io
from pathlib import Path
import platform
import re
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile

from release import ROOT, TARGETS, archive_name

LINUX_LIBRARIES = {"libasound.so.2", "libgcc_s.so.1", "libm.so.6", "libc.so.6",
                   "ld-linux-x86-64.so.2", "ld-linux-aarch64.so.1"}
# Reviewed Windows OS libraries; UCRT implementations are supplied by Windows.
# Separate Visual C++/LLVM runtime DLLs remain outside this boundary.
WINDOWS_LIBRARIES = {"kernel32.dll", "ntdll.dll", "user32.dll", "advapi32.dll", "ole32.dll", "oleaut32.dll", "uuid.dll", "winmm.dll", "avrt.dll", "ws2_32.dll", "bcrypt.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "propsys.dll", "setupapi.dll", "powrprof.dll", "cfgmgr32.dll", "wintrust.dll", "crypt32.dll", "psapi.dll", "dbghelp.dll", "version.dll", "userenv.dll", "combase.dll", "mmdevapi.dll"}
WINDOWS_UCRT_LIBRARIES = {
    'api-ms-win-crt-environment-l1-1-0.dll',
    'api-ms-win-crt-heap-l1-1-0.dll',
    'api-ms-win-crt-locale-l1-1-0.dll',
    'api-ms-win-crt-math-l1-1-0.dll',
    'api-ms-win-crt-private-l1-1-0.dll',
    'api-ms-win-crt-runtime-l1-1-0.dll',
    'api-ms-win-crt-stdio-l1-1-0.dll',
    'api-ms-win-crt-string-l1-1-0.dll',
    'ucrtbase.dll',
}

# One silent stereo frame at 48 kHz, as a 32-bit float RIFF/WAVE file.
SILENT_WAV = (b"RIFF" + struct.pack("<I", 58) + b"WAVEfmt " + struct.pack("<IHHIIHHH", 18, 3, 2, 48000, 384000, 8, 32, 0)
              + b"fact" + struct.pack("<II", 4, 1) + b"data" + struct.pack("<I", 8) + bytes(8))


def run(*command, cwd=None, encoding=None):
    return subprocess.run([str(part) for part in command], cwd=cwd, check=True, capture_output=True,
                          text=True, encoding=encoding, timeout=120).stdout


def pe_imports(data, target=None):
    """Read ordinary imports from a PE32+ executable; delay loads are unsupported."""
    try:
        if data[:2] != b"MZ":
            raise ValueError("not a PE executable")
        header = struct.unpack_from("<I", data, 0x3C)[0]
        if data[header:header + 4] != b"PE\0\0":
            raise ValueError("invalid PE signature")
        machine, sections, optional_size, characteristics = struct.unpack_from("<HH12xHH", data, header + 4)
        optional = header + 24
        if (optional_size < 240 or struct.unpack_from("<H", data, optional)[0] != 0x20B
                or not characteristics & 2 or characteristics & 0x2000):
            raise ValueError("expected a PE32+ executable, not a DLL")
        if machine not in (0x8664, 0xAA64) or (target is not None and machine != (0xAA64 if target.startswith("aarch64") else 0x8664)):
            raise ValueError("PE CPU does not match the requested target")
        directories = struct.unpack_from("<I", data, optional + 108)[0]
        if directories < 14:
            raise ValueError("PE data directories are incomplete")
        if any(struct.unpack_from("<II", data, optional + 112 + 13 * 8)):
            raise ValueError("delay-loaded DLL imports are unsupported")
        import_rva, import_size = struct.unpack_from("<II", data, optional + 120)
        table = [struct.unpack_from("<4I", data, optional + optional_size + 40 * i + 8) for i in range(sections)]
        def offset(rva, size=1):
            for length, address, raw_size, raw in table:
                if address <= rva and rva + size <= address + min(length, raw_size) and raw + rva - address + size <= len(data):
                    return raw + rva - address
            raise ValueError("PE import points outside section bytes")
        names = []
        if not import_rva and not import_size:
            return names
        entry = offset(import_rva, import_size)
        for end in range(entry, entry + import_size - 19, 20):
            if not any(struct.unpack_from("<5I", data, end)):
                return names
            name = offset(struct.unpack_from("<I", data, end + 12)[0])
            terminator = data.index(b"\0", name)
            dll = data[name:terminator].decode("ascii").lower()
            if not re.fullmatch(r"[a-z0-9_.-]+\.dll", dll):
                raise ValueError("invalid PE DLL name")
            names.append(dll)
        raise ValueError("unterminated PE import directory")
    except (struct.error, UnicodeError, IndexError) as error:
        raise ValueError("malformed PE executable") from error


def check_identity(executable, target):
    if "apple" in target:
        headers = run("otool", "-hv", executable).splitlines()
        expected = "ARM64" if target.startswith("aarch64") else "X86_64"
        rows = [line.split() for line in headers if line.strip().startswith("MH_")]
        if len(rows) != 1 or rows[0][0] != "MH_MAGIC_64" or rows[0][1] != expected or rows[0][4] != "EXECUTE":
            raise ValueError("Mach-O CPU/type does not match the requested executable target")
    elif "linux" in target:
        header = run("readelf", "-h", executable)
        expected = "AArch64" if target.startswith("aarch64") else "Advanced Micro Devices X86-64"
        def field(name):
            match = re.search(rf"^\s*{name}:\s*(.+)$", header, re.M)
            return match[1].strip() if match else ""
        if field("Class") != "ELF64" or field("Machine") != expected or not re.match(r"(?:EXEC|DYN)\b", field("Type")):
            raise ValueError("ELF CPU/type does not match the requested executable target")
        # DYN also describes shared libraries. Require exactly one interpreter
        # segment and the target's system loader, not a runner-private loader.
        segments = run("readelf", "-lW", executable)
        loader = "/lib/ld-linux-aarch64.so.1" if target.startswith("aarch64") else "/lib64/ld-linux-x86-64.so.2"
        interpreters = re.findall(r"^\s*\[Requesting program interpreter: ([^\]\r\n]+)\]\s*$", segments, re.M)
        if len(re.findall(r"^\s*INTERP\s", segments, re.M)) != 1 or interpreters != [loader]:
            raise ValueError(f"ELF executable must request exactly the system loader {loader}")
    else:
        pe_imports(executable.read_bytes(), target)


def check_macos_minimum(executable, minimum):
    declared, command, macos = [], None, False
    for line in run("otool", "-l", executable).splitlines():
        fields = line.split()
        if fields[:2] == ["Load", "command"]:
            command, macos = None, False
        elif fields[:1] == ["cmd"]:
            command = fields[1]
        elif command == "LC_BUILD_VERSION" and fields[:1] == ["platform"]:
            macos = fields[1] in ("1", "macos")
        elif command == "LC_BUILD_VERSION" and macos and fields[:1] == ["minos"]:
            declared.append(fields[1])
        elif command == "LC_VERSION_MIN_MACOSX" and fields[:1] == ["version"]:
            declared.append(fields[1])
    if declared != [minimum]:
        raise ValueError(f"the executable declares macOS {declared}, not {minimum}")


def check_dependencies(executable, target):
    """Allow only reviewed system libraries. Require a known OS import sentinel
    so an empty or broken dependency reader cannot pass."""
    if "apple" in target:
        run("codesign", "--verify", "--strict", executable)
        libraries = [line.split(" (")[0].strip() for line in run("otool", "-L", executable).splitlines()[1:]]
        required = "/usr/lib/libSystem.B.dylib"
        bad = [name for name in libraries if not name.startswith(("/usr/lib/", "/System/Library/"))]
    elif "linux" in target:
        dynamic = run("readelf", "-d", executable)
        if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
            raise ValueError("ELF RPATH/RUNPATH is not permitted")
        versions = re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", run("readelf", "-W", "--version-info", executable))
        if not versions or any(tuple(map(int, version.split("."))) > (2, 35) for version in versions):
            raise ValueError(f"Linux GLIBC requirements must not exceed 2.35: {versions}")
        libraries = re.findall(r"\(NEEDED\).*\[(.+)\]", dynamic)
        required = "libc.so.6"
        bad = sorted(set(libraries) - LINUX_LIBRARIES)
    else:
        libraries = pe_imports(executable.read_bytes(), target)
        required = "kernel32.dll"
        bad = [name for name in libraries if name not in WINDOWS_LIBRARIES | WINDOWS_UCRT_LIBRARIES
               and (not re.fullmatch(r"(?:api|ext)-ms-win-[a-z0-9-]+\.dll", name)
                    or name.startswith("api-ms-win-crt-"))]
    if required not in libraries:
        raise ValueError(f"the executable's dependency list lacks {required}: {libraries}")
    if bad:
        raise ValueError(f"unexpected runtime dependencies: {bad}")


def members(archive):
    """Read exact regular-file records, rejecting duplicates and other types."""
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as source:
            records = source.infolist()
            if len({info.filename for info in records}) != len(records) or any(not stat.S_ISREG(info.external_attr >> 16) for info in records):
                raise ValueError("duplicate or nonregular ZIP member")
            return {info.filename: (info.external_attr >> 16, source.read(info)) for info in records}
    with tarfile.open(archive) as source:
        records = source.getmembers()
        if len({info.name for info in records}) != len(records) or any(not info.isfile() for info in records):
            raise ValueError("duplicate or nonregular tar member")
        return {info.name: (info.mode | stat.S_IFREG, source.extractfile(info).read()) for info in records}


def smoke(executable, version, scratch):
    if run(executable, "--version", encoding="utf-8").strip() != f"auditory-lab {version}":
        raise ValueError("the packaged executable reports another version")
    catalog = sorted((ROOT / "crates" / "auditory-core" / "catalog").glob("*.toml"))
    listed = [line.split()[0] for line in run(executable, "list", encoding="utf-8").splitlines()]
    if sorted(listed) != [path.stem for path in catalog]:
        raise ValueError("the packaged catalog differs from the source catalog")
    saved = scratch / "saved.toml"
    run(executable, "show", catalog[0].stem, "--output", saved, encoding="utf-8")
    if saved.read_bytes() != catalog[0].read_bytes():
        raise ValueError("show --output wrote other bytes than the catalog file")
    preset = scratch / "silence.toml"
    preset.write_text("schema_version = 2\nduration_seconds = 0.000020833333333333333\nfade_seconds = 0\n")
    wav = scratch / "silence.wav"
    run(executable, "export", preset, "--sample-rate", "48000", "--output", wav, encoding="utf-8")
    if wav.read_bytes() != SILENT_WAV:
        raise ValueError("the packaged export differs from the expected WAV bytes")
    refused = subprocess.run([str(executable), "export", str(preset), "--output", str(wav)], capture_output=True)
    if refused.returncode != 1 or wav.read_bytes() != SILENT_WAV:
        raise ValueError("the packaged export replaced an existing file")


def check_macos_native():
    # -i ignores a missing key on native Intel macOS; -n prints only its value.
    translated = run("/usr/sbin/sysctl", "-in", "sysctl.proc_translated").strip()
    if translated not in ("", "0"):
        raise ValueError("packaging requires a native macOS process, not Rosetta translation")


def package(target, version, repository, binary, output, link_trace=None):
    entry = next((item for item in TARGETS if item["target"] == target), None)
    if entry is None:
        raise ValueError(f"unsupported release target: {target}")
    if "windows" in target and link_trace is None:
        raise ValueError("Windows packaging requires --link-trace from windows_runtime.py build")
    if "windows" not in target and link_trace is not None:
        raise ValueError("--link-trace applies only to Windows packages")
    host = re.search(r"^host: (\S+)", run("rustc", "-vV"), re.M)[1]
    if host != target:
        raise ValueError(f"packaging {target} needs a native {target} host, not {host}")
    operating_system = "Darwin" if "apple" in target else "Linux" if "linux" in target else "Windows"
    architectures = {"aarch64": {"aarch64", "arm64"}, "x86_64": {"x86_64", "amd64"}}
    if platform.system() != operating_system or platform.machine().lower() not in architectures[target.split("-")[0]]:
        raise ValueError("the operating system and native machine do not match the requested target")
    if "apple" in target:
        check_macos_native()
    name = archive_name(entry["platform"], version)
    root = name.removesuffix(".zip").removesuffix(".tar.gz")
    exe_name = "auditory-lab.exe" if "windows" in target else "auditory-lab"
    output.mkdir(parents=True, exist_ok=True)
    if (output / name).exists() or (output / name).is_symlink():
        raise ValueError(f"{output / name} already exists")
    with tempfile.TemporaryDirectory(dir=output) as scratch:
        # The archive reaches `output` only after every check has passed.
        archive = Path(scratch) / name
        stage = Path(scratch) / root
        stage.mkdir()
        executable = stage / exe_name
        shutil.copyfile(binary, executable)
        if "apple" in target:
            run("codesign", "--force", "--sign", "-", executable)
        check_identity(executable, target)
        if "windows" in target:
            from windows_runtime import verify
            verify(executable, target, link_trace)
        check_dependencies(executable, target)
        if "minimum" in entry:
            check_macos_minimum(executable, entry["minimum"])
        minimums = {item["platform"]: item["minimum"] for item in TARGETS if "minimum" in item}
        readme = (ROOT / "scripts" / "archive-readme.txt").read_text(encoding="utf-8").format(
            version=version, repository=repository,
            macos_aarch64=minimums["macos-aarch64"], macos_x86_64=minimums["macos-x86_64"])
        (stage / "README.txt").write_text(readme, encoding="utf-8")
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        shutil.copyfile(ROOT / "THIRD-PARTY-NOTICES.txt", stage / "THIRD-PARTY-NOTICES.txt")
        files = {path.name: (0o755 if path == executable else 0o644, path.read_bytes()) for path in stage.iterdir()}
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive, "x", zipfile.ZIP_DEFLATED) as zipped:
                for file, (mode, data) in sorted(files.items()):
                    info = zipfile.ZipInfo(f"{root}/{file}", time.localtime()[:6])
                    info.create_system, info.external_attr = 3, (stat.S_IFREG | mode) << 16
                    zipped.writestr(info, data, zipfile.ZIP_DEFLATED)
        else:
            with tarfile.open(archive, "x:gz") as tarred:
                for file, (mode, data) in sorted(files.items()):
                    info = tarfile.TarInfo(f"{root}/{file}")
                    info.size, info.mode, info.mtime = len(data), mode, int(time.time())
                    tarred.addfile(info, io.BytesIO(data))
        expected = {f"{root}/{file}": (stat.S_IFREG | mode, data) for file, (mode, data) in files.items()}
        if members(archive) != expected:
            raise ValueError("the archive does not hold exactly the staged files")
        extracted = Path(scratch) / "extracted"
        extracted.mkdir()
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as zipped:
                zipped.extractall(extracted)
            # Python's extraction drops the mode that archive tools restore.
            (extracted / root / exe_name).chmod(0o755)
        else:
            run("tar", "-xzf", archive, "-C", extracted)
        unpacked = extracted / root / exe_name
        smoke(unpacked, version, Path(scratch))
        return Path(shutil.move(archive, output / name))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for option in ("target", "version", "repository"):
        parser.add_argument("--" + option, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--link-trace", type=Path, help="verified Windows native link receipt")
    args = parser.parse_args()
    try:
        print(package(args.target, args.version, args.repository, args.binary, args.output, args.link_trace))
    except (ValueError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile, tarfile.TarError) as error:
        sys.exit(f"package: {error}")


if __name__ == "__main__":
    main()
