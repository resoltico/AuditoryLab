# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Which crates the notices cover, and the guards on reviewed supplements."""
import copy
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import notices


def package_id(name):
    source = ("path+file:///workspace" if name in {"auditory-cli", "auditory-gui", "auditory-core"} else
              "registry+https://github.com/rust-lang/crates.io-index")
    return f"{source}#{name}@1.0.0"


def package(name, source="registry+https://github.com/rust-lang/crates.io-index", kind=None):
    return {"id": package_id(name), "name": name, "version": "1.0.0", "source": source, "license": "MIT",
            "manifest_path": f"/registry/{name}/Cargo.toml",
            "targets": [{"name": {"auditory-cli": "auditorylab", "auditory-gui": "auditorylab-gui"}[name], "kind": ["bin"]}] if name in {"auditory-cli", "auditory-gui"} else []}


def dep(name, *kinds):
    return {"pkg": package_id(name), "dep_kinds": [{"kind": kind} for kind in kinds]}


METADATA = {
    "workspace_members": [package_id("auditory-cli"), package_id("auditory-gui"), package_id("auditory-core")],
    "packages": [package("auditory-cli", source=None), package("auditory-gui", source=None), package("auditory-core", source=None),
                 package("normal"), package("proc-macro"), package("build-only"), package("dev-only"),
                 package("under-proc-macro"), package("under-build")],
    "resolve": {"nodes": [
        {"id": package_id("auditory-gui"), "deps": [dep("auditory-core", None)]},
        {"id": package_id("auditory-cli"), "deps": [dep("auditory-core", None), dep("dev-only", "dev")]},
        {"id": package_id("auditory-core"), "deps": [dep("normal", None), dep("proc-macro", None), dep("build-only", "build")]},
        {"id": package_id("normal"), "deps": []},
        {"id": package_id("proc-macro"), "deps": [dep("under-proc-macro", None)]},
        {"id": package_id("build-only"), "deps": [dep("under-build", None)]},
        {"id": package_id("dev-only"), "deps": []},
        {"id": package_id("under-proc-macro"), "deps": []},
        {"id": package_id("under-build"), "deps": []},
    ]},
}

class ClosureTests(unittest.TestCase):
    def test_normal_dependencies_and_proc_macros_are_covered_but_not_build_or_dev_ones(self):
        tree = "\n".join(name + " v1.0.0" for name in
                         ("auditory-cli", "auditory-gui", "auditory-core", "normal", "proc-macro", "under-proc-macro"))
        runtime = "\n".join(name + " v1.0.0" for name in ("auditory-cli", "auditory-gui", "auditory-core", "normal"))
        def selected(command, **kwargs):
            return runtime if "normal,no-proc-macro" in command else tree
        with mock.patch.object(notices.subprocess, "check_output", side_effect=selected) as query:
            material = notices.notice_packages(METADATA, "fixture-target")
        for offset in (0, 2):
            for root, call in zip(METADATA["workspace_members"][:2], query.call_args_list[offset:offset + 2]):
                self.assertIn(root, call.args[0])
        roles = {package["name"]: package["notice_role"] for package in material}
        self.assertEqual(sorted(roles), ["normal", "proc-macro", "under-proc-macro"])
        self.assertEqual(roles["normal"], "runtime dependency")
        self.assertEqual(roles["proc-macro"], "host code-generation material")
        self.assertEqual(roles["under-proc-macro"], "host code-generation material")

    def test_runtime_reachability_unions_products_without_dropping_host_material(self):
        def selected(command, **kwargs):
            base = "auditory-cli v1.0.0\nauditory-gui v1.0.0\n"
            if "normal,no-proc-macro" not in command:
                return base + "proc-macro v1.0.0\nunder-proc-macro v1.0.0\n"
            return base + ("under-proc-macro v1.0.0\n" if command[-1] == package_id("auditory-gui") else "")
        with mock.patch.object(notices.subprocess, "check_output", side_effect=selected):
            material = notices.notice_packages(METADATA, "fixture-target")
        roles = {package["name"]: package["notice_role"] for package in material}
        self.assertEqual(roles["under-proc-macro"], "runtime dependency")
        self.assertEqual(roles["proc-macro"], "host code-generation material")

    def test_missing_or_ambiguous_root_cannot_silently_drop_notices(self):
        for mode in ("missing", "ambiguous"):
            meta = copy.deepcopy(METADATA)
            if mode == "missing":
                meta["workspace_members"].remove(meta["packages"][0]["id"])
            else:
                other = copy.deepcopy(meta["packages"][0])
                other["id"] = "path+file:///other#auditory-cli@1.0.0"
                meta["packages"].append(other)
                meta["workspace_members"].append(other["id"])
            with self.subTest(mode=mode), self.assertRaisesRegex(ValueError, "exactly one auditory-cli"):
                notices.notice_packages(meta, "fixture-target")


    def test_actual_cargo_tree_identity_selection_fails_closed(self):
        baseline = "auditory-cli v1.0.0\nauditory-gui v1.0.0\nnormal v1.0.0 (*)\n"
        # Metadata intentionally includes build-only/dev-only/inactive nodes;
        # Cargo's normal tree is the selected closure, not all metadata edges.
        with mock.patch.object(notices.subprocess, "check_output", return_value=baseline):
            self.assertEqual([p["name"] for p in notices.notice_packages(METADATA, "fixture-target")], ["normal"])
        ambiguous = copy.deepcopy(METADATA)
        other = copy.deepcopy(next(p for p in ambiguous["packages"] if p["name"] == "normal"))
        other["id"] = "registry+https://other.invalid/index#normal@1.0.0"
        ambiguous["packages"].append(other)
        for meta, output in [(METADATA, baseline + "unexpected v1.0.0\n"),
                             (ambiguous, baseline), (METADATA, "normal v1.0.0\n")]:
            with mock.patch.object(notices.subprocess, "check_output", return_value=output), self.assertRaises(ValueError):
                notices.notice_packages(meta, "fixture-target")


class SupplementTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.licenses = Path(self.directory.name)
        (self.licenses / "MIT.txt").write_bytes(b"MIT terms\n")
        (self.licenses / ".cargo_vcs_info.json").write_text('{"git":{"sha1":"reviewed-commit"}}', encoding="utf-8")
        self.digest = hashlib.sha256(b"MIT terms\n").hexdigest()
        patcher = mock.patch.object(notices, "LICENSES", self.licenses)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.addCleanup(self.directory.cleanup)

    def reviewed(self, license="MIT", digest=None):
        return {"normal": {"license": license, "version": "1.0.0", "repository": None, "vcs": "reviewed-commit", "files": [{"file": "MIT.txt", "sha256": digest or self.digest}]}}

    def package(self):
        result = package("normal")
        result["manifest_path"] = str(self.licenses / "Cargo.toml")
        return result

    def test_same_license_identity_changes_require_review(self):
        for field, value in (("version", "2.0.0"), ("repository", "https://changed.example")):
            changed = self.package()
            changed[field] = value
            with self.assertRaisesRegex(ValueError, "published identity"):
                notices.supplement(changed, self.reviewed())
        (self.licenses / ".cargo_vcs_info.json").write_text('{"git":{"sha1":"another-commit"}}', encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "published identity"):
            notices.supplement(self.package(), self.reviewed())

    def test_additional_source_change_requires_review(self):
        (self.licenses / "sin.rs").write_bytes(b"source")
        reviewed = self.reviewed()
        reviewed["normal"]["sources"] = {"sin.rs": hashlib.sha256(b"source").hexdigest()}
        notices.supplement(self.package(), reviewed)
        (self.licenses / "sin.rs").write_bytes(b"altered")
        with self.assertRaisesRegex(ValueError, "reviewed source"):
            notices.supplement(self.package(), reviewed)

    def test_a_reviewed_supplement_supplies_the_files(self):
        files = notices.supplement(self.package(), self.reviewed())
        self.assertEqual([path.name for path in files], ["MIT.txt"])

    def test_missing_relicensed_and_altered_supplements_are_rejected(self):
        for reviewed in [{}, self.reviewed(license="Apache-2.0"), self.reviewed(digest="0" * 64)]:
            with self.assertRaises(ValueError):
                notices.supplement(self.package(), reviewed)


class LibmNoticeTests(unittest.TestCase):
    def test_linked_source_notices_preserve_the_permission_and_attributions(self):
        text = (notices.LICENSES / "libm-source-notices.txt").read_text(encoding="utf-8")
        for attribution in ("SunSoft", "SunPro", "Copyright (C) 2004 by Sun Microsystems"):
            self.assertIn(attribution, text)
        self.assertIn("software is freely granted, provided that this notice", text)
        self.assertIn("is preserved.", text)

    def test_missing_or_altered_libm_material_is_rejected(self):
        import json
        reviewed = json.loads((notices.LICENSES / "supplements.json").read_text(encoding="utf-8"))
        libm = next(p for p in notices.notice_packages(notices.metadata("aarch64-apple-darwin"), "aarch64-apple-darwin") if p["name"] == "libm")
        with self.assertRaises(ValueError):
            notices.supplement(libm, {})
        saved = reviewed["libm"]["files"]
        reviewed["libm"]["files"] = []
        with self.assertRaisesRegex(ValueError, "no reviewed notice"):
            notices.supplement(libm, reviewed)
        reviewed["libm"]["files"] = saved
        sources = reviewed["libm"].pop("sources")
        with self.assertRaisesRegex(ValueError, "missing linked source"):
            notices.supplement(libm, reviewed)
        reviewed["libm"]["sources"] = sources
        reviewed["libm"]["files"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "reviewed bytes"):
            notices.supplement(libm, reviewed)


if __name__ == "__main__":
    unittest.main()
