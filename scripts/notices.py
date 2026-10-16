#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Generate THIRD-PARTY-NOTICES.txt from the locked dependency graph.

The file includes both products' normal-dependency closures across release targets,
including proc-macro crates and excluding build/dev-only crates. Complete
upstream Rust standard-library and Windows native-runtime bundles can cover
additional material not linked into a particular executable. Ordinary crate
texts are printed once; complete component bundles remain unfiltered.
Crates publishing no legal files need reviewed supplements. Linked libm source
permission notices are preserved. Reviewed material binds published source
identity and exact bytes; identity checks do not establish review quality.
"""

import argparse
import hashlib
from html import unescape
from html.parser import HTMLParser
import json
import os
import shutil
import tempfile
from pathlib import Path
import re
import subprocess
import sys

from release import ROOT, TARGETS, cargo_identities

OUTPUT = ROOT / "THIRD-PARTY-NOTICES.txt"
LICENSES = ROOT / "scripts" / "licenses"
NOTICE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")

RUST = """The executable includes the Rust standard library {version}.
Its complete official component notices follow, without filtering or deduplication,
rendered from share/doc/rust/COPYRIGHT-library.html in the pinned toolchain.
Source distribution:
https://static.rust-lang.org/dist/rust-docs-{version}-x86_64-unknown-linux-gnu.tar.xz
The toolchain's own in-tree license texts are included here as: {texts}.

Complete official Rust standard-library component notices:

