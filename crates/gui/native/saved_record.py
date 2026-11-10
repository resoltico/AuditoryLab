"""Persisted native-keyboard witness, independent of GUI provider strings."""
import hashlib
import json
import subprocess
import tomllib


def saved_record(cli, library, expected_notes, previous=None):
    def run(*args):
        result = subprocess.run([str(cli), "--library-dir", str(library), *args],
                                capture_output=True, timeout=5)
        if result.returncode == 4:  # Publication currently holds the library lock.
            return None
        result.check_returncode()
        return result.stdout

    output = run("list", "--collection", "user", "--format", "json")
    if output is None:
        return None
    listed = json.loads(output)
    if not listed["complete"]:
        raise RuntimeError("native save produced an incomplete library listing")
    entries = listed["entries"]
    if not entries and previous is None:
        return None
    if len(entries) != 1:
        raise RuntimeError("native save must own exactly one user record")
    entry = entries[0]
    if previous and entry["version_token"] == previous["version_token"]:
        return None
    if entry["collection"] != "user" or entry["trashed"] or not entry["version_token"]:
        raise RuntimeError("native save did not publish an active owned record")
    if previous and (entry["key"], entry["document_id"]) != (previous["key"], previous["document_id"]):
        raise RuntimeError("native save created another record instead of updating its binding")
    portable = run("recipe", entry["key"])
    if portable is None:
        return None
    path = library / "recipes" / (entry["document_id"] + ".toml")
    persisted = path.read_bytes()
    if persisted != portable:
        raise RuntimeError("native save file differs from ordinary CLI portable export")
    recipe = tomllib.loads(persisted.decode("utf-8"))
    if recipe["document"]["id"] != entry["document_id"] or recipe["description"]["notes"] != expected_notes:
        raise RuntimeError("native save persisted wrong identity or literal notes")
    return {"key": entry["key"], "document_id": entry["document_id"],
            "version_token": entry["version_token"], "sha256": hashlib.sha256(persisted).hexdigest(),
            "owned_records": 1, "notes": expected_notes}
