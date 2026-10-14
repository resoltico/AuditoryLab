#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Publish only the six qualified archives, using a verified resumable draft."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from urllib.parse import quote

from release import ROOT, prepare, release_notes, validate_repository, validate_source, validate_version


class ApiError(RuntimeError):
    def __init__(self, message, missing=False):
        super().__init__(message)
        self.missing = missing


class GitHub:
    """GitHub CLI transport; credentials remain in the CLI's environment/keychain."""
    def api(self, endpoint, method="GET", data=None):
        command = ["gh", "api", endpoint, "--method", method]
        if data is not None:
            command += ["--input", "-"]
        result = subprocess.run(command, input=json.dumps(data) if data is not None else None,
                                text=True, capture_output=True, timeout=90, check=False)
        if result.returncode:
            raise ApiError(result.stderr.strip(), missing="(HTTP 404)" in result.stderr)
        return json.loads(result.stdout) if result.stdout.strip() else None

    def download(self, endpoint, destination):
        with destination.open("wb") as output:
            subprocess.run(["gh", "api", endpoint, "-H", "Accept: application/octet-stream"],
                           stdout=output, check=True, timeout=180)

    def upload(self, repository, release_id, path):
        endpoint = f"https://uploads.github.com/repos/{repository}/releases/{release_id}/assets?name={quote(path.name, safe='')}"
        subprocess.run(["gh", "api", endpoint, "--method", "POST", "--input", str(path),
                        "-H", "Content-Type: application/octet-stream"], stdout=subprocess.DEVNULL,
                       check=True, timeout=300)


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def tag_commit(api, repository, tag):
    """Resolve the actual tag object, including nested annotated tags."""
    try:
        obj = api.api(f"repos/{repository}/git/ref/tags/{tag}")["object"]
    except ApiError as error:
        if error.missing:
            return None
        raise
    visited = set()
    for _ in range(8):
        sha, kind = obj["sha"], obj["type"]
        validate_source(sha)
        if kind == "commit":
            return sha
        if kind != "tag" or sha in visited:
            raise ValueError("Release tag does not resolve to a commit.")
        visited.add(sha)
        obj = api.api(f"repos/{repository}/git/tags/{sha}")["object"]
    raise ValueError("Release tag annotation nesting exceeds the supported bound.")


def assert_identity(api, repository, tag, source, require_tag=True):
    main = api.api(f"repos/{repository}/git/ref/heads/main")["object"]
    if main.get("type") != "commit" or main.get("sha") != source:
        raise ValueError("Remote main differs from the qualified source commit.")
    commit = tag_commit(api, repository, tag)
    if commit != source and (require_tag or commit is not None):
        raise ValueError("Release tag differs from the qualified source commit.")


def require_release(release, tag, source, notes):
    if not isinstance(release, dict):
        raise ValueError("The selected release is missing; stop without recreating it.")
    if (release.get("tag_name") != tag or release.get("name") != tag
            or release.get("body") != notes or release.get("prerelease") is not False):
        raise ValueError("Existing release identity or notes differ; no changes made.")
    if release.get("draft") and release.get("target_commitish") != source:
        raise ValueError("Draft source differs from the qualified commit.")


def asset_inventory(assets, files, complete=False):
    names = [asset["name"] for asset in assets]
    ids = [asset["id"] for asset in assets]
    if len(set(names)) != len(names) or len(set(ids)) != len(ids):
        raise ValueError("Remote release has duplicate asset names or IDs.")
    if set(names) - set(files):
        raise ValueError("Remote release has unexpected assets; no assets removed.")
    if complete and set(names) != set(files):
        raise ValueError("Remote release asset set is incomplete.")
    for asset in assets:
        if asset.get("state") == "starter" and asset.get("size") == 0 and not complete:
            continue
        if asset.get("state") != "uploaded" or asset.get("size") != files[asset["name"]].stat().st_size:
            raise ValueError("Remote asset is incomplete or has an unexpected size.")
    return {asset["name"]: asset for asset in assets}


