# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Reject conflicting public/draft content and exercise ambiguous API outcomes."""
import copy
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from publish_release import ApiError, Publisher, qualified_files, tag_commit


SOURCE = "a" * 40
OTHER = "b" * 40
REPOSITORY = "owner/repository"
NOTES = "- First release.\n"


class GitHubFixture:
    def __init__(self):
        self.main = SOURCE
        self.tag = None
        self.current = None
        self.assets = []
        self.content = {}
        self.mutations = []
        self.ambiguous = set()
        self.before_publish_main = False
        self.timeout = set()

    def api(self, endpoint, method="GET", data=None):
        base = "repos/" + REPOSITORY
        if method != "GET":
            self.mutations.append((method, endpoint, copy.deepcopy(data)))
        if endpoint == base + "/git/ref/heads/main":
            return {"object": {"type": "commit", "sha": self.main, "url": "not-used"}}
        if endpoint == base + "/git/ref/tags/v0.1.0":
            if self.tag is None:
                raise ApiError("missing", missing=True)
            return {"object": {"type": "commit", "sha": self.tag}}
        if endpoint == base + "/git/refs" and method == "POST":
            self.tag = data["sha"]
            if "tag" in self.timeout:
                raise subprocess.TimeoutExpired(["gh", "api"], 90)
            return {"object": {"type": "commit", "sha": self.tag}}
        if endpoint == base + "/releases/tags/v0.1.0":
            if self.current is None:
                raise ApiError("missing", missing=True)
            return copy.deepcopy(self.current)
        if endpoint == base + "/releases" and method == "POST":
            self.current = dict(data, id=1, html_url="https://github.com/owner/repository/releases/tag/v0.1.0")
            if "create" in self.timeout:
                raise subprocess.TimeoutExpired(["gh", "api"], 90)
            if "create" in self.ambiguous:
                raise ApiError("lost create response")
            return copy.deepcopy(self.current)
        if endpoint == base + "/releases/1/assets?per_page=100":
            if self.before_publish_main and len(self.assets) == 2:
                self.main = OTHER
            return copy.deepcopy(self.assets)
        if endpoint == base + "/releases/1" and method == "PATCH":
            self.current["draft"] = False
            if "publish" in self.timeout:
                raise subprocess.TimeoutExpired(["gh", "api"], 90)
            if "publish" in self.ambiguous:
                raise ApiError("lost publish response")
            return copy.deepcopy(self.current)
        if endpoint.startswith(base + "/releases/assets/") and method == "DELETE":
            asset_id = int(endpoint.rsplit("/", 1)[1])
            self.assets = [asset for asset in self.assets if asset["id"] != asset_id]
            return None
        raise AssertionError((method, endpoint, data))

    def upload(self, repository, tag, path):
        self.mutations.append(("UPLOAD", path.name, None))
        asset_id = max(self.content, default=0) + 1
        self.content[asset_id] = path.read_bytes()
        self.assets.append({"id": asset_id, "name": path.name, "state": "uploaded", "size": path.stat().st_size})
        if "upload" in self.ambiguous:
            raise subprocess.CalledProcessError(1, ["gh", "release", "upload"])

    def download(self, endpoint, destination):
        destination.write_bytes(self.content[int(endpoint.rsplit("/", 1)[1])])


