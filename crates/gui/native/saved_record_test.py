"""Literal persistence controls; no native runtime pass is implied."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from saved_record import saved_record


class SavedRecordControls(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.library = Path(self.scratch.name)
        (self.library / "recipes").mkdir()
        self.entry = {"key": "user:00000000-0000-4000-8000-000000000001", "document_id": "00000000-0000-4000-8000-000000000001",
                      "collection": "user", "trashed": False, "version_token": "new"}
        self.content = b'[document]\nid = "00000000-0000-4000-8000-000000000001"\n[description]\nnotes = "first native line\\nsecond native line"\n'
        (self.library / "recipes" / (self.entry["document_id"] + ".toml")).write_bytes(self.content)

    def probe(self, entries=None, complete=True, previous=None, portable=None, expected="first native line\nsecond native line"):
        listing = json.dumps({"complete": complete, "entries": [self.entry] if entries is None else entries}).encode()
        with patch("saved_record.subprocess.run", side_effect=[subprocess.CompletedProcess([], 0, listing),
                   subprocess.CompletedProcess([], 0, self.content if portable is None else portable)]):
            return saved_record(Path("cli"), self.library, expected, previous)

    def test_exact_literal_file_passes(self):
        self.assertEqual(self.probe()["owned_records"], 1)

    def test_initial_publication_is_pending(self):
        self.assertIsNone(self.probe(entries=[]))

    def test_duplicate_records_fail(self):
        with self.assertRaisesRegex(RuntimeError, "exactly one"):
            self.probe(entries=[self.entry, self.entry])

    def test_wrong_notes_fail(self):
        with self.assertRaisesRegex(RuntimeError, "literal notes"):
            self.probe(expected="")

    def test_wrong_file_fails(self):
        with self.assertRaisesRegex(RuntimeError, "portable export"):
            self.probe(portable=b"foreign")

    def test_incomplete_listing_fails(self):
        with self.assertRaisesRegex(RuntimeError, "incomplete"):
            self.probe(complete=False)

    def test_unchanged_version_is_pending(self):
        self.assertIsNone(self.probe(previous=dict(self.entry)))

    def test_new_binding_fails(self):
        with self.assertRaisesRegex(RuntimeError, "another record"):
            self.probe(previous={"key": "user:foreign", "document_id": "foreign", "version_token": "old"})

    def test_library_lock_is_pending(self):
        with patch("saved_record.subprocess.run", return_value=subprocess.CompletedProcess([], 4, b"")):
            self.assertIsNone(saved_record(Path("cli"), self.library, ""))


if __name__ == "__main__":
    unittest.main()
