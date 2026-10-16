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
import json
from decimal import Decimal
from pathlib import Path
import platform
import plistlib
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

from release import (ROOT, TARGETS, PRODUCTS, archive_name, cargo_identity, MACOS_MINIMUM,
                     check_build_identity, expected_identity, validate_build_request)

LINUX_LIBRARIES = {"libasound.so.2", "libgcc_s.so.1", "libm.so.6", "libc.so.6",
                   "ld-linux-x86-64.so.2", "ld-linux-aarch64.so.1"}
# Reviewed Windows OS libraries; UCRT implementations are supplied by Windows.
# Separate Visual C++/LLVM runtime DLLs remain outside this boundary.
WINDOWS_LIBRARIES = {"kernel32.dll", "ntdll.dll", "user32.dll", "advapi32.dll", "ole32.dll", "oleaut32.dll", "uuid.dll", "winmm.dll", "avrt.dll", "ws2_32.dll", "bcrypt.dll", "bcryptprimitives.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "propsys.dll", "setupapi.dll", "powrprof.dll", "cfgmgr32.dll", "wintrust.dll", "crypt32.dll", "psapi.dll", "dbghelp.dll", "version.dll", "userenv.dll", "combase.dll", "mmdevapi.dll"}
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

# Additional OS APIs used by the selected Winit/software/font/accessibility profile.
# Keep this separate: adding a GUI import never expands the CLI envelope.
GUI_WINDOWS_LIBRARIES = WINDOWS_LIBRARIES | {
    "gdi32.dll", "dwrite.dll", "dwmapi.dll", "imm32.dll", "comctl32.dll",
    "uiautomationcore.dll", "oleacc.dll", "uxtheme.dll", "shcore.dll", "hid.dll", "msimg32.dll",
    "d3d12.dll", "dxgi.dll", "d3dcompiler_47.dll", "opengl32.dll",
}
GUI_LINUX_SYSTEM_FONTS = {"libfreetype.so.6", "libfontconfig.so.1"}
GUI_LINUX_LIBRARIES = LINUX_LIBRARIES | GUI_LINUX_SYSTEM_FONTS

# One silent stereo frame at 48 kHz, as a 32-bit float RIFF/WAVE file.
SILENT_WAV = (b"RIFF" + struct.pack("<I", 58) + b"WAVEfmt " + struct.pack("<IHHIIHHH", 18, 3, 2, 48000, 384000, 8, 32, 0)
              + b"fact" + struct.pack("<II", 4, 1) + b"data" + struct.pack("<I", 8) + bytes(8))


def subprocess_failure(error):
    """Bounded diagnostic evidence from failed native tools and smoke commands."""
    sections = [str(error)]
    for label, value in (("stdout", error.stdout), ("stderr", error.stderr)):
        if value:
            if isinstance(value, bytes):
                value = value.decode("utf-8", errors="backslashreplace")
            sections.append(f"{label}:\n{value[-16384:]}")
    return "\n".join(sections)


def run(*command, cwd=None, encoding=None, text=True):
    try:
        return subprocess.run([str(part) for part in command], cwd=cwd, check=True, capture_output=True,
                              text=text, encoding=encoding, timeout=120).stdout
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        raise ValueError(subprocess_failure(error)) from error


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


def check_identity(executable, target, product="cli"):
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
        data = executable.read_bytes()
        pe_imports(data, target)
        if product == "gui":
            header = struct.unpack_from("<I", data, 0x3C)[0]
            if struct.unpack_from("<H", data, header + 24 + 68)[0] != 2:
                raise ValueError("GUI executable must use the Windows GUI subsystem")


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


def check_dependencies(executable, target, product="cli"):
    """Allow only reviewed system libraries. Require a known OS import sentinel
    so an empty or broken dependency reader cannot pass."""
    if "apple" in target:
        run("codesign", "--verify", "--strict", executable)
        libraries = [line.split(" (")[0].strip() for line in run("otool", "-L", executable).splitlines()[1:]]
        required = "/usr/lib/libSystem.B.dylib"
        if product == "cli":
            bad = [name for name in libraries if not name.startswith(("/usr/lib/", "/System/Library/"))]
        else:
            # Reviewed Slint software/Winit/rfd host probe plus the existing CPAL route.
            frameworks = {"AppKit", "ApplicationServices", "CoreGraphics", "CoreVideo", "Carbon",
                          "Foundation", "CoreFoundation", "QuartzCore", "CoreText", "ColorSync",
                          "CoreServices", "CoreAudio", "AudioToolbox", "Metal"}
            allowed = {"/usr/lib/libSystem.B.dylib", "/usr/lib/libobjc.A.dylib", "/usr/lib/libiconv.2.dylib"}
            bad = [name for name in libraries if name not in allowed and not any(
                re.fullmatch(r"/System/Library/Frameworks/" + framework + r"\.framework/Versions/[^/]+/" + framework, name)
                for framework in frameworks)]
    elif "linux" in target:
        dynamic = run("readelf", "-d", executable)
        if re.search(r"\((?:RPATH|RUNPATH)\)", dynamic):
            raise ValueError("ELF RPATH/RUNPATH is not permitted")
        versions = re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", run("readelf", "-W", "--version-info", executable))
        if not versions or any(tuple(map(int, version.split("."))) > (2, 35) for version in versions):
            raise ValueError(f"Linux GLIBC requirements must not exceed 2.35: {versions}")
        libraries = re.findall(r"\(NEEDED\).*\[(.+)\]", dynamic)
        if product == "gui" and not GUI_LINUX_SYSTEM_FONTS <= set(libraries):
            raise ValueError("GUI ELF requires system FreeType and Fontconfig imports")
        required = "libc.so.6"
        bad = sorted(set(libraries) - (LINUX_LIBRARIES if product == "cli" else GUI_LINUX_LIBRARIES))
    else:
        libraries = pe_imports(executable.read_bytes(), target)
        required = "kernel32.dll"
        allowed = WINDOWS_LIBRARIES if product == "cli" else GUI_WINDOWS_LIBRARIES
        bad = [name for name in libraries if name not in allowed | WINDOWS_UCRT_LIBRARIES
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


def catalog_order(path):
    """Match the catalog's documented natural identifier ordering."""
    return [(1, int(part)) if part.isdecimal() else (0, part)
            for part in re.split(r"(\d+)", path.stem)]


def listed_duration(text):
    """Read the controlled human duration view for package smoke assertions."""
    approximate = text.startswith("~")
    value = text[1:] if approximate else text
    parts = list(re.finditer(r"(\d+(?:\.\d+)?) (h|min|s)", value))
    if not parts or " ".join(part[0] for part in parts) != value:
        raise ValueError(f"invalid listed duration: {text!r}")
    units = [part[2] for part in parts]
    if units != [unit for unit in ("h", "min", "s") if unit in units]:
        raise ValueError(f"duplicate or unordered listed duration units: {text!r}")
    factors = {"h": 3600, "min": 60, "s": 1}
    return sum((Decimal(part[1]) * factors[part[2]] for part in parts), Decimal(0)), approximate


def validate_catalog_listing(text, catalog):
    """Assert the controlled two-part catalog view; this is not a public protocol.

    Source extraction deliberately covers the current reviewed catalog's single
    numeric duration and JSON-compatible basic-string title. A changed source
    shape needs a deliberate smoke update, not an alternate TOML implementation.
    """
    lines = text.splitlines()
    if not lines or re.fullmatch(r"PRESET +DURATION", lines[0]) is None:
        raise ValueError("the packaged catalog must begin with one PRESET/DURATION header")
    rows = []
    for line in lines[1:]:
        if not line.strip():
            raise ValueError("the packaged catalog contains a blank record")
        if not line[0].isspace():
            match = re.fullmatch(r"([a-z0-9-]+) +(.+)", line)
            if match is None:
                raise ValueError(f"invalid packaged catalog ID/duration row: {line!r}")
            rows.append((match[1], match[2], []))
        else:
            if not rows:
                raise ValueError("the packaged catalog has an orphan Sound continuation")
            content = line.lstrip()
            sound = rows[-1][2]
            if content.startswith("Sound:"):
                if sound or not content.startswith("Sound: ") or not content[7:]:
                    raise ValueError("the packaged catalog has a duplicate or empty Sound field")
                sound.append(content[7:])
            elif sound:
                sound.append(content)
            else:
                raise ValueError("the packaged catalog continuation must begin with Sound:")
    catalog = sorted(catalog, key=catalog_order)
    if not catalog:
        raise ValueError("the source catalog is empty")
    if [row[0] for row in rows] != [path.stem for path in catalog]:
        raise ValueError("the packaged catalog has missing, extra, duplicate, reordered or truncated IDs")
    for (identifier, duration, sound), path in zip(rows, catalog):
        source = path.read_text(encoding="utf-8")
        declared = re.findall(r"^duration_seconds = ([0-9.eE+-]+)$", source, re.M)
        titles = re.findall(r'^title = (".*")$', source, re.M)
        if len(declared) != 1 or len(titles) != 1:
            raise ValueError(f"{path} needs review for catalog smoke source extraction")
        seconds, approximate = listed_duration(duration)
        tolerance = Decimal("0.0000000005") if approximate else Decimal(0)
        if abs(seconds - Decimal(declared[0])) > tolerance:
            raise ValueError(f"the packaged catalog duration differs for {identifier}")
        if not sound or " ".join(" ".join(sound).split()) != " ".join(json.loads(titles[0]).split()):
            raise ValueError(f"the packaged catalog Sound differs for {identifier}")


def smoke(executable, version, scratch):
    # Exact structured identity is checked separately; --version remains a human view.
    if re.match(rf"\Aauditorylab {re.escape(version)}(?: |$)", run(executable, "--version", encoding="utf-8").strip()) is None:
        raise ValueError("the packaged executable reports another version")
    catalog = sorted((ROOT / "crates" / "auditory-core" / "catalog").glob("*.toml"), key=catalog_order)
    validate_catalog_listing(run(executable, "list", encoding="utf-8"), catalog)
    if run(executable, "recipe", catalog[0].stem, text=False) != catalog[0].read_bytes():
        raise ValueError("recipe stdout wrote other bytes than the catalog file")
    saved = scratch / "saved.toml"
    run(executable, "recipe", catalog[0].stem, "--output", saved, encoding="utf-8")
    if saved.read_bytes() != catalog[0].read_bytes():
        raise ValueError("recipe --output wrote other bytes than the catalog file")
    preset = scratch / "silence.toml"
    preset.write_text("schema_version = 2\nduration_seconds = 0.000020833333333333333\nfade_seconds = 0\n")
    run(executable, "inspect", "--file", preset, encoding="utf-8")
    wav = scratch / "silence.wav"
    run(executable, "export", "--file", preset, "--sample-rate", "48000", "--output", wav, encoding="utf-8")
    if wav.read_bytes() != SILENT_WAV:
        raise ValueError("the packaged export differs from the expected WAV bytes")
    command = [str(executable), "export", "--file", str(preset), "--output", str(wav)]
    try:
        refused = subprocess.run(command, capture_output=True, timeout=120)
    except subprocess.TimeoutExpired as error:
        raise ValueError(subprocess_failure(error)) from error
    if refused.returncode != 1:
        error = subprocess.CalledProcessError(refused.returncode, command, refused.stdout, refused.stderr)
        raise ValueError("packaged export must reject an existing destination with status 1:\n" +
                         subprocess_failure(error))
    if wav.read_bytes() != SILENT_WAV:
        raise ValueError("the packaged export replaced an existing file")


def check_macos_native():
    # -i ignores a missing key on native Intel macOS; -n prints only its value.
    translated = run("/usr/sbin/sysctl", "-in", "sysctl.proc_translated").strip()
    if translated not in ("", "0"):
        raise ValueError("packaging requires a native macOS process, not Rosetta translation")


def executable_member(product, target):
    binary = PRODUCTS[product]["bin"] + (".exe" if "windows" in target else "")
    return "Auditory Lab.app/Contents/MacOS/" + binary if product == "gui" and "apple" in target else binary


def expected_members(product, target):
    result = {executable_member(product, target), "README.txt", "LICENSE", "THIRD-PARTY-NOTICES.txt"}
    if product == "gui" and "apple" in target:
        result.update({"Auditory Lab.app/Contents/Info.plist", "Auditory Lab.app/Contents/_CodeSignature/CodeResources"})
    if product == "gui" and "linux" in target:
        result.add("auditorylab-gui.desktop")
    return result


def bundle_metadata(version):
    return {"CFBundleName": "Auditory Lab", "CFBundleDisplayName": "Auditory Lab",
            "CFBundleIdentifier": "org.resoltico.auditorylab", "CFBundleExecutable": PRODUCTS["gui"]["bin"],
            "CFBundlePackageType": "APPL", "CFBundleShortVersionString": version,
            "CFBundleVersion": version, "LSMinimumSystemVersion": MACOS_MINIMUM, "NSHighResolutionCapable": True}


def validate_bundle(data, version):
    if plistlib.loads(data) != bundle_metadata(version):
        raise ValueError("GUI bundle metadata differs from the product/version/minimum-OS contract")


def gui_smoke(executable, scratch):
    directory = scratch / "gui-check"
    directory.mkdir()
    result = json.loads(run(executable, "--check-package", directory, encoding="utf-8"))
    if result != {"frames": 1, "sample_rate": 48000, "file": "silence.wav"}:
        raise ValueError("GUI package check returned incorrect facade facts")
    wavs = list(directory.glob("*.wav"))
    if len(wavs) != 1 or wavs[0].read_bytes() != SILENT_WAV:
        raise ValueError("GUI package check must produce exactly one independent byte-verified silent WAV")
    command = [str(executable), "--check-package", str(directory)]
    result = subprocess.run(command, capture_output=True, timeout=120)
    if result.returncode == 0 or wavs[0].read_bytes() != SILENT_WAV:
        raise ValueError("GUI package check must reject a nonempty destination without overwriting")


def source_reference(repository, build_kind, source_revision):
    validate_build_request(build_kind, source_revision)
    if source_revision is not None:
        return f"Documentation and exact build source:\nhttps://github.com/{repository}/tree/{source_revision}#readme"
    return ("Exact build source: this unqualified local artifact comes from its builder's\n"
            "checkout. Obtain that exact checkout from the builder; no immutable Git\n"
            "revision is claimed. Upstream project documentation may differ from this\n"
            f"build: https://github.com/{repository}#readme")


def package(target, version, repository, binary, output, link_trace=None, product="cli",
            build_kind="local", source_revision=None):
    validate_build_request(build_kind, source_revision)
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
    name = archive_name(entry["platform"], version, product, build_kind, source_revision)
    root = name.removesuffix(".zip").removesuffix(".tar.gz")
    output.mkdir(parents=True, exist_ok=True)
    if (output / name).exists() or (output / name).is_symlink():
        raise ValueError(f"{output / name} already exists")
    identity = cargo_identity(product=product)
    if identity["version"] != version:
        raise ValueError(f"package version {version} differs from Cargo's {identity['version']}")
    exe_name = executable_member(product, target)
    with tempfile.TemporaryDirectory(dir=output) as scratch:
        # The archive reaches `output` only after every check has passed.
        archive = Path(scratch) / name
        stage = Path(scratch) / root
        stage.mkdir()
        executable = stage / exe_name
        executable.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary, executable)
        executable.chmod(0o755)
        if "apple" in target:
            run("codesign", "--force", "--sign", "-", executable)
        check_build_identity(json.loads(run(executable, "--build-info", encoding="utf-8")),
                             product, version, target, build_kind, source_revision)
        check_identity(executable, target, product)
        if "windows" in target:
            from windows_runtime import verify
            verify(executable, target, link_trace, product=product)
        check_dependencies(executable, target, product)
        if "apple" in target:
            check_macos_minimum(executable, MACOS_MINIMUM)
        readme = (ROOT / "scripts" / "archive-readme.txt").read_text(encoding="utf-8").format(
            version=version, repository=repository,
            macos_minimum=MACOS_MINIMUM,
            source_reference=source_reference(repository, build_kind, source_revision))
        if product == "gui":
            readme = (ROOT / "scripts/archive-readme-gui.txt").read_text(encoding="utf-8").format(
                version=version, macos_minimum=MACOS_MINIMUM,
                source_reference=source_reference(repository, build_kind, source_revision))
        readme += "\nBuild identity: " + json.dumps(expected_identity(product, version, target, build_kind, source_revision), sort_keys=True) + "\n"
        (stage / "README.txt").write_text(readme, encoding="utf-8")
        if product == "gui" and "apple" in target:
            bundle = stage / "Auditory Lab.app"
            (bundle / "Contents/Info.plist").write_bytes(plistlib.dumps(bundle_metadata(version)))
            run("codesign", "--force", "--sign", "-", bundle)
            run("codesign", "--verify", "--strict", bundle)
        if product == "gui" and "linux" in target:
            (stage / "auditorylab-gui.desktop").write_text(
                "[Desktop Entry]\nType=Application\nName=Auditory Lab\nExec=auditorylab-gui\nTerminal=false\nCategories=Science;\n", encoding="utf-8")
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        shutil.copyfile(ROOT / "THIRD-PARTY-NOTICES.txt", stage / "THIRD-PARTY-NOTICES.txt")
        files = {path.relative_to(stage).as_posix(): (0o755 if path == executable else 0o644, path.read_bytes())
                 for path in stage.rglob("*") if path.is_file()}
        if set(files) != expected_members(product, target):
            raise ValueError("staged package members differ from the product contract")
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
        check_build_identity(json.loads(run(unpacked, "--build-info", encoding="utf-8")),
                             product, version, target, build_kind, source_revision)
        if product == "gui":
            gui_smoke(unpacked, Path(scratch))
        else:
            smoke(unpacked, version, Path(scratch))
        return Path(shutil.move(archive, output / name))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for option in ("target", "version", "repository"):
        parser.add_argument("--" + option, required=True)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--link-trace", type=Path, help="verified Windows native link receipt")
    parser.add_argument("--product", choices=PRODUCTS, default="cli")
    parser.add_argument("--build-kind", choices=("local", "candidate", "release"), default="local")
    parser.add_argument("--source-revision")
    args = parser.parse_args()
    try:
        print(package(args.target, args.version, args.repository, args.binary, args.output, args.link_trace,
                      args.product, args.build_kind, args.source_revision))
    except (ValueError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile, tarfile.TarError) as error:
        sys.exit(f"package: {error}")


if __name__ == "__main__":
    main()