class PublishTests(unittest.TestCase):
    def setUp(self):
        parent = Path(__file__).resolve().parents[1] / "target" / "release-tool-tests"
        parent.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=parent)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.files = {}
        for name, body in (("archive.zip", b"qualified archive"), ("SHA256SUMS", b"independent checksums")):
            path = self.root / name
            path.write_bytes(body)
            self.files[name] = path
        self.api = GitHubFixture()
        self.publisher = Publisher(self.api, REPOSITORY, "0.1.0", SOURCE, NOTES, self.files, self.root / "remote")

    def seed(self, draft=True):
        self.api.tag = SOURCE
        self.api.current = {"id": 1, "tag_name": "v0.1.0", "target_commitish": SOURCE,
            "name": "v0.1.0", "body": NOTES, "draft": draft, "prerelease": False,
            "html_url": "https://github.com/owner/repository/releases/tag/v0.1.0"}

    def upload_all(self):
        for path in self.files.values():
            self.api.upload(REPOSITORY, "v0.1.0", path)
        self.api.mutations.clear()

    def test_draft_first_then_verified_publication(self):
        url = self.publisher.publish()
        self.assertTrue(url.endswith("/v0.1.0"))
        self.assertFalse(self.api.current["draft"])
        self.assertEqual(len(self.api.assets), 2)
        self.assertEqual(self.api.mutations[-1][0], "PATCH")

    def test_matching_published_release_has_no_mutation(self):
        self.seed(draft=False)
        self.upload_all()
        self.publisher.publish()
        self.assertEqual(self.api.mutations, [])

    def test_published_missing_tag_is_not_recreated(self):
        self.seed(draft=False)
        self.upload_all()
        self.api.tag = None
        with self.assertRaises(ValueError):
            self.publisher.publish()
        self.assertEqual(self.api.mutations, [])

    def test_wrong_source_or_tag_prevents_mutation(self):
        for attr in ("main", "tag"):
            with self.subTest(attr=attr):
                setattr(self.api, attr, OTHER)
                with self.assertRaises(ValueError):
                    self.publisher.publish()
                self.assertEqual(self.api.mutations, [])
                setattr(self.api, attr, SOURCE if attr == "main" else None)

    def test_wrong_notes_prevents_mutation(self):
        self.seed()
        self.api.current["body"] = "invented notes"
        with self.assertRaises(ValueError):
            self.publisher.publish()
        self.assertEqual(self.api.mutations, [])

    def test_extra_asset_prevents_publication_and_is_preserved(self):
        self.seed()
        self.api.assets = [{"id": 42, "name": "unexpected.exe", "state": "uploaded", "size": 1}]
        with self.assertRaises(ValueError):
            self.publisher.publish()
        self.assertEqual(self.api.mutations, [])
        self.assertEqual(self.api.assets[0]["name"], "unexpected.exe")

    def test_duplicate_names_or_ids_rejected(self):
        for duplicate_id in (False, True):
            self.seed()
            self.upload_all()
            if duplicate_id:
                self.api.assets[1]["id"] = self.api.assets[0]["id"]
            else:
                self.api.assets.append(copy.deepcopy(self.api.assets[0]))
            with self.assertRaises(ValueError):
                self.publisher.publish()
            self.assertEqual(self.api.mutations, [])
            self.api.assets.clear()
            self.api.content.clear()

    def test_changed_draft_bytes_are_not_overwritten(self):
        self.seed()
        self.upload_all()
        asset_id = self.api.assets[0]["id"]
        self.api.content[asset_id] = b"x" * len(self.api.content[asset_id])
        with self.assertRaises(ValueError):
            self.publisher.publish()
        self.assertEqual(self.api.mutations, [])
        self.assertTrue(self.api.current["draft"])

    def test_empty_failed_starter_repaired_only_in_draft(self):
        self.seed()
        self.api.assets = [{"id": 42, "name": "archive.zip", "state": "starter", "size": 0}]
        self.publisher.publish()
        self.assertTrue(any(method == "DELETE" for method, _, _ in self.api.mutations))
        self.assertFalse(self.api.current["draft"])

    def test_ambiguous_create_upload_and_publish_are_read_back(self):
        self.api.ambiguous = {"create", "upload", "publish"}
        self.publisher.publish()
        self.assertFalse(self.api.current["draft"])
        self.assertFalse(any(method == "DELETE" for method, _, _ in self.api.mutations))

    def test_timeouts_after_accepted_mutations_are_read_back(self):
        self.api.timeout = {"tag", "create", "publish"}
        self.publisher.publish()
        self.assertFalse(self.api.current["draft"])
        self.assertFalse(any(method == "DELETE" for method, _, _ in self.api.mutations))

    def test_broken_checksum_link_cannot_create_outside_staging(self):
        staging = self.root / "staging"
        staging.mkdir()
        (staging / "archive.zip").write_bytes(b"archive validation is tested separately")
        outside = self.root / "outside-checksums"
        try:
            (staging / "SHA256SUMS").symlink_to(outside)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f"Symbolic links unavailable: {error}")
        # Isolate the checksum destination boundary; real archives have separate controls.
        fixture = types.SimpleNamespace(matrix_for=lambda _: [{"archive": "archive.zip", "target": "fixture"}],
                                        verify_archive=lambda *args: None)
        with mock.patch.dict(sys.modules, {"release_archive": fixture}):
            with self.assertRaises(ValueError):
                qualified_files(staging, REPOSITORY, "0.1.0", SOURCE)
        self.assertFalse(outside.exists())

    def test_main_changed_before_visibility_prevents_publish(self):
        self.api.before_publish_main = True
        with self.assertRaises(ValueError):
            self.publisher.publish()
        self.assertTrue(self.api.current["draft"])
        self.assertFalse(any(method == "PATCH" for method, _, _ in self.api.mutations))

    def test_annotated_tags_are_peeled_and_cycles_rejected(self):
        class TagApi:
            def __init__(self, cycle):
                self.cycle = cycle

            def api(self, endpoint):
                if "/git/ref/" in endpoint or self.cycle:
                    return {"object": {"type": "tag", "sha": OTHER}}
                return {"object": {"type": "commit", "sha": SOURCE}}
        self.assertEqual(tag_commit(TagApi(False), REPOSITORY, "v0.1.0"), SOURCE)
        with self.assertRaises(ValueError):
            tag_commit(TagApi(True), REPOSITORY, "v0.1.0")


if __name__ == "__main__":
    unittest.main()
