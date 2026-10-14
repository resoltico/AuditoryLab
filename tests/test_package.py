# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""The packager's PE import reader and archive reader, against independent fixtures."""
import io
from pathlib import Path
import stat
import struct
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from package import members, pe_imports, check_dependencies, check_identity, check_macos_minimum, check_macos_native, package
from release import archive_name


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

    def test_emulated_compiler_host_does_not_establish_native_machine(self):
        with patch("package.run", return_value="host: x86_64-apple-darwin\n"), patch("package.platform.system", return_value="Darwin"), patch("package.platform.machine", return_value="arm64"), self.assertRaisesRegex(ValueError, "native machine"):
            package("x86_64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path("unused"))
        with patch("package.run", return_value="host: x86_64-apple-darwin\n"), patch("package.platform.system", return_value="Linux"), patch("package.platform.machine", return_value="x86_64"), self.assertRaisesRegex(ValueError, "operating system"):
            package("x86_64-apple-darwin", "0.1.0", "owner/repo", Path("unused"), Path("unused"))

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
            destination = Path(directory) / archive_name("macos-aarch64", "0.1.0")
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


if __name__ == "__main__":
    unittest.main()
