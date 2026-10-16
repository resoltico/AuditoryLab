# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""The packager's PE import reader and archive reader, against independent fixtures."""
import io
import json
from pathlib import Path
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from package import source_reference, expected_members, bundle_metadata, validate_bundle, validate_catalog_listing, ROOT, run, members, pe_imports, check_dependencies, check_identity, check_macos_minimum, check_macos_native, package
from release import MACOS_MINIMUM, archive_name, expected_identity


def minimal_pe(dlls):
    """A PE32+ image with one section holding an import directory for `dlls`."""
    data = bytearray(0x400)
    struct.pack_into("<2sI", data, 0, b"MZ", 0)
    struct.pack_into("<I", data, 0x3C, 0x40)
    struct.pack_into("<4sHHIIIHH", data, 0x40, b"PE\0\0", 0x8664, 1, 0, 0, 0, 240, 0x22)
    optional = 0x40 + 24
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<I", data, optional + 108, 16)
    struct.pack_into("<II", data, optional + 120, 0x1000, 20 * (len(dlls) + 1))
    section = optional + 240
    struct.pack_into("<8sIIII", data, section, b".idata\0\0", 0x200, 0x1000, 0x200, 0x200)
    name_rva = 0x1100
    for index, dll in enumerate(dlls):
        struct.pack_into("<5I", data, 0x200 + 20 * index, 0, 0, 0, name_rva, 0)
        offset = 0x200 + name_rva - 0x1000
        data[offset:offset + len(dll) + 1] = dll.encode("ascii") + b"\0"
        name_rva += len(dll) + 1
    return bytes(data)


