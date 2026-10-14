# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Rejecting controls for the native release archive boundary."""

import copy
import io
import json
import os
from pathlib import Path
import stat
import struct
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import release_archive as archive


VERSION = "0.1.0"
SOURCE = "a" * 40
REPOSITORY = "resoltico/AuditoryLab"
MAC = "aarch64-apple-darwin"
LINUX = "x86_64-unknown-linux-gnu"
WINDOWS = "x86_64-pc-windows-msvc"


def mach_binary(cpu=0x100000C, minimum=0xF0000):
    header = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 2, 40, 0, 0)
    build = struct.pack("<IIIIII", 0x32, 24, 1, minimum, 0xF0000, 0)
    signature = struct.pack("<IIII", 0x1D, 16, 72, 8)
    return header + build + signature + b"signature"


def elf_binary(machine=62):
    data = bytearray(512)
    data[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<HH", data, 16, 3, machine)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
    interpreter = b"/lib64/ld-linux-x86-64.so.2\0"
    struct.pack_into("<IIQQQQQQ", data, 64, 3, 4, 128, 128, 128, len(interpreter), len(interpreter), 1)
    data[128:128 + len(interpreter)] = interpreter
    return bytes(data)


def pe_binary(machine=0x8664, dll=b"kernel32.dll"):
    data = bytearray(1024)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 128)
    data[128:132] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 132, machine, 1, 0, 0, 0, 240, 2)
    struct.pack_into("<H", data, 152, 0x20B)
    struct.pack_into("<II", data, 152 + 120, 0x1000, 40)
    struct.pack_into("<IIII", data, 152 + 240 + 8, 512, 0x1000, 512, 512)
    struct.pack_into("<IIIII", data, 512, 0x1060, 0, 0, 0x1080, 0x1070)
    data[640:640 + len(dll) + 1] = dll + b"\0"
    return bytes(data)


def valid_payload(target=MAC):
    entry = archive.target_entry(target, VERSION)
    executable = "auditory-lab.exe" if entry["windows"] else "auditory-lab"
    binary = pe_binary() if entry["windows"] else mach_binary() if entry["macos"] else elf_binary()
    platform_info = archive.executable_header(binary, target)
    if entry["macos"]:
        platform_info["libraries"] = ["/usr/lib/libSystem.B.dylib"]
    elif not entry["windows"]:
        platform_info.update(libraries=["libasound.so.2", "libc.so.6"], glibc_versions=["2.2.5", "2.35"])
    notices = {"format": 1, "target": target, "rust": {"version": archive.pinned_channel(), "file": "third-party/RUST-COPYRIGHT-library.html"}, "packages": [{"name": "libm", "version": "0.2.16", "license": "MIT", "files": ["third-party/libm-0.2.16/LICENSE.txt", "third-party/libm-0.2.16/src/math/sin.rs"]}]}
    files = {executable: binary, "LICENSE": b"Mozilla Public License Version 2.0", "USAGE.txt": b"Use --help.\n", "INSTALL.txt": f"https://github.com/{REPOSITORY}/tree/v{VERSION}\n".encode(), "third-party/RUST-COPYRIGHT-library.html": b"Runtime notices", "third-party/libm-0.2.16/LICENSE.txt": b"MIT license", "third-party/libm-0.2.16/src/math/sin.rs": b"Sun permission header"}
    if entry["windows"]:
        runtime = {"visual_studio_version": "17.14.1.0", "product_id": "Microsoft.VisualStudio.Product.Enterprise", "toolset_version": "14.44.1", "windows_sdk_version": "10.0.1.0", "terms": archive.MICROSOFT_TERMS[17], "libraries": [{"name": name, "sha256": "b" * 64} for name in ("libcmt.lib", "libvcruntime.lib", "libucrt.lib")]}
        notices["microsoft_runtime"] = runtime
        files["third-party/MICROSOFT-RUNTIME.txt"] = b"Microsoft runtime terms reference"
        files["third-party/" + runtime["terms"]["license_file"]] = (archive.ROOT / "scripts" / "licenses" / runtime["terms"]["license_file"]).read_bytes()
    files["third-party/index.json"] = json.dumps(notices).encode()
    info = {"format": 1, "target": target, "version": VERSION, "source": SOURCE, "repository": REPOSITORY, "compiler": {"release": archive.pinned_channel(), "host": target}, "settings": {"overflow_checks": True, "cpu": "portable-default", "crt": "static-retail" if entry["windows"] else None, "macos_deployment_target": "15.0" if entry["macos"] else None}, "platform": platform_info}
    return entry, files, info