{components}
End of official Rust standard-library component notices."""
RUST_TEXTS = ("MIT.txt", "Apache-2.0.txt", "BSD-2-Clause.txt", "Unicode-3.0.txt")


class RustComponentNotices(HTMLParser):
    """Render the official notice document without losing legal text or context."""
    TAGS = {"html", "head", "meta", "title", "body", "h1", "h2", "h3", "ul", "li",
            "p", "b", "a", "div", "code", "details", "summary", "pre"}
    BLOCKS = {"title", "h1", "h2", "h3", "ul", "li", "p", "div", "details", "summary", "pre"}

    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.stack = []
        self.seen = set()
        self.doctype = False
        self.output = []
        self.words = []
        self.depth = 0
        self.sections = []
        self.legal_blocks = []
        self.pre_nonempty = False

    def flush(self):
        text = " ".join("".join(self.words).split())
        if text:
            self.output.append("  " * self.depth + text + "\n")
        self.words = []

    def handle_decl(self, decl):
        if decl.lower() != "doctype html" or self.doctype or self.stack or self.seen:
            raise ValueError("unsupported Rust notice declaration")
        self.doctype = True

    def handle_starttag(self, tag, attrs):
        if tag not in self.TAGS or (self.stack and self.stack[-1][0] == "pre"):
            raise ValueError(f"unsupported Rust notice markup: {tag}")
        parent = self.stack[-1][0] if self.stack else None
        if tag in {"html", "head", "body"}:
            if tag in self.seen or parent != (None if tag == "html" else "html"):
                raise ValueError("malformed Rust notice document structure")
            if tag == "html" and not self.doctype:
                raise ValueError("Rust notice document lacks its doctype")
            if (tag == "head" and "body" in self.seen) or (tag == "body" and "head" not in self.seen):
                raise ValueError("Rust notice head/body order is malformed")
            self.seen.add(tag)
        elif not parent or (tag == "meta" and parent != "head"):
            raise ValueError("Rust notice markup outside document structure")
        if tag in self.BLOCKS:
            self.flush()
        if tag == "h2":
            self.sections.append(dict(attrs).get("id"))
        if tag == "pre":
            self.pre_nonempty = False
        if tag == "div":
            self.depth += 1
        if tag in {"h1", "h2", "h3"}:
            self.words.append("#" * int(tag[1]) + " ")
        if tag != "meta":
            self.stack.append((tag, dict(attrs).get("href") if tag == "a" else None))

    def handle_startendtag(self, tag, attrs):
        if tag != "meta":
            raise ValueError(f"unsupported self-closing Rust notice markup: {tag}")
        self.handle_starttag(tag, attrs)

    def handle_endtag(self, tag):
        if not self.stack or self.stack[-1][0] != tag:
            raise ValueError(f"mismatched Rust notice closing tag: {tag}")
        if tag == "pre":
            self.legal_blocks.append(self.pre_nonempty)
            self.output.append("\n")
        elif tag in self.BLOCKS:
            self.flush()
        _, href = self.stack.pop()
        if href and not href.startswith("#"):
            self.words.append(" [" + href + "]")
        if tag == "div":
            self.depth -= 1

    def append(self, text):
        if not self.stack:
            if text.strip():
                raise ValueError("Rust notice text outside document structure")
        elif self.stack[-1][0] == "pre":
            self.pre_nonempty |= bool(text.strip())
            self.output.append(text)
        else:
            self.words.append(text)

    def handle_data(self, data):
        if "<" in data:
            raise ValueError("unescaped or truncated Rust notice markup")
        self.append(data)

    def entity(self, prefix):
        # HTMLParser also reports references without a semicolon. Preserve that
        # distinction: adding one would change literal text such as AT&T.
        line, column = self.getpos()
        end = self.line_starts[line - 1] + column + len(prefix)
        raw = prefix + (";" if self.source[end:end + 1] == ";" else "")
        self.append(unescape(raw))

    def handle_entityref(self, name):
        self.entity("&" + name)

    def handle_charref(self, name):
        self.entity("&#" + name)

    def handle_comment(self, data):
        if self.stack and self.stack[-1][0] == "pre":
            raise ValueError("markup inside Rust notice legal block")

    def handle_pi(self, data):
        raise ValueError("unsupported Rust notice processing instruction")

    def unknown_decl(self, data):
        raise ValueError("unsupported Rust notice declaration")

    def render(self, source):
        self.source = source
        self.line_starts = [0] + [match.end() for match in re.finditer("\n", source)]
        self.feed(source)
        self.close()
        if self.stack or self.seen != {"html", "head", "body"}:
            raise ValueError("unclosed or incomplete Rust notice document")
        self.flush()
        return "".join(self.output)


def rust_component_notices(source):
    """Require an official component-document shape as well as valid markup."""
    parser = RustComponentNotices()
    rendered = parser.render(source)
    sections = ("in-tree-files", "out-of-tree-dependencies")
    if (any(parser.sections.count(section) != 1 for section in sections)
            or parser.sections.index(sections[0]) >= parser.sections.index(sections[1])
            or not parser.legal_blocks or not all(parser.legal_blocks)):
        raise ValueError("Rust component notice source lacks official sections or nonempty legal blocks")
    return rendered


WINDOWS_MATERIALS = {
    "mingw-COPYING", "mingw-COPYING-runtime.txt", "mingw-DISCLAIMER.PD",
    "mingw-pseudo-reloc-notice.txt", "llvm-libunwind-LICENSE.TXT",
    "mingw-complex-internal-notice.txt",
}


def windows_runtime_notices(channel):
    """The complete reviewed native material for the fixed Windows link route."""
    reviewed = json.loads((LICENSES / "windows-runtime.json").read_text(encoding="utf-8"))
    producer, materials = reviewed["producer"], reviewed["materials"]
    if producer["rust"] != channel:
        raise ValueError("Windows native runtime material requires review for this Rust version")
    if len(materials) != len(WINDOWS_MATERIALS) or {entry["file"] for entry in materials} != WINDOWS_MATERIALS:
        raise ValueError("Windows native runtime material is incomplete or unreviewed")
    body = [f"Windows executables use Rust {channel}'s bundled open-source MinGW-w64\n"
            "startup/helpers and static LLVM libunwind. UCRT implementations remain\n"
            "Windows OS DLLs and are not shipped in these archives. The package build\n"
            "checks actual bundled input identity and selected native members.\n"
            f"Producer sources: llvm-mingw {producer['llvm_mingw']}; MinGW-w64\n"
            f"{producer['mingw']}; LLVM {producer['llvm']}\n"
            f"({producer['llvm_commit']}).\n"
            "The complete upstream MinGW runtime notice bundle below also describes\n"
            "components beyond the reviewed startup/helper selection; it is retained\n"
            "without filtering. These are native runtime notices, not compiler-script\n"
            "licenses or a grant for proprietary Microsoft CRT/startup code."]
    for entry in materials:
        path = LICENSES / entry["file"]
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != entry["sha256"]:
            raise ValueError(f"{path} differs from its reviewed native notice bytes")
        body.append(f"--- {entry['file']} ---\nSource: {entry['origin']}\n\n" + raw.decode("utf-8"))
    return "\n\n".join(body)


def metadata(target):
    return json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target],
        cwd=ROOT, text=True, encoding="utf-8"))


def notice_packages(meta, target):
    """Retain normal material and distinguish Cargo's actual runtime closure.

    Metadata includes inactive optional edges. Cargo tree owns feature/target
    selection; no-proc-macro removes producer subtrees from the runtime query.
    Both queries select the same exact checked product IDs independently, and
    every returned package must map uniquely back to this metadata graph.
    """
    identities = cargo_identities(meta)
    packages = {}
    for package in meta["packages"]:
        packages.setdefault((package["name"], package["version"]), []).append(package)
    sets = {}
    for scope, edges in (("normal", "normal"), ("runtime", "normal,no-proc-macro")):
        selected = {}
        for identity in identities.values():
            command = ["cargo", "tree", "--locked", "--target", target, "--edges", edges,
                       "--prefix", "none", "--format", "{p}", "-p", identity["id"]]
            output = subprocess.check_output(command, cwd=ROOT, text=True, encoding="utf-8")
            for line in output.splitlines():
                if not line:
                    continue
                match = re.match(r"^(\S+) v(\S+)(?: |$)", line)
                if match is None or len(packages.get((match[1], match[2]), [])) != 1:
                    raise ValueError(f"Cargo {scope} dependency identity is absent or ambiguous: {line!r}")
                package = packages[(match[1], match[2])][0]
                selected[package["id"]] = package
        if not {identity["id"] for identity in identities.values()} <= selected.keys():
            raise ValueError(f"Cargo {scope} dependency output omits a checked product root")
        sets[scope] = selected
    if not sets["runtime"].keys() <= sets["normal"].keys():
        raise ValueError("Cargo runtime dependency output is not a subset of normal material")
    workspace = set(meta["workspace_members"])
    result = []
    for package in sets["normal"].values():
        if package["source"] is None and (package["id"] not in workspace or package["name"] in {"i-slint-backend-winit", "sctk-adwaita"}):
            if package["id"] in workspace:
                raise ValueError("vendored third-party producers must be excluded from workspace ownership, not omitted from notices")
            validate_patched_dependency(package)
        if package["id"] not in workspace:
            result.append(dict(package, notice_role=("runtime dependency" if package["id"] in sets["runtime"]
                                                    else "host code-generation material")))
    return result


def validate_patched_dependency(package):
    """Retain patched third-party roots and prove their exact reviewed diff/base."""
    records = json.loads((LICENSES / "patched-dependencies.json").read_text(encoding="utf-8"))
    key = package["name"] + "@" + package["version"]
    record = records.get(key)
    directory = Path(package["manifest_path"]).parent.resolve()
    if record is None or not directory.is_relative_to(ROOT.resolve()) or directory != (ROOT / record["path"]).resolve():
        raise ValueError(f"third-party path dependency {key} needs reviewed patch provenance")
    provenance_path = directory / "auditorylab-provenance.json"
    patch_path = directory / "auditorylab.patch"
    if (hashlib.sha256(provenance_path.read_bytes()).hexdigest() != record["provenance_sha256"]
            or hashlib.sha256(patch_path.read_bytes()).hexdigest() != record["patch_sha256"]):
        raise ValueError("patched third-party diff/provenance changed; independent review is required")
    provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    if (provenance["package"], provenance["version"], provenance["crate_checksum_sha256"], provenance["upstream_git_commit"]) != (
            package["name"], package["version"], record["upstream_crate_sha256"], record["upstream_git_commit"]):
        raise ValueError("patched dependency upstream identity differs from reviewed producer")
    original = provenance["upstream_files_sha256"]
    modified = set(record["modified_files"])
    if modified != set(provenance["modified_upstream_files"]) or modified != {"i-slint-backend-winit": {"Cargo.toml", "lib.rs"}, "sctk-adwaita": {"Cargo.toml"}}.get(package["name"]):
        raise ValueError("unsupported patched source scope")
    extras = {"AUDITORYLAB.md", "auditorylab.patch", "auditorylab-provenance.json"}
    if any(path.is_symlink() for path in directory.rglob("*")):
        raise ValueError("patched dependency source must not escape through symlinks")
    actual = {path.relative_to(directory).as_posix() for path in directory.rglob("*") if path.is_file()}
    expected = set(original) | extras
    if actual != expected:
        raise ValueError("patched dependency has missing or unreviewed added files: "
                         + "missing=" + repr(sorted(expected - actual)) + "; added=" + repr(sorted(actual - expected)))
    for name, digest in original.items():
        path = directory / name
        if path.is_symlink() or (name not in modified and hashlib.sha256(path.read_bytes()).hexdigest() != digest):
            raise ValueError("unreviewed patched dependency source/license change: " + name)
    normalized = []
    for line in patch_path.read_text(encoding="utf-8").splitlines(keepends=True):
        if line.startswith(("--- ", "+++ ")):
            prefix = "upstream/" if line.startswith("--- ") else record["path"] + "/"
            name = line[4:].rstrip("\n").removeprefix(prefix)
            if not line[4:].startswith(prefix) or name not in modified:
                raise ValueError("patch header escapes the reviewed source files")
            line = line[:4] + ("a/" if line.startswith("--- ") else "b/") + name + "\n"
        normalized.append(line)
    temporary = ROOT / "target/notices-temp"
    temporary.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=temporary) as scratch:
        scratch = Path(scratch)
        for name in modified:
            shutil.copyfile(directory / name, scratch / name)
        environment = os.environ.copy()
        for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"):
            environment.pop(name, None)
        environment["GIT_CEILING_DIRECTORIES"] = str(scratch)
        # Text-mode stdin rewrites LF on Windows and breaks exact patch context.
        result = subprocess.run(["git", "apply", "--reverse", "--whitespace=nowarn", "-"], cwd=scratch,
                                input="".join(normalized).encode("utf-8"), capture_output=True, env=environment)
        if result.returncode:
            raise ValueError("reviewed patch cannot reconstruct upstream source: "
                             + result.stderr.decode("utf-8", errors="backslashreplace"))
        if any(hashlib.sha256((scratch / name).read_bytes()).hexdigest() != original[name] for name in modified):
            raise ValueError("reversed patch does not match the reviewed original source")
    return record


def published_files(package):
    directory = Path(package["manifest_path"]).parent
    files = set()
    for path in directory.iterdir():
        if path.name.upper().startswith(NOTICE_PREFIXES):
            if path.is_file():
                files.add(path)
            elif path.is_dir():
                files.update(child for child in path.rglob("*") if child.is_file())
    if package.get("license_file"):
        files.add(directory / package["license_file"])
    return sorted(files)


def supplement(package, reviewed):
    """Reviewed license or additional source notices for a published package."""
    entry = reviewed.get(package["name"] + "@" + package["version"], reviewed.get(package["name"]))
    if entry is None:
        raise ValueError(f"{package['name']} {package['version']} lacks required reviewed notices; "
                         "review its upstream license and add it to scripts/licenses/supplements.json")
    if entry["license"] != package.get("license"):
        raise ValueError(f"{package['name']} {package['version']} now declares {package.get('license')!r}; "
                         "review the change and update its supplement")
    if not entry["files"]:
        raise ValueError(f"{package['name']} has no reviewed notice files")
    if package["name"] == "libm" and set(entry.get("sources", {})) != {
            f"src/math/{name}.rs" for name in
            ("sin", "cos", "k_sin", "k_cos", "rem_pio2", "rem_pio2_large", "pow", "log10")}:
        raise ValueError("libm is missing linked source-notice review inputs")
    directory = Path(package["manifest_path"]).parent
    vcs = json.loads((directory / ".cargo_vcs_info.json").read_text(encoding="utf-8"))["git"]["sha1"]
    if (entry["version"], entry["repository"], entry["vcs"]) != (package["version"], package.get("repository"), vcs):
        raise ValueError(f"{package['name']} differs from its reviewed published identity; review its supplement")
    for source, digest in entry.get("sources", {}).items():
        if hashlib.sha256((directory / source).read_bytes()).hexdigest() != digest:
            raise ValueError(f"{package['name']} {source} differs from its reviewed source")
    files = []
    for item in entry["files"]:
        path = LICENSES / item["file"]
        if hashlib.sha256(path.read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError(f"{path} differs from its reviewed bytes")
        files.append(path)
    return files


def framework_materials(meta):
    """Bind generated Slint widgets/assets separately from normal dependencies."""
    reviewed = json.loads((LICENSES / "slint-framework.json").read_text(encoding="utf-8"))
    packages = [package for package in meta["packages"] if package["name"] == reviewed["name"]]
    if len(packages) != 1 or packages[0]["version"] != reviewed["version"]:
        raise ValueError("Slint compiler asset producer needs a new source/license review")
    directory = Path(packages[0]["manifest_path"]).parent
    vcs = json.loads((directory / ".cargo_vcs_info.json").read_text(encoding="utf-8"))["git"]["sha1"]
    actual = {path.relative_to(directory).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
              for folder in ("widgets", "LICENSES") for path in (directory / folder).rglob("*") if path.is_file()}
    if vcs != reviewed["vcs"] or actual != reviewed["files"]:
        raise ValueError("Slint standard widgets/assets differ from reviewed source; review additions and notices")
    selected = directory / "LICENSES/LicenseRef-Slint-Royalty-free-2.0.md"
    if selected.read_bytes() != (LICENSES / "slint-royalty-free-2.0.md").read_bytes():
        raise ValueError("selected Slint royalty-free terms differ from reviewed material")
    for name, digest in reviewed["upstream_review"].items():
        path = LICENSES / ("slint-" + name)
        if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            raise ValueError("Slint upstream asset-license review materials differ")
    return directory, reviewed


def system_library_notices():
    """Retain external engine obligations separately from MIT binding crates."""
    reviewed = json.loads((LICENSES / "system-library-notices.json").read_text(encoding="utf-8"))
    if ({item["package"] for item in reviewed["materials"]} != {"libfreetype6", "libfontconfig1"}
            or len(reviewed["materials"]) != 2
            or reviewed["acknowledgment"] != "This software is based in part on the work of the FreeType Team."):
        raise ValueError("system font integration needs its reviewed engine acknowledgment and terms")
    output = [reviewed["acknowledgment"],
              "The Linux GUI links external system FreeType and Fontconfig. The FreeType\n"
              "engine integration selects the FreeType License (FTL). These are not the\n"
              "MIT Rust binding crates or bundled native implementations. Reference\n"
              "copyright/grant bundles below come from reviewed Ubuntu 22.04 producers.\n"
              "They also cover source/build/package material, not all linked into the\n"
              "application. Actual installed package versions are recorded separately\n"
              "during native qualification; keep system security updates installed."]
    for item in reviewed["materials"]:
        path = LICENSES / item["file"]
        content = path.read_bytes()
        if hashlib.sha256(content).hexdigest() != item["sha256"]:
            raise ValueError("reviewed system library copyright/grant material differs: " + path.name)
        output.append(f"Reference producer: {item['package']} {item['version']} ({item['architecture']})\n"
                      f"Distribution: {reviewed['distribution']}\n"
                      f"Source: {item['source']}\nOriginal location: {item['origin']}\n\n"
                      + content.decode("utf-8"))
    return "\n\n".join(output)


def generate():
    channel = re.search(r'^channel = "([^"]+)"', (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)[1]
    version = subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True, encoding="utf-8").split()[1]
    if version != channel:
        raise ValueError(f"Rust {version} does not match the pinned toolchain {channel}")
    reviewed = json.loads((LICENSES / "supplements.json").read_text(encoding="utf-8"))
    crates = {}
    framework = None
    for target in TARGETS:
        graph = metadata(target["target"])
        if framework is None:
            framework = framework_materials(graph)
        for package in notice_packages(graph, target["target"]):
            key = (package["name"], package["version"])
            stored, systems = crates.setdefault(key, (package, set()))
            systems.add(target["platform"].split("-")[0])
            if package["notice_role"] == "runtime dependency":
                stored["notice_role"] = "runtime dependency"
    texts = {}
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], cwd=ROOT, text=True, encoding="utf-8").strip())
    components = rust_component_notices(
        (sysroot / "share" / "doc" / "rust" / "COPYRIGHT-library.html").read_bytes().decode("utf-8"))
    rust = []
    for name in RUST_TEXTS:
        text = (sysroot / "share" / "doc" / "rust" / "licenses" / name).read_text(encoding="utf-8")
        number = texts.setdefault(text.replace("\r\n", "\n").strip("\n"), len(texts) + 1)
        rust.append(f"[{number}] {name}")
    entries = []
    for (name, version), (package, systems) in sorted(crates.items()):
        files = published_files(package)
        entry = reviewed.get(name + "@" + version, reviewed.get(name, {}))
        if not files or entry.get("additional_material", False):
            files += supplement(package, reviewed)
        numbers = []
        for path in files:
            text = path.read_text(encoding="utf-8").replace("\r\n", "\n").strip("\n")
            number = texts.setdefault(text, len(texts) + 1)
            numbers.append(f"[{number}] {path.name}")
        where = "all platforms" if len(systems) == 3 else ", ".join(sorted(systems))
        provenance = ""
        if package.get("source") is None:
            patch = validate_patched_dependency(package)
            provenance = (f"  Modified source: {patch['path']}; upstream crate SHA-256 "
                          f"{patch['upstream_crate_sha256']}\n"
                          f"  Upstream commit: {patch['upstream_git_commit']}\n"
                          f"  Reviewed patch SHA-256: {patch['patch_sha256']}\n")
        entries.append(f"{name} {version}\n  License: {package.get('license') or 'see files'}\n"
                       f"  Source: {package.get('repository') or 'https://crates.io/crates/' + name}\n"
                       f"  Scope: {package['notice_role']}\n"
                       f"  Selected for: {where}\n{provenance}  Texts: {'; '.join(numbers)}")
    compiler, framework_review = framework
    asset_texts = []
    for path in sorted((compiler / "LICENSES").iterdir()) + [LICENSES / "slint-CC-BY-SA-4.0.txt"]:
        text = path.read_text(encoding="utf-8").replace("\r\n", "\n").strip("\n")
        number = texts.setdefault(text, len(texts) + 1)
        asset_texts.append(f"[{number}] {path.name}")
    body = ["Third-party notices for Auditory Lab",
            "This file combines both products’ normal-dependency union across release targets with\n"
            "complete upstream Rust standard-library and Windows native-runtime notice\n"
            "bundles. Those bundles can cover additional build/platform material not\n"
            "linked into a particular executable. Ordinary crate license/notice texts\n"
            "are numbered and printed once; complete upstream bundles are not filtered.",
            "== Rust standard library ==\n" + RUST.format(version=channel, texts="; ".join(rust), components=components),
            "== Windows native startup and runtime support ==\n" + windows_runtime_notices(channel),
            "== Slint desktop integration and generated assets ==\n"
            "The desktop product selects the Royalty-free Desktop, Mobile, and Web\n"
            "Applications License 2.0 for Slint; Auditory Lab source remains MPL-2.0.\n"
            "Attribution is the standard AboutSlint widget in Help -> About. This is\n"
            "a desktop integration, not standalone Slint, embedded use or an exposed\n"
            "Slint API. The retained upstream terms govern this dependency.\n"
            "Cargo's runtime normal closure excludes proc-macro producer subtrees. The\n"
            "crate list separately retains host code-generation material and its grants;\n"
            "that role does not deny obligations for emitted code/resources. Explicit\n"
            "build/dev-only edges are excluded. Runtime scope means selected by at least\n"
            "one product/target; platform lists describe retained normal material.\n"
            "Generated widgets/icons/logo come from\n"
            f"i-slint-compiler {framework_review['version']} ({framework_review['vcs']}).\n"
            "All compiler widget/resource and license inputs are identity-checked;\n"
            "Material Icons (Apache-2.0), Copyright © Microsoft Corporation Fluent icons\n"
            "(MIT), Cosmic Icons by System76 (CC-BY-SA-4.0), and Copyright © SixtyFPS\n"
            "GmbH <info@slint.dev> logo (CC-BY-ND-4.0) grants\n"
            "are retained with the upstream compiler license set. The logo is unmodified;\n"
            "their license set is retained as: " + "; ".join(asset_texts) + ".\n"
            "System fonts are resolved at runtime; no project font/image is bundled.\n"
            "Source: https://github.com/slint-ui/slint/tree/v1.18.1/internal/compiler/widgets",
            "== Linux system font integration ==\n"
            "The GUI selects system FreeType and Fontconfig through the supported root\n"
            "Cargo links overrides. freetype-sys build.rs is skipped, so its bundled\n"
            "FreeType native implementation is not compiled or shipped. The MIT crate\n"
            "entry covers its Rust bindings, not the uncompiled native C corpus.\n"
            "Use security-updated distribution libraries. SCTK source retains an inactive\n"
            "Cantarell font with its original copyright and full SIL OFL terms below;\n"
            "the selected crossfont title renderer uses system fonts.\n",
            "== External Linux system font libraries ==\n" + system_library_notices(),
            "== Crates ==\n" + "\n\n".join(entries),
            "== License and notice texts =="]
    for text, number in texts.items():
        body.append(f"[{number}]\n{text}")
    return "\n\n".join(body) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if the committed file is not current")
    args = parser.parse_args()
    try:
        text = generate()
        if args.check:
            if not OUTPUT.is_file() or OUTPUT.read_text(encoding="utf-8") != text:
                sys.exit("notices: THIRD-PARTY-NOTICES.txt is not current; run python3 scripts/notices.py")
        else:
            OUTPUT.write_text(text, encoding="utf-8")
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"notices: {error}")


if __name__ == "__main__":
    main()