class Publisher:
    def __init__(self, api, repository, version, source, notes, files, scratch):
        self.api, self.repository, self.source = api, repository, source
        self.tag, self.notes, self.files = "v" + version, notes, files
        self.scratch = scratch
        scratch.mkdir(parents=True, exist_ok=True)

    def release(self):
        # The tag-name endpoint returns published releases only. Authenticated
        # listing includes drafts; inspect every page before choosing one ID.
        page, matches, ids = 1, [], set()
        while True:
            releases = self.api.api(f"repos/{self.repository}/releases?per_page=100&page={page}")
            for release in releases:
                if release["id"] in ids:
                    raise ValueError("Release listing contains duplicate IDs.")
                ids.add(release["id"])
                if release["tag_name"] == self.tag:
                    matches.append(release)
            if len(releases) < 100:
                break
            page += 1
        if len(matches) > 1:
            raise ValueError("Multiple releases match the selected tag; no changes made.")
        return matches[0] if matches else None

    def refresh(self, release):
        """Read the same object; a deleted/replaced draft is never recreated."""
        try:
            current = self.api.api(f"repos/{self.repository}/releases/{release['id']}")
        except ApiError as error:
            if error.missing:
                raise ValueError("The selected release ID disappeared; stop without recreating it.") from error
            raise
        if current.get("id") != release["id"]:
            raise ValueError("Release refresh changed identity; no changes made.")
        return current

    def assets(self, release):
        return self.api.api(f"repos/{self.repository}/releases/{release['id']}/assets?per_page=100")

    def verify_assets(self, release):
        inventory = asset_inventory(self.assets(release), self.files, complete=True)
        for name, asset in inventory.items():
            downloaded = self.scratch / name
            self.api.download(f"repos/{self.repository}/releases/assets/{asset['id']}", downloaded)
            if digest(downloaded) != digest(self.files[name]):
                raise ValueError(f"Remote asset content differs: {name}")

    def publish(self):
        api, repository, tag, source = self.api, self.repository, self.tag, self.source
        assert_identity(api, repository, tag, source, require_tag=False)
        release = self.release()
        if release:
            require_release(release, tag, source, self.notes)
            asset_inventory(self.assets(release), self.files, complete=not release["draft"])
            if not release["draft"]:
                assert_identity(api, repository, tag, source)
                self.verify_assets(release)
                current = self.refresh(release)
                require_release(current, tag, source, self.notes)
                if current["draft"]:
                    raise ValueError("The selected release is no longer published.")
                assert_identity(api, repository, tag, source)
                return current["html_url"]
        if tag_commit(api, repository, tag) is None:
            try:
                api.api(f"repos/{repository}/git/refs", "POST", {"ref": "refs/tags/" + tag, "sha": source})
            except (ApiError, subprocess.TimeoutExpired):
                # A lost response is ambiguous: accept only verified remote identity.
                assert_identity(api, repository, tag, source)
        assert_identity(api, repository, tag, source)
        if release is None:
            try:
                release = api.api(f"repos/{repository}/releases", "POST", {
                    "tag_name": tag, "target_commitish": source, "name": tag,
                    "body": self.notes, "draft": True, "prerelease": False,
                    "generate_release_notes": False})
            except (ApiError, subprocess.TimeoutExpired):
                release = self.release()
                if release is None:
                    raise
        require_release(release, tag, source, self.notes)
        if release["draft"]:
            inventory = asset_inventory(self.assets(release), self.files)
            for name, path in sorted(self.files.items()):
                asset = inventory.get(name)
                if asset and asset["state"] == "starter":
                    api.api(f"repos/{repository}/releases/assets/{asset['id']}", "DELETE")
                    asset = None
                if asset:
                    downloaded = self.scratch / name
                    api.download(f"repos/{repository}/releases/assets/{asset['id']}", downloaded)
                    if digest(downloaded) != digest(path):
                        raise ValueError(f"Draft asset content differs: {name}; retain the draft for review.")
                else:
                    try:
                        api.upload(repository, release["id"], path)
                    except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                        # Never delete/reupload after an uncertain upload result.
                        current = asset_inventory(self.assets(release), self.files)
                        candidate = current.get(name)
                        if not candidate or candidate["state"] != "uploaded":
                            raise
                        downloaded = self.scratch / name
                        api.download(f"repos/{repository}/releases/assets/{candidate['id']}", downloaded)
                        if digest(downloaded) != digest(path):
                            raise ValueError(f"Uncertain upload has conflicting content: {name}")
            self.verify_assets(release)
            release = self.refresh(release)
            require_release(release, tag, source, self.notes)
            if not release["draft"]:
                raise ValueError("Draft was published concurrently; stop for review.")
            assert_identity(api, repository, tag, source)
            try:
                api.api(f"repos/{repository}/releases/{release['id']}", "PATCH", {"draft": False, "make_latest": "true"})
            except (ApiError, subprocess.TimeoutExpired):
                current = self.refresh(release)
                if current["draft"]:
                    raise
        release = self.refresh(release)
        require_release(release, tag, source, self.notes)
        if release["draft"]:
            raise ValueError("Release is still a draft.")
        assert_identity(api, repository, tag, source)
        self.verify_assets(release)
        return release["html_url"]