class PeTests(unittest.TestCase):
    def test_import_names_are_read_lowercased_in_order(self):
        image = minimal_pe(["KERNEL32.dll", "VCRUNTIME140.dll"])
        self.assertEqual(pe_imports(image), ["kernel32.dll", "vcruntime140.dll"])

    def test_pe_identity_and_delay_import_rejections(self):
        image = minimal_pe(["kernel32.dll"])
        for offset, value in [(0, b"NO"), (0x40, b"XX"), (0x44, b"\x64\xaa"), (0x56, b"\x22\x20"), (0x58, b"\x0b\x01"), (0x58 + 216, b"\x01\x00")]:
            changed = bytearray(image)
            changed[offset:offset + len(value)] = value
            with self.assertRaises(ValueError):
                pe_imports(changed, "x86_64-pc-windows-gnullvm")

    def test_windows_policy_rejects_private_and_external_crt_imports(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "tool.exe"
            executable.write_bytes(minimal_pe(["KERNEL32.dll", "ucrtbase.dll", "api-ms-win-crt-private-l1-1-0.dll", "api-ms-win-core-file-l1-1-0.dll"]))
            check_dependencies(executable, "x86_64-pc-windows-gnullvm")
            for dll in ["runner-private.dll", "msvcr120.dll", "concrt140.dll", "vcomp140.dll", "vcruntime140.dll", "msvcp140.dll", "msvcrt.dll", "libunwind.dll", "libunwind-1.dll", "api-ms-win-crt-unreviewed-l1-1-0.dll"]:
                executable.write_bytes(minimal_pe(["kernel32.dll", dll]))
                with self.assertRaises(ValueError, msg=dll):
                    check_dependencies(executable, "x86_64-pc-windows-gnullvm")
            executable.write_bytes(minimal_pe(["ntdll.dll"]))
            with self.assertRaises(ValueError):
                check_dependencies(executable, "x86_64-pc-windows-gnullvm")

    def test_an_image_without_imports_yields_none(self):
        self.assertEqual(pe_imports(minimal_pe([])), [])


class MemberTests(unittest.TestCase):
    def test_zip_and_tar_members_report_modes_and_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            zipped = Path(directory) / "a.zip"
            with zipfile.ZipFile(zipped, "w") as archive:
                info = zipfile.ZipInfo("root/tool")
                info.external_attr = (stat.S_IFREG | 0o755) << 16
                archive.writestr(info, b"x")
            self.assertEqual(members(zipped), {"root/tool": (stat.S_IFREG | 0o755, b"x")})
            tarred = Path(directory) / "a.tar.gz"
            with tarfile.open(tarred, "w:gz") as archive:
                info = tarfile.TarInfo("root/README.txt")
                info.size, info.mode = 2, 0o644
                archive.addfile(info, io.BytesIO(b"hi"))
            self.assertEqual(members(tarred), {"root/README.txt": (stat.S_IFREG | 0o644, b"hi")})


class NativeGuardTests(unittest.TestCase):
    def test_contextual_macos_minimum(self):
        valid = ["Load command 1\n cmd LC_VERSION_MIN_MACOSX\n version 10.12\n sdk 26.0\n", "Load command 1\n cmd LC_BUILD_VERSION\n platform 1\n minos 11.0\n sdk 26.0\n"]
        for text, minimum in zip(valid, ["10.12", "11.0"]):
            with patch("package.run", return_value=text):
                check_macos_minimum(Path("tool"), minimum)
        for text in ["minos 11.0\n", valid[1].replace("platform 1", "platform 2"), valid[1] + valid[1], valid[0]]:
            with patch("package.run", return_value=text), self.assertRaises(ValueError):
                check_macos_minimum(Path("tool"), "11.0")

    def test_native_cpu_and_type_guards(self):
        with patch("package.run", return_value="MH_MAGIC_64 ARM64 ALL 0x00 EXECUTE 23 2616 FLAGS"):
            check_identity(Path("tool"), "aarch64-apple-darwin")
            with self.assertRaises(ValueError):
                check_identity(Path("tool"), "x86_64-apple-darwin")
        for text in ["MH_MAGIC_64 ARM64 ALL 0x00 DYLIB 23 2616 FLAGS", "MH_MAGIC ARM64 ALL 0x00 EXECUTE 23 2616 FLAGS"]:
            with patch("package.run", return_value=text), self.assertRaises(ValueError):
                check_identity(Path("tool"), "aarch64-apple-darwin")

    def test_elf_identity_requires_the_target_system_loader(self):
        for architecture, machine, loader in [
            ("aarch64", "AArch64", "/lib/ld-linux-aarch64.so.1"),
            ("x86_64", "Advanced Micro Devices X86-64", "/lib64/ld-linux-x86-64.so.2"),
        ]:
            target = architecture + "-unknown-linux-gnu"
            header = f" Class: ELF64\n Type: DYN (Position-Independent Executable file)\n Machine: {machine}\n"
            segments = "  INTERP 0x000238 0x238 0x238 0x1c 0x1c R 0x1\n" + f"      [Requesting program interpreter: {loader}]\n"
            with self.subTest(target=target), patch("package.run", side_effect=[header, segments]) as reader:
                check_identity(Path("tool"), target)
                self.assertEqual(reader.call_args_list[-1].args, ("readelf", "-lW", Path("tool")))
            other_loader = "/lib64/ld-linux-x86-64.so.2" if architecture == "aarch64" else "/lib/ld-linux-aarch64.so.1"
            for rejected in [
                segments.replace(loader, "/runner/private/ld.so"),
                segments.replace(loader, other_loader),
                "  LOAD 0x000238\n",
                "  INTERP 0x000238\n",
                segments + segments,
                segments + f"      [Requesting program interpreter: {loader}]\n",
                segments + "  INTERP 0x000238\n",
            ]:
                with self.subTest(target=target, segments=rejected), patch("package.run", side_effect=[header, rejected]), self.assertRaisesRegex(ValueError, "system loader"):
                    check_identity(Path("tool"), target)
            for changed in [header.replace(machine, "Other"), header.replace("ELF64", "ELF32"), header.replace("DYN", "REL")]:
                with self.subTest(target=target, header=changed), patch("package.run", side_effect=[changed, segments]), self.assertRaisesRegex(ValueError, "CPU/type"):
                    check_identity(Path("tool"), target)

    def test_linux_runtime_floor_and_paths(self):
        dynamic = " (NEEDED) Shared library: [libc.so.6]\n"
        with patch("package.run", side_effect=[dynamic, "Name: GLIBC_2.35 Name: GLIBC_2.2.5"]):
            check_dependencies(Path("tool"), "x86_64-unknown-linux-gnu")
        for libraries, versions in [(dynamic, "GLIBC_2.36"), (dynamic, ""), (dynamic + " (RUNPATH) [/private]", "GLIBC_2.35"), (dynamic + " (RPATH) [/private]", "GLIBC_2.35"), (dynamic + " (NEEDED) [libprivate.so]", "GLIBC_2.35"), ("", "GLIBC_2.35")]:
            with patch("package.run", side_effect=[libraries, versions]), self.assertRaises(ValueError):
                check_dependencies(Path("tool"), "x86_64-unknown-linux-gnu")

    def test_linux_gui_requires_system_font_libraries_without_expanding_cli(self):
        dynamic = " (NEEDED) [libc.so.6]\n (NEEDED) [libfreetype.so.6]\n (NEEDED) [libfontconfig.so.1]\n"
        with patch("package.run", side_effect=[dynamic, "GLIBC_2.35"]):
            check_dependencies(Path("gui"), "x86_64-unknown-linux-gnu", "gui")
        for changed in (dynamic.replace("libfreetype.so.6", "libother.so"), dynamic.replace("libfontconfig.so.1", "libother.so")):
            with patch("package.run", side_effect=[changed, "GLIBC_2.35"]), self.assertRaises(ValueError):
                check_dependencies(Path("gui"), "x86_64-unknown-linux-gnu", "gui")
        with patch("package.run", side_effect=[dynamic, "GLIBC_2.35"]), self.assertRaises(ValueError):
            check_dependencies(Path("cli"), "x86_64-unknown-linux-gnu", "cli")

    def test_emulated_compiler_host_does_not_establish_native_machine(self):
        with patch("package.run", return_value="host: x86_64-apple-darwin\n"), patch("package.platform.system", return_value="Darwin"), patch("package.platform.machine", return_value="arm64"), self.assertRaisesRegex(ValueError, "native machine"):
            package("x86_64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path("unused"))
        with patch("package.run", return_value="host: x86_64-apple-darwin\n"), patch("package.platform.system", return_value="Linux"), patch("package.platform.machine", return_value="x86_64"), self.assertRaisesRegex(ValueError, "operating system"):
            package("x86_64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path("unused"))

    def test_current_macos_artifact_policy_requires_exact_common_floor(self):
        self.assertEqual(MACOS_MINIMUM, "15.0")
        for floor in ("15.0", "10.12", "11.0", "14.0", "16.0"):
            text = f"Load command 1\n cmd LC_BUILD_VERSION\n platform 1\n minos {floor}\n sdk 27.0\n"
            with self.subTest(floor=floor), patch("package.run", return_value=text):
                if floor == MACOS_MINIMUM:
                    check_macos_minimum(Path("tool"), MACOS_MINIMUM)
                else:
                    with self.assertRaises(ValueError):
                        check_macos_minimum(Path("tool"), MACOS_MINIMUM)

    def test_package_rejects_old_macos_binary_for_both_products_and_architectures(self):
        for product in ('cli', 'gui'):
            for architecture in ('aarch64', 'x86_64'):
                target = architecture + '-apple-darwin'
                def native_output(*command, **kwargs):
                    if command == ('rustc', '-vV'):
                        return 'host: ' + target + '\n'
                    if len(command) == 2 and command[1] == '--build-info':
                        return json.dumps(expected_identity(product, '0.2.0', target, 'local', None))
                    if command[:2] == ('codesign', '--force'):
                        return ''
                    if command[:2] == ('otool', '-l'):
                        return 'Load command 1\n cmd LC_BUILD_VERSION\n platform 1\n minos 14.0\n sdk 27.0\n'
                    raise AssertionError('unexpected package command: ' + repr(command))
                with self.subTest(product=product, target=target), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    binary = root / 'owned-binary'
                    binary.write_bytes(b'owned native-policy fixture')
                    output = root / 'archives'
                    with patch('package.run', side_effect=native_output), \
                         patch('package.platform.system', return_value='Darwin'), \
                         patch('package.platform.machine', return_value=architecture), \
                         patch('package.check_macos_native') as native, \
                         patch('package.cargo_identity', return_value={'version': '0.2.0'}), \
                         patch('package.check_identity') as cpu, patch('package.check_dependencies') as imports, \
                         self.assertRaisesRegex(ValueError, 'not 15.0'):
                        package(target, '0.2.0', 'owner/repo', binary, output, product=product)
                    native.assert_called_once()
                    cpu.assert_called_once()
                    imports.assert_called_once()
                    self.assertEqual(list(output.iterdir()), [])

    def test_macos_translation_is_rejected_even_when_reported_architectures_match(self):
        with patch("package.run", side_effect=["host: x86_64-apple-darwin\n", "1\n"]), patch("package.platform.system", return_value="Darwin"), patch("package.platform.machine", return_value="x86_64"), self.assertRaisesRegex(ValueError, "Rosetta"):
            package("x86_64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path("unused"))

    def test_macos_native_and_missing_translation_key_are_accepted(self):
        for value in ["0\n", ""]:
            with patch("package.run", return_value=value) as query:
                check_macos_native()
                query.assert_called_once_with("/usr/sbin/sysctl", "-in", "sysctl.proc_translated")

    def test_windows_package_requires_reviewed_target_and_bound_link_receipt(self):
        with self.assertRaisesRegex(ValueError, "unsupported release target"):
            package("x86_64-pc-windows-msvc", "0.1.0", "owner/repo", Path("unused"), Path("unused"))
        with self.assertRaisesRegex(ValueError, "--link-trace"):
            package("x86_64-pc-windows-gnullvm", "0.1.0", "owner/repo", Path("unused"), Path("unused"))

    def test_dangling_destination_symlink_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / archive_name("macos-aarch64", "0.1.0", build_kind="local")
            destination.symlink_to("missing.zip")
            with patch("package.run", side_effect=["host: aarch64-apple-darwin\n", "0\n"]), patch("package.platform.system", return_value="Darwin"), patch("package.platform.machine", return_value="arm64"), self.assertRaisesRegex(ValueError, "already exists"):
                package("aarch64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path(directory))
            self.assertTrue(destination.is_symlink())
            self.assertEqual(str(destination.readlink()), "missing.zip")


class MemberRejectionTests(unittest.TestCase):
    def test_duplicate_and_nonregular_archive_records(self):
        with tempfile.TemporaryDirectory() as directory:
            for duplicate in [False, True]:
                zipped = Path(directory) / "bad.zip"
                with zipfile.ZipFile(zipped, "w") as archive, warnings.catch_warnings():
                    warnings.simplefilter("ignore", UserWarning)
                    info = zipfile.ZipInfo("root/tool")
                    info.external_attr = ((stat.S_IFREG if duplicate else stat.S_IFLNK) | 0o755) << 16
                    archive.writestr(info, b"x")
                    if duplicate:
                        archive.writestr(info, b"x")
                with self.assertRaises(ValueError):
                    members(zipped)
                tarred = Path(directory) / "bad.tar.gz"
                with tarfile.open(tarred, "w:gz") as archive:
                    info = tarfile.TarInfo("root/tool")
                    if duplicate:
                        info.size = 1
                        archive.addfile(info, io.BytesIO(b"x"))
                        archive.addfile(info, io.BytesIO(b"x"))
                    else:
                        info.type, info.linkname = tarfile.SYMTYPE, "other"
                        archive.addfile(info)
                with self.assertRaises(ValueError):
                    members(tarred)


class CatalogListingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.listing = (Path(__file__).parent / "fixtures/catalog-listing.txt").read_text(encoding="utf-8")
        cls.catalog = list((ROOT / "crates/auditory-core/catalog").glob("*.toml"))

    def test_real_catalog_fixture_has_all_nineteen_records(self):
        self.assertEqual(len(self.catalog), 19)
        validate_catalog_listing(self.listing, self.catalog)

    def test_sound_continuations_are_allowed_without_becoming_catalog_ids(self):
        wrapped = self.listing.replace("Sound: 40 Hz train of 1 ms 10 kHz tones",
                                       "Sound: 40 Hz train of 1 ms\n         10 kHz tones")
        validate_catalog_listing(wrapped, self.catalog)

    def test_empty_missing_extra_duplicate_reordered_and_truncated_ids_reject(self):
        lines = self.listing.splitlines(keepends=True)
        cases = {
            "empty": "",
            "header-only": lines[0],
            "missing": "".join(lines[:1] + lines[3:]),
            "extra": self.listing + "other-id 1 min\n  Sound: Other\n",
            "duplicate": self.listing + "".join(lines[1:3]),
            "reordered": "".join(lines[:1] + lines[3:5] + lines[1:3] + lines[5:]),
            "truncated": self.listing.replace("beauchene-2016-binaural-5-hz", "beauchene-2016-binaural"),
        }
        for label, text in cases.items():
            with self.subTest(label=label), self.assertRaises(ValueError):
                validate_catalog_listing(text, self.catalog)

    def test_header_and_sound_structure_confusion_rejects(self):
        lines = self.listing.splitlines(keepends=True)
        cases = {
            "no-header": "".join(lines[1:]),
            "second-header": self.listing + lines[0],
            "wrong-header": self.listing.replace("PRESET", "PRESETS", 1),
            "indented-header": "  " + self.listing,
            "header-as-sound": self.listing.replace("  Sound: 5 Hz binaural beat", "  PRESET DURATION", 1),
            "orphan-continuation": lines[0] + "  Sound: Orphan\n" + "".join(lines[1:]),
            "missing-sound": "".join(lines[:2] + lines[3:]),
            "empty-sound": self.listing.replace("  Sound: 5 Hz binaural beat", "  Sound: ", 1),
            "duplicate-sound": self.listing.replace(lines[2], lines[2] * 2, 1),
            "unindented-sound": self.listing.replace("  Sound:", "Sound:", 1),
            "blank-record": self.listing + "\n",
        }
        for label, text in cases.items():
            with self.subTest(label=label), self.assertRaises(ValueError):
                validate_catalog_listing(text, self.catalog)

    def test_incorrect_duration_and_sound_reject(self):
        for before, after in (("3 min", "180 min"), ("3 min", "three minutes"),
                              ("5 min 7.2 s", "5 min 7.3 s"), ("5 min 7.2 s", "5 min 5 min"),
                              ("5 Hz binaural beat", "Different sound")):
            with self.subTest(after=after), self.assertRaises(ValueError):
                validate_catalog_listing(self.listing.replace(before, after, 1), self.catalog)

    def test_empty_source_catalog_cannot_pass(self):
        with self.assertRaisesRegex(ValueError, "source catalog is empty"):
            validate_catalog_listing("PRESET DURATION\n", [])


class ProductPackageTests(unittest.TestCase):
    def test_readme_source_reference_uses_qualified_sha_and_no_local_fake_ref(self):
        local = source_reference('owner/repo', 'local', None)
        self.assertIn("builder's", local)
        self.assertIn('unqualified', local)
        self.assertNotIn('/tree/', local)
        for kind in ('candidate', 'release'):
            qualified = source_reference('owner/repo', kind, 'a' * 40)
            self.assertIn('/tree/' + 'a' * 40 + '#readme', qualified)
            self.assertNotIn('/tree/v0.2.0', qualified)
        with self.assertRaises(ValueError):
            source_reference('owner/repo', 'local', 'a' * 40)
        for product in ('archive-readme.txt', 'archive-readme-gui.txt'):
            template = (ROOT / 'scripts' / product).read_text()
            rendered = template.format(version='0.2.0', repository='owner/repo',
                                       macos_minimum=MACOS_MINIMUM, source_reference=local)
            self.assertIn('MPL-2.0' if product.endswith('gui.txt') else 'Mozilla Public License', rendered)
            self.assertIn(local, rendered)
            self.assertIn('macOS 15.0 or later', rendered)
            self.assertNotIn('/tree/v0.2.0', rendered)

    def test_gui_bundle_metadata_and_members_are_separate_from_cli(self):
        import plistlib
        target = "aarch64-apple-darwin"
        self.assertEqual(len(expected_members("cli", target)), 4)
        self.assertIn("Auditory Lab.app/Contents/MacOS/auditorylab-gui", expected_members("gui", target))
        correct = bundle_metadata("0.2.0")
        validate_bundle(plistlib.dumps(correct), "0.2.0")
        self.assertEqual(correct["LSMinimumSystemVersion"], "15.0")
        for old_floor in ("10.12", "11.0", "14.0", "16.0"):
            with self.subTest(floor=old_floor), self.assertRaises(ValueError):
                validate_bundle(plistlib.dumps(dict(correct, LSMinimumSystemVersion=old_floor)), "0.2.0")
        for key, value in [("LSMinimumSystemVersion", "11.0"), ("CFBundleExecutable", "auditorylab"),
                           ("CFBundlePackageType", "BNDL"), ("CFBundleShortVersionString", "0.1.0")]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_bundle(plistlib.dumps(dict(correct, **{key: value})), "0.2.0")

    def test_windows_gui_subsystem_and_import_additions_do_not_change_cli(self):
        with tempfile.TemporaryDirectory() as directory:
            executable = Path(directory) / "gui.exe"
            image = bytearray(minimal_pe(["kernel32.dll", "gdi32.dll"]))
            struct.pack_into("<H", image, 0x40 + 24 + 68, 2)
            executable.write_bytes(image)
            check_identity(executable, "x86_64-pc-windows-gnullvm", "gui")
            check_dependencies(executable, "x86_64-pc-windows-gnullvm", "gui")
            with self.assertRaisesRegex(ValueError, "unexpected runtime"):
                check_dependencies(executable, "x86_64-pc-windows-gnullvm", "cli")
            executable.write_bytes(minimal_pe(["kernel32.dll", "opengl32.dll"]))
            check_dependencies(executable, "x86_64-pc-windows-gnullvm", "gui")
            with self.assertRaisesRegex(ValueError, "unexpected runtime"):
                check_dependencies(executable, "x86_64-pc-windows-gnullvm", "cli")
            struct.pack_into("<H", image, 0x40 + 24 + 68, 3)
            executable.write_bytes(image)
            with self.assertRaisesRegex(ValueError, "GUI subsystem"):
                check_identity(executable, "x86_64-pc-windows-gnullvm", "gui")

    def test_gui_exact_framework_policy_cannot_loosen_cli_import_gate(self):
        output = "binary:\n    /usr/lib/libSystem.B.dylib (compatibility version 1)\n    /System/Library/Frameworks/Unreviewed.framework/Versions/A/Unreviewed (compatibility version 1)\n"
        with patch("package.run", side_effect=["", output]), self.assertRaisesRegex(ValueError, "unexpected runtime"):
            check_dependencies(Path("gui"), "aarch64-apple-darwin", "gui")


class DiagnosticTests(unittest.TestCase):
    def test_raw_recipe_boundary_does_not_normalize_line_endings(self):
        value = run(sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'raw\\r\\nsource\\r\\n')", text=False)
        self.assertEqual(value, b"raw\r\nsource\r\n")

    def test_failure_keeps_command_status_stdout_and_stderr(self):
        with self.assertRaises(ValueError) as caught:
            run(sys.executable, "-c", "import sys; print('tool output'); print('specific cause', file=sys.stderr); sys.exit(7)", encoding="utf-8")
        self.assertIn("tool output", str(caught.exception))
        self.assertIn("specific cause", str(caught.exception))
        self.assertIn("7", str(caught.exception))
        self.assertIsInstance(caught.exception.__cause__, subprocess.CalledProcessError)

    def test_timeout_preserves_partial_bytes_and_cause(self):
        error = subprocess.TimeoutExpired(["fixture-tool"], 120, output=b"partial output", stderr=b"timeout detail")
        with patch("package.subprocess.run", side_effect=error), self.assertRaises(ValueError) as caught:
            run("fixture-tool")
        for evidence in ("fixture-tool", "120", "partial output", "timeout detail"):
            self.assertIn(evidence, str(caught.exception))
        self.assertIs(caught.exception.__cause__, error)


if __name__ == "__main__":
    unittest.main()