class ArchiveChecks(unittest.TestCase):
    def setUp(self):
        scratch = archive.ROOT / "target" / "release-archive-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def write_archive(self, target=MAC, change=None, additional=None, mode=0o755):
        entry, files, info = valid_payload(target)
        if change:
            change(files, info)
        info["files"] = {name: archive.digest(data) for name, data in files.items()}
        files["BUILD_INFO.json"] = json.dumps(info).encode()
        path = self.directory / entry["archive"]
        if path.suffix == ".zip":
            with zipfile.ZipFile(path, "w") as output:
                for name, data in files.items():
                    member = zipfile.ZipInfo(entry["root"] + "/" + name)
                    member.create_system = 3
                    member.external_attr = (stat.S_IFREG | (mode if name == "auditory-lab" else 0o644)) << 16
                    output.writestr(member, data)
                if additional:
                    additional(output, entry)
        else:
            with tarfile.open(path, "w:gz") as output:
                for name, data in files.items():
                    member = tarfile.TarInfo(entry["root"] + "/" + name)
                    member.size = len(data)
                    member.mode = mode if name == "auditory-lab" else 0o644
                    output.addfile(member, io.BytesIO(data))
                if additional:
                    additional(output, entry)
        return path

    def verify(self, path, target=MAC):
        return archive.verify_archive(path, target, VERSION, SOURCE, REPOSITORY)

    def test_all_native_archives_have_unique_matrix_identity(self):
        entries = archive.matrix_for(VERSION)
        self.assertEqual(len(entries), 6)
        for key in ("target", "archive", "artifact", "root"):
            self.assertEqual(len({entry[key] for entry in entries}), 6)
        self.assertTrue(all(entry["artifact"].startswith("release-") for entry in entries))
        with self.assertRaises(ValueError):
            archive.matrix_for("01.0.0")
        with self.assertRaises(ValueError):
            archive.target_entry("universal-apple-darwin", VERSION)

    def test_valid_structural_mac_windows_and_linux_controls(self):
        for target in (MAC, WINDOWS, LINUX):
            with self.subTest(target=target):
                self.assertEqual(self.verify(self.write_archive(target), target)["target"], target)

    def test_wrong_architecture_fat_unsigned_and_wrong_minos_are_rejected(self):
        invalid = [mach_binary(cpu=0x1000007), mach_binary(minimum=0xE0200), mach_binary(minimum=0xF0100), b"\xca\xfe\xba\xbe" + b"\0" * 100, mach_binary()[:56]]
        for binary in invalid:
            with self.subTest(binary=binary[:8]):
                with self.assertRaises(ValueError):
                    archive.executable_header(binary, MAC)
        with self.assertRaises(ValueError):
            archive.executable_header(elf_binary(machine=183), LINUX)
        with self.assertRaises(ValueError):
            archive.executable_header(pe_binary(machine=0xAA64), WINDOWS)

    def test_static_crt_rejects_runtime_and_unreviewed_dlls(self):
        archive.executable_header(pe_binary(), WINDOWS)
        for dll in (b"vcruntime140.dll", b"ucrtbase.dll", b"api-ms-win-crt-stdio-l1-1-0.dll", b"runner-private.dll"):
            with self.subTest(dll=dll), self.assertRaises(ValueError):
                archive.executable_header(pe_binary(dll=dll), WINDOWS)

    def test_runtime_deployment_controls_reject_accidental_dependencies(self):
        archive.check_macos_libraries(["/System/Library/Frameworks/CoreAudio.framework/Versions/A/CoreAudio", "/usr/lib/libSystem.B.dylib"])
        for libraries in ([], ["/opt/homebrew/lib/libprivate.dylib"], ["@rpath/libprivate.dylib"]):
            with self.assertRaises(ValueError):
                archive.check_macos_libraries(libraries)
        archive.check_linux_dependencies(["libasound.so.2", "libc.so.6"], ["2.2.5", "2.35"])
        for libraries, versions in ((["libc.so.6"], ["2.35"]), (["libasound.so.2", "libprivate.so"], ["2.35"]), (["libasound.so.2"], []), (["libasound.so.2"], ["2.36"])):
            with self.assertRaises(ValueError):
                archive.check_linux_dependencies(libraries, versions)

    def test_identity_setting_and_content_rejections(self):
        for key, value in (("source", "c" * 40), ("target", WINDOWS), ("version", "0.2.0"), ("repository", "other/repository")):
            with self.subTest(key=key):
                path = self.write_archive(change=lambda files, info: info.update({key: value}))
                with self.assertRaises(ValueError):
                    self.verify(path)
        path = self.write_archive(change=lambda files, info: info["settings"].update(overflow_checks=False))
        with self.assertRaises(ValueError):
            self.verify(path)
        path = self.write_archive(change=lambda files, info: files.update({"unexpected.txt": b"extra"}))
        with self.assertRaises(ValueError):
            self.verify(path)
        path = self.write_archive(mode=0o644)
        with self.assertRaises(ValueError):
            self.verify(path)

    def test_archive_content_hash_detects_tampering(self):
        path = self.write_archive()
        with zipfile.ZipFile(path) as original:
            members = [(member, original.read(member)) for member in original.infolist()]
        with zipfile.ZipFile(path, "w") as output:
            for member, data in members:
                output.writestr(member, data + b"modified" if member.filename.endswith("/USAGE.txt") else data)
        with self.assertRaises(ValueError):
            self.verify(path)

    def test_missing_licensing_and_template_rejections(self):
        for name in ("third-party/RUST-COPYRIGHT-library.html", "third-party/libm-0.2.16/src/math/sin.rs", "LICENSE"):
            with self.subTest(name=name):
                path = self.write_archive(change=lambda files, info: files.pop(name))
                with self.assertRaises(ValueError):
                    self.verify(path)
        path = self.write_archive(change=lambda files, info: files.update({"INSTALL.txt": b"{{VERSION}}"}))
        with self.assertRaises(ValueError):
            self.verify(path)
        path = self.write_archive(WINDOWS, change=lambda files, info: files.pop("third-party/MICROSOFT-RUNTIME.txt"))
        with self.assertRaises(ValueError):
            self.verify(path, WINDOWS)

    def test_zip_paths_duplicates_links_and_empty_directory_are_rejected(self):
        cases = ["../outside", "/outside", "ROOT/../outside", "C:/outside", "ROOT\\outside", "ROOT//outside", "ROOT/./outside", "ROOT/empty/"]
        for name in cases:
            with self.subTest(name=name):
                path = self.write_archive(additional=lambda output, entry: output.writestr(name.replace("ROOT", entry["root"]), b""))
                with self.assertRaises(ValueError):
                    self.verify(path)
        def linked(output, entry):
            member = zipfile.ZipInfo(entry["root"] + "/third-party/link")
            member.create_system = 3
            member.external_attr = (stat.S_IFLNK | 0o777) << 16
            output.writestr(member, "../../outside")
        with self.assertRaises(ValueError):
            self.verify(self.write_archive(additional=linked))
        def duplicate(output, entry):
            output.writestr(entry["root"] + "/LICENSE", "duplicate")
        with patch("warnings.warn"):
            path = self.write_archive(additional=duplicate)
        with self.assertRaises(ValueError):
            self.verify(path)

    def test_tar_links_and_traversal_are_rejected(self):
        for name, kind in (("../outside", tarfile.REGTYPE), ("ROOT/third-party/link", tarfile.SYMTYPE), ("ROOT/third-party/hardlink", tarfile.LNKTYPE)):
            def add(output, entry):
                member = tarfile.TarInfo(name.replace("ROOT", entry["root"]))
                member.type = kind
                member.linkname = "../../outside"
                output.addfile(member)
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.verify(self.write_archive(LINUX, additional=add), LINUX)

    def test_wrong_filename_source_and_output_location_are_rejected(self):
        path = self.write_archive()
        moved = path.with_name("another.zip")
        path.rename(moved)
        with self.assertRaises(ValueError):
            self.verify(moved)
        with self.assertRaises(ValueError):
            archive.verify_archive(moved, MAC, VERSION, "short", REPOSITORY)
        with self.assertRaises(ValueError):
            archive.owned_path(archive.ROOT / "scripts" / "output")

    def test_dependency_closure_keeps_build_and_aliased_edges_but_excludes_dev(self):
        packages = [{"id": name, "name": "auditory-lab" if name == "cli" else name, "version": "1.0.0", "source": None if name == "cli" else "registry+https://github.com/rust-lang/crates.io-index"} for name in ("cli", "normal", "build", "dev", "transitive")]
        def edge(name, kind):
            return {"name": "alias", "pkg": name, "dep_kinds": [{"kind": kind, "target": "cfg(windows)"}]}
        metadata = {"workspace_members": ["cli"], "packages": packages, "resolve": {"nodes": [{"id": "cli", "deps": [edge("normal", None), edge("build", "build"), edge("dev", "dev")]}, {"id": "normal", "deps": [edge("transitive", None)]}, {"id": "build", "deps": []}, {"id": "dev", "deps": []}, {"id": "transitive", "deps": []}]}}
        self.assertEqual([package["name"] for package in archive.dependency_closure(metadata)], ["build", "normal", "transitive"])
        empty = copy.deepcopy(metadata)
        empty["resolve"]["nodes"][0]["deps"] = [edge("dev", "dev")]
        with self.assertRaises(ValueError):
            archive.dependency_closure(empty)

    def test_build_flags_reject_cpu_native_and_disabled_checks(self):
        keys = ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS", "MACOSX_DEPLOYMENT_TARGET", "CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS")
        environment = {key: value for key, value in os.environ.items() if key not in keys}
        environment["MACOSX_DEPLOYMENT_TARGET"] = "15.0"
        with patch.dict(os.environ, environment, clear=True):
            archive.check_build_flags(archive.target_entry(MAC, VERSION))
            for minimum in ("14.2", "15.1"):
                os.environ["MACOSX_DEPLOYMENT_TARGET"] = minimum
                with self.subTest(minimum=minimum), self.assertRaises(ValueError):
                    archive.check_build_flags(archive.target_entry(MAC, VERSION))
            os.environ["MACOSX_DEPLOYMENT_TARGET"] = "15.0"
            os.environ["RUSTFLAGS"] = "-C target-cpu=native"
            with self.assertRaises(ValueError):
                archive.check_build_flags(archive.target_entry(MAC, VERSION))
            os.environ.pop("RUSTFLAGS")
            os.environ["CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS"] = "false"
            with self.assertRaises(ValueError):
                archive.check_build_flags(archive.target_entry(MAC, VERSION))

    def test_complete_wav_oracle_rejects_sizes_frame_counts_and_samples(self):
        # Independent fixed fixture, rather than invoking the Rust WAV writer.
        good = bytes.fromhex("524946463a00000057415645666d7420120000000300020080bb000000dc050008002000000066616374040000000100000064617461080000000000000000000000")
        self.assertEqual(len(good), 66)
        archive.validate_silent_wav(good)
        for position in (0, 4, 16, 20, 22, 24, 28, 32, 34, 36, 38, 42, 46, 50, 54, 58, 65):
            bad = bytearray(good)
            bad[position] ^= 1
            with self.subTest(position=position), self.assertRaises(ValueError):
                archive.validate_silent_wav(bytes(bad))
        for bad in (good[:-1], good + b"\0"):
            with self.assertRaises(ValueError):
                archive.validate_silent_wav(bad)

    def test_windows_runtime_identity_requires_selected_native_retail_evidence(self):
        vs = self.directory / "VisualStudio"
        tools = vs / "VC" / "Tools" / "MSVC" / "14.44.1"
        sdk = self.directory / "WindowsSDK"
        linker = tools / "bin" / "Hostx64" / "x64" / "link.exe"
        libraries = [tools / "lib" / "x64" / "libcmt.lib", tools / "lib" / "x64" / "libvcruntime.lib", sdk / "Lib" / "10.0.1.0" / "ucrt" / "x64" / "libucrt.lib"]
        for path in [linker, *libraries]:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"native retail control")
        identity = self.directory / "visual-studio.json"
        selected = {"installationPath": str(vs), "installationVersion": "17.14.1.0", "productId": "Microsoft.VisualStudio.Product.Enterprise", "isPrerelease": False}
        identity.write_text(json.dumps(selected))
        environment = {"VSINSTALLDIR": str(vs), "VCToolsInstallDir": str(tools), "VCToolsVersion": "14.44.1", "WindowsSdkDir": str(sdk), "WindowsSDKVersion": "10.0.1.0", "AUDITORY_LAB_VS_INFO": str(identity), "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER": str(linker), "LIB": os.pathsep.join(str(path.parent) for path in libraries)}
        with patch.dict(os.environ, environment, clear=True):
            info = archive.windows_runtime(WINDOWS)
            self.assertEqual(info["toolset_version"], "14.44.1")
            self.assertEqual(len(info["libraries"]), 3)
            for change in ({"isPrerelease": True}, {"installationVersion": "19.0.1.0"}, {"installationPath": str(sdk)}, {"productId": "Microsoft.VisualStudio.Product.Community"}, {"productId": "Microsoft.VisualStudio.Product.BuildTools"}):
                identity.write_text(json.dumps(dict(selected, **change)))
                with self.subTest(change=change), self.assertRaises(ValueError):
                    archive.windows_runtime(WINDOWS)
            identity.write_text(json.dumps(selected))
            os.environ["CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"] = str(tools / "bin" / "HostARM64" / "x64" / "link.exe")
            with self.assertRaises(ValueError):
                archive.windows_runtime(WINDOWS)
            os.environ["CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"] = str(linker)
            alternate = self.directory / "other-toolset"
            alternate.mkdir()
            (alternate / "libcmt.lib").write_bytes(b"other library selected first")
            os.environ["LIB"] = str(alternate) + os.pathsep + environment["LIB"]
            with self.assertRaises(ValueError):
                archive.windows_runtime(WINDOWS)
            os.environ["LIB"] = environment["LIB"]
            libraries[0].unlink()
            with self.assertRaises(ValueError):
                archive.windows_runtime(WINDOWS)

    def test_existing_and_dangling_archive_links_are_not_followed(self):
        for target in (MAC, WINDOWS, LINUX):
            with self.subTest(target=target):
                output = self.directory / target
                output.mkdir()
                entry = archive.target_entry(target, VERSION)
                path = output / entry["archive"]
                referent = self.directory / (target + "-referent")
                try:
                    path.symlink_to(referent)
                except OSError as error:
                    self.skipTest(f"This host cannot create the required symlink control: {error}")
                host = {"compiler": {"host": target, "release": archive.pinned_channel()}}
                with patch.object(archive, "assert_host", return_value=host), patch.object(archive.shutil, "copyfile") as copy_file:
                    with self.assertRaisesRegex(ValueError, "symbolic link"):
                        archive.package(target, VERSION, SOURCE, REPOSITORY, self.directory / "missing-binary", output, self.directory / (target + "-scratch"))
                    copy_file.assert_not_called()
                self.assertFalse(referent.exists())

    def test_runtime_licenses_are_complete_reviewed_source_materials(self):
        for terms in archive.MICROSOFT_TERMS.values():
            content = (archive.ROOT / "scripts" / "licenses" / terms["license_file"]).read_bytes()
            self.assertEqual(archive.digest(content), terms["text_sha256"])
            self.assertIn(b"MICROSOFT SOFTWARE LICENSE TERMS", content)
            self.assertIn(b"IF YOU COMPLY", content)
        def alter_terms(files, info):
            name = "third-party/" + archive.MICROSOFT_TERMS[17]["license_file"]
            files[name] += b"altered terms"
        with self.assertRaisesRegex(ValueError, "runtime terms"):
            self.verify(self.write_archive(WINDOWS, change=alter_terms), WINDOWS)

    def test_lowercase_and_mixed_case_published_license_files_are_discovered(self):
        directory = self.directory / "published-crate"
        directory.mkdir()
        names = ("license-mit", "license-apache-2.0", "Notice.txt", "copyright", "README.md")
        for name in names:
            (directory / name).write_text(name)
        self.assertEqual({path.name for path in archive.license_files(directory)}, set(names[:-1]))

    def test_missing_material_requires_exact_reviewed_upstream_identity_and_bytes(self):
        directory = self.directory / "published-objc2"
        directory.mkdir()
        expected = archive.upstream_supplements()["block2-0.6.2"]
        package = {"name": "block2", "version": "0.6.2", "repository": expected["repository"], "license": expected["license"]}
        vcs = directory / ".cargo_vcs_info.json"
        vcs.write_text(json.dumps(expected["vcs"]))
        materials, provenance = archive.supplemental_materials(package, directory)
        self.assertEqual(provenance, expected)
        self.assertEqual({path.name for _, path in materials}, {"LICENSE.md", "MIT-standard.txt"})
        for change in ({"version": "0.6.3"}, {"repository": "https://github.com/other/objc2"}, {"license": "Apache-2.0"}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                archive.supplemental_materials(dict(package, **change), directory)
        for change in ({"git": {"sha1": "a" * 40}, "path_in_vcs": "crates/block2"}, {"git": expected["vcs"]["git"], "path_in_vcs": "crates/another"}):
            vcs.write_text(json.dumps(change))
            with self.subTest(vcs=change), self.assertRaises(ValueError):
                archive.supplemental_materials(package, directory)
        vcs.write_text(json.dumps(expected["vcs"]))
        altered = copy.deepcopy(archive.upstream_supplements())
        altered["block2-0.6.2"]["files"][0]["sha256"] = "0" * 64
        with patch.object(archive, "upstream_supplements", return_value=altered), self.assertRaises(ValueError):
            archive.supplemental_materials(package, directory)

    def test_upstream_source_files_preserve_exact_declared_grants_and_copyrights(self):
        supplements = archive.upstream_supplements()
        self.assertEqual(len(supplements), 10)
        for package, record in supplements.items():
            self.assertEqual(record["selected_license"], "MIT")
            self.assertRegex(record["vcs"]["git"]["sha1"], r"^[0-9a-f]{40}$")
            for item in record["files"]:
                content = (archive.ROOT / "scripts" / "licenses" / item["source_file"]).read_bytes()
                self.assertEqual(archive.digest(content), item["sha256"], (package, item["source_file"]))
                self.assertTrue(item["url"].startswith("https://raw.githubusercontent.com/"))
        source = (archive.ROOT / "scripts" / "licenses" / "objc2-LICENSE.md").read_text()
        self.assertIn("Apple SDKs", source)
        self.assertIn("https://www.apple.com/legal/sla/docs/xcode.pdf", source)
        standard = (archive.ROOT / "scripts" / "licenses" / "MIT-standard.txt").read_text()
        self.assertIn("Copyright (c) <year> <copyright holders>", standard)
        sample = (archive.ROOT / "scripts" / "licenses" / "dasp-sample-LICENSE-MIT").read_text()
        self.assertIn("Copyright (c) 2016 RustAudio Developers", sample)

    def test_archive_rejects_changed_supplement_provenance_and_material(self):
        def include_supplement(files, info):
            key = "block2-0.6.2"
            expected = archive.upstream_supplements()[key]
            names = []
            for item in expected["files"]:
                name = f"third-party/{key}/{item['archive_file']}"
                files[name] = (archive.ROOT / "scripts" / "licenses" / item["source_file"]).read_bytes()
                names.append(name)
            index = json.loads(files["third-party/index.json"])
            index["packages"].append({"name": "block2", "version": "0.6.2", "license": expected["license"], "repository": expected["repository"], "files": names, "upstream_supplement": expected})
            files["third-party/index.json"] = json.dumps(index).encode()
        self.verify(self.write_archive(change=include_supplement))
        def changed_material(files, info):
            include_supplement(files, info)
            files["third-party/block2-0.6.2/LICENSE.md"] += b"altered grant"
        with self.assertRaisesRegex(ValueError, "upstream license material"):
            self.verify(self.write_archive(change=changed_material))
        def changed_provenance(files, info):
            include_supplement(files, info)
            index = json.loads(files["third-party/index.json"])
            index["packages"][-1]["upstream_supplement"]["vcs"]["git"]["sha1"] = "0" * 40
            files["third-party/index.json"] = json.dumps(index).encode()
        with self.assertRaisesRegex(ValueError, "upstream license supplement"):
            self.verify(self.write_archive(change=changed_provenance))
        def omitted_marker_and_changed_material(files, info):
            include_supplement(files, info)
            index = json.loads(files["third-party/index.json"])
            index["packages"][-1].pop("upstream_supplement")
            files["third-party/index.json"] = json.dumps(index).encode()
            files["third-party/block2-0.6.2/LICENSE.md"] += b"altered grant without provenance marker"
        with self.assertRaisesRegex(ValueError, "upstream license supplement"):
            self.verify(self.write_archive(change=omitted_marker_and_changed_material))
        def unknown_marker(files, info):
            index = json.loads(files["third-party/index.json"])
            index["packages"][0]["upstream_supplement"] = archive.upstream_supplements()["block2-0.6.2"]
            files["third-party/index.json"] = json.dumps(index).encode()
        with self.assertRaisesRegex(ValueError, "upstream license supplement"):
            self.verify(self.write_archive(change=unknown_marker))


if __name__ == "__main__":
    unittest.main()
