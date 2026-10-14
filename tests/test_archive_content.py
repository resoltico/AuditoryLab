# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Independent content/type controls for native distribution archives."""

import io
import json
from pathlib import Path
import stat
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from test_release_archive import LINUX, MAC, REPOSITORY, SOURCE, VERSION, WINDOWS, archive, trusted_package_fixture, valid_payload


class ArchiveContentChecks(unittest.TestCase):
    def setUp(self):
        anchor = archive.ROOT / "target" / "archive-content-tests"
        anchor.mkdir(parents=True, exist_ok=True)
        self.scratch = tempfile.TemporaryDirectory(dir=anchor)
        self.addCleanup(self.scratch.cleanup)
        self.directory = Path(self.scratch.name)
        self.trusted_packages = [trusted_package_fixture(self.directory, "libm", "0.2.16")]
        metadata = patch.object(archive, "target_notice_packages", return_value=self.trusted_packages)
        metadata.start()
        self.addCleanup(metadata.stop)
        rust_text = self.directory / "trusted-RUST-COPYRIGHT-library.html"
        rust_text.write_bytes(b"Runtime notices")
        rust = patch.object(archive, "rust_library_notice", return_value=rust_text)
        rust.start()
        self.addCleanup(rust.stop)
        self.fixture_package = None
        self.fixture_files = {}
        self.reference_case = 0

    def seed_notice_reference(self, relative="LICENSE.txt", data=b"Independent original grant\n", *, declared=None):
        """Seed source separately; later archive mutations never change it."""
        self.reference_case += 1
        directory = self.directory / f"notice-reference-case-{self.reference_case}"
        source_file = directory / relative
        source_file.parent.mkdir(parents=True)
        source_file.write_bytes(data)
        reference = {"name": "fixture", "version": "1.0.0", "license": "MIT",
                     "license_file": declared, "repository": None,
                     "manifest_path": str(directory / "Cargo.toml")}
        self.trusted_packages[:] = [self.trusted_packages[0], reference]
        name = "third-party/fixture-1.0.0/" + relative
        self.fixture_package = {key: reference[key] for key in ("name", "version", "license", "license_file", "repository")}
        self.fixture_package["files"] = [name]
        self.fixture_files = {name: data}

    def write_archive(self, target=MAC, change=None, member_change=None, additional=None):
        entry, files, info = valid_payload(target)
        if self.fixture_package:
            inventory = json.loads(files["third-party/index.json"])
            inventory["packages"].append(self.fixture_package)
            files["third-party/index.json"] = json.dumps(inventory).encode()
            files.update(self.fixture_files)
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
                    member.external_attr = (stat.S_IFREG | (0o755 if name in ("auditory-lab", "auditory-lab.exe") else 0o644)) << 16
                    if member_change:
                        member_change(name, member)
                    output.writestr(member, data)
                if additional:
                    additional(output, entry)
        else:
            with tarfile.open(path, "w:gz") as output:
                for name, data in files.items():
                    member = tarfile.TarInfo(entry["root"] + "/" + name)
                    member.size = len(data)
                    member.mode = 0o755 if name == "auditory-lab" else 0o644
                    if member_change:
                        member_change(name, member)
                    output.addfile(member, io.BytesIO(data))
                if additional:
                    additional(output, entry)
        return path

    def verify(self, path, target=MAC):
        return archive.verify_archive(path, target, VERSION, SOURCE, REPOSITORY)

    def add_notice(self, files, relative, data, *, declared=None):
        inventory = json.loads(files["third-party/index.json"])
        package = next(package for package in inventory["packages"] if package["name"] == "fixture")
        package["files"].append("third-party/fixture-1.0.0/" + relative)
        if declared is not None:
            package["license_file"] = declared
        files["third-party/fixture-1.0.0/" + relative] = data
        files["third-party/index.json"] = json.dumps(inventory).encode()

    def test_valid_structural_controls_cover_all_container_platforms(self):
        for target in (MAC, WINDOWS, LINUX):
            with self.subTest(target=target):
                self.assertEqual(self.verify(self.write_archive(target), target)["target"], target)

    def test_regular_payload_named_exactly_root_is_rejected(self):
        for target in (MAC, LINUX):
            def add(output, entry):
                if target == MAC:
                    member = zipfile.ZipInfo(entry["root"])
                    member.external_attr = (stat.S_IFREG | 0o644) << 16
                    output.writestr(member, b"unlisted root payload")
                else:
                    member = tarfile.TarInfo(entry["root"])
                    member.size = 21
                    member.mode = 0o644
                    output.addfile(member, io.BytesIO(b"unlisted root payload"))
            with self.subTest(target=target), self.assertRaisesRegex(ValueError, "root must be a directory"):
                self.verify(self.write_archive(target, additional=add), target)

    def test_zip_directory_kind_without_trailing_slash_is_rejected(self):
        def change(name, member):
            if name == "USAGE.txt":
                member.external_attr = (stat.S_IFDIR | 0o644) << 16
        with self.assertRaisesRegex(ValueError, "type and trailing slash disagree"):
            self.verify(self.write_archive(member_change=change))

    def test_zip_regular_kind_with_directory_slash_is_rejected(self):
        def add(output, entry):
            member = zipfile.ZipInfo(entry["root"] + "/third-party/")
            member.external_attr = (stat.S_IFREG | 0o755) << 16
            output.writestr(member, b"")
        with self.assertRaisesRegex(ValueError, "type and trailing slash disagree"):
            self.verify(self.write_archive(additional=add))

    def test_valid_explicit_directory_and_missing_zip_kind(self):
        for kind in (stat.S_IFDIR, 0):
            def add(output, entry):
                member = zipfile.ZipInfo(entry["root"] + "/third-party/")
                member.external_attr = (kind | 0o755) << 16
                output.writestr(member, b"")
            with self.subTest(kind=kind):
                self.verify(self.write_archive(additional=add))

    def test_directory_payload_and_bad_modes_are_rejected(self):
        for target in (MAC, LINUX):
            for mode, payload in ((0o777, b""), (0o755, b"payload")):
                def add(output, entry):
                    if target == MAC:
                        member = zipfile.ZipInfo(entry["root"] + "/third-party/")
                        member.external_attr = (stat.S_IFDIR | mode) << 16
                        output.writestr(member, payload)
                    else:
                        member = tarfile.TarInfo(entry["root"] + "/third-party")
                        member.type = tarfile.DIRTYPE
                        member.mode = mode
                        member.size = len(payload)
                        output.addfile(member, io.BytesIO(payload))
                with self.subTest(target=target, mode=mode, payload=payload), self.assertRaisesRegex(ValueError, "directories must have no payload and mode 0755"):
                    self.verify(self.write_archive(target, additional=add), target)

    def test_executable_and_data_modes_are_exact_for_every_container(self):
        for target in (MAC, WINDOWS, LINUX):
            for name, mode in (("LICENSE", 0o777), ("USAGE.txt", 0o755), ("third-party/RUST-COPYRIGHT-library.html", 0o600), ("auditory-lab.exe" if target == WINDOWS else "auditory-lab", 0o644)):
                def change(current, member):
                    if current == name:
                        if target == LINUX:
                            member.mode = mode
                        else:
                            member.external_attr = (stat.S_IFREG | mode) << 16
                with self.subTest(target=target, name=name, mode=mode), self.assertRaisesRegex(ValueError, "files must use mode"):
                    self.verify(self.write_archive(target, member_change=change), target)

    def test_tar_owner_metadata_is_neutral(self):
        for key, value in (("uid", 1001), ("gid", 1001), ("uname", "runner"), ("gname", "runner")):
            def change(name, member):
                if name == "LICENSE":
                    setattr(member, key, value)
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "neutral numeric ownership"):
                self.verify(self.write_archive(LINUX, member_change=change), LINUX)

    def test_rehashed_notice_inventory_cannot_admit_executables_or_source(self):
        self.seed_notice_reference()
        for name in ("helper.exe", "helper.rs", "LICENSE.rs"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "complete trusted source materials"):
                self.verify(self.write_archive(change=lambda files, info: self.add_notice(files, name, b"ordinary UTF-8 text")))

    def test_rehashed_declared_license_cannot_disguise_source_or_binary_name(self):
        self.seed_notice_reference()
        for name in ("helper.exe", "helper.rs"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "trusted Cargo metadata"):
                self.verify(self.write_archive(change=lambda files, info: self.add_notice(files, name, b"ordinary text", declared=name)))

    def test_legal_basename_cannot_disguise_binary_or_invalid_utf8(self):
        self.seed_notice_reference()
        for data, message in ((b"MZ\0binary", "NUL/binary"), (b"\xff\xfe", "UTF-8 text")):
            with self.subTest(data=data), self.assertRaisesRegex(ValueError, message):
                self.verify(self.write_archive(change=lambda files, info: self.add_notice(files, "LICENSE-EXTRA", data)))

    def test_original_case_versioned_name_and_nested_declared_license_are_valid(self):
        for name, declared in (("license-apache-2.0", None), ("COPYRIGHT.html", None), ("legal/COPYING", "legal/COPYING"), ("legal/agreement.txt", "legal/agreement.txt")):
            with self.subTest(name=name):
                self.seed_notice_reference(name, b"License text\n", declared=declared)
                self.verify(self.write_archive())

    def test_package_inventory_cannot_borrow_another_packages_notice(self):
        self.seed_notice_reference()
        def change(files, info):
            inventory = json.loads(files["third-party/index.json"])
            inventory["packages"][-1]["files"] = ["third-party/libm-0.2.16/LICENSE.txt"]
            files["third-party/index.json"] = json.dumps(inventory).encode()
        with self.assertRaisesRegex(ValueError, "complete trusted source materials"):
            self.verify(self.write_archive(change=change))

    def write_tree(self, target, members, *, zip_kind=True):
        """Write raw tree controls, deliberately independent of inventories."""
        entry = archive.target_entry(target, VERSION)
        path = self.directory / entry["archive"]
        if path.suffix == ".zip":
            with zipfile.ZipFile(path, "w") as output:
                for name, payload in members:
                    directory = payload is None
                    member = zipfile.ZipInfo(entry["root"] + "/" + name + ("/" if directory else ""))
                    kind = stat.S_IFDIR if directory else stat.S_IFREG
                    member.external_attr = ((kind if zip_kind else 0) | (0o755 if directory else 0o644)) << 16
                    output.writestr(member, b"" if directory else payload)
        else:
            with tarfile.open(path, "w:gz") as output:
                for name, payload in members:
                    member = tarfile.TarInfo(entry["root"] + "/" + name)
                    member.mode = 0o755 if payload is None else 0o644
                    if payload is None:
                        member.type = tarfile.DIRTYPE
                        output.addfile(member)
                    else:
                        member.size = len(payload)
                        output.addfile(member, io.BytesIO(payload))
        return path, entry["root"]

    def test_regular_ancestors_reject_exact_and_casefolded_names_in_all_orders(self):
        for target in (MAC, LINUX):
            for parent in ("LICENSE", "license"):
                for reverse in (False, True):
                    for zip_kind in ((False, True) if target == MAC else (True,)):
                        members = [("notices/LICENSE", b"parent file"), ("notices/" + parent + "/NOTICE", b"child file")]
                        if reverse:
                            members.reverse()
                        with self.subTest(target=target, parent=parent, reverse=reverse, zip_kind=zip_kind):
                            path, root = self.write_tree(target, members, zip_kind=zip_kind)
                            with self.assertRaisesRegex(ValueError, "regular file as a parent directory"):
                                archive.archive_members(path, root)

    def test_explicit_directory_cannot_descend_from_a_regular_file(self):
        for target in (MAC, LINUX):
            for reverse in (False, True):
                members = [("notices/LICENSE", b"parent file"), ("notices/license/child", None), ("notices/license/child/NOTICE", b"notice")]
                if reverse:
                    members.reverse()
                with self.subTest(target=target, reverse=reverse):
                    path, root = self.write_tree(target, members)
                    with self.assertRaisesRegex(ValueError, "regular file as a parent directory"):
                        archive.archive_members(path, root)

    def test_normal_directory_trees_and_shared_string_prefixes_remain_valid(self):
        for target in (MAC, LINUX):
            for explicit in (False, True):
                for reverse in (False, True):
                    members = [("notices/LICENSE", b"license"), ("notices/LICENSE-copy/NOTICE", b"notice")]
                    if explicit:
                        members.extend([("notices", None), ("notices/LICENSE-copy", None)])
                    if reverse:
                        members.reverse()
                    with self.subTest(target=target, explicit=explicit, reverse=reverse):
                        path, root = self.write_tree(target, members)
                        files, _ = archive.archive_members(path, root)
                        self.assertEqual(files, {root + "/notices/LICENSE": b"license", root + "/notices/LICENSE-copy/NOTICE": b"notice"})


if __name__ == "__main__":
    unittest.main()