def qualified_files(directory, repository, version, source):
    from release_archive import matrix_for, verify_archive
    matrix = matrix_for(version)
    expected = {entry["archive"] for entry in matrix}
    actual = {path.name for path in directory.iterdir()}
    if actual not in (expected, expected | {"SHA256SUMS"}):
        raise ValueError("Staging must contain exactly the six qualified archives.")
    files = {}
    for entry in matrix:
        path = directory / entry["archive"]
        if path.is_symlink() or not path.is_file():
            raise ValueError("Staged archives must be ordinary files.")
        verify_archive(path, entry["target"], version, source, repository)
        files[path.name] = path
    sums = directory / "SHA256SUMS"
    if sums.is_symlink():
        raise ValueError("Staged SHA256SUMS must not be a symbolic link.")
    content = "".join(f"{digest(path)}  {name}\n" for name, path in sorted(files.items()))
    if sums.exists():
        if not sums.is_file() or sums.read_text(encoding="ascii") != content:
            raise ValueError("Existing staged SHA256SUMS conflicts with the qualified archives.")
    else:
        sums.write_text(content, encoding="ascii")
    files[sums.name] = sums
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--source", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--assets", type=Path, required=True)
    args = parser.parse_args()
    try:
        version, source, repository = validate_version(args.version), validate_source(args.source), validate_repository(args.repository)
        if prepare(version, repository)["source"] != source:
            raise ValueError("Publisher checkout differs from the qualified source commit.")
        scratch = ROOT / "target" / "release-publication"
        scratch.mkdir(parents=True, exist_ok=True)
        files = qualified_files(args.assets, repository, version, source)
        for name, path in files.items():
            if name == "SHA256SUMS":
                continue
            result = subprocess.run(["gh", "attestation", "verify", str(path), "--repo", repository,
                "--signer-workflow", repository + "/.github/workflows/release.yml", "--signer-digest", source,
                "--source-digest", source, "--source-ref", "refs/heads/main", "--deny-self-hosted-runners", "--format", "json"],
                capture_output=True, text=True, check=True, timeout=180)
            if not json.loads(result.stdout):
                raise ValueError(f"No verified provenance for {name}")
            (scratch / (name + ".attestation.json")).write_text(result.stdout, encoding="utf-8")
        notes = release_notes((ROOT / "CHANGELOG.md").read_text(encoding="utf-8"), version)
        url = Publisher(GitHub(), repository, version, source, notes, files, scratch).publish()
        print(url)
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Release publication failed: {error}\n")


if __name__ == "__main__":
    main()
