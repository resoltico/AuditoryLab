#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Generate THIRD-PARTY-NOTICES.txt from the locked dependency graph.

The file includes the CLI normal-dependency closure across release targets,
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
from pathlib import Path
import re
import subprocess
import sys

from release import ROOT, TARGETS

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


def linked_packages(meta):
    """The registry packages in the CLI's normal-dependency closure."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    packages = {package["id"]: package for package in meta["packages"]}
    pending = [p["id"] for p in meta["packages"] if p["name"] == "auditory-lab" and p["source"] is None]
    seen = set()
    while pending:
        current = pending.pop()
        if current not in seen:
            seen.add(current)
            pending.extend(dep["pkg"] for dep in nodes[current]["deps"]
                           if any(kind["kind"] is None for kind in dep["dep_kinds"]))
    return [packages[i] for i in seen if packages[i]["source"] is not None]


def published_files(package):
    directory = Path(package["manifest_path"]).parent
    files = {path for path in directory.iterdir()
             if path.is_file() and path.name.upper().startswith(NOTICE_PREFIXES)}
    if package.get("license_file"):
        files.add(directory / package["license_file"])
    return sorted(files)


def supplement(package, reviewed):
    """Reviewed license or additional source notices for a published package."""
    entry = reviewed.get(package["name"])
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


def generate():
    channel = re.search(r'^channel = "([^"]+)"', (ROOT / "rust-toolchain.toml").read_text(encoding="utf-8"), re.M)[1]
    version = subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True, encoding="utf-8").split()[1]
    if version != channel:
        raise ValueError(f"Rust {version} does not match the pinned toolchain {channel}")
    reviewed = json.loads((LICENSES / "supplements.json").read_text(encoding="utf-8"))
    crates = {}
    for target in TARGETS:
        for package in linked_packages(metadata(target["target"])):
            key = (package["name"], package["version"])
            crates.setdefault(key, (package, set()))[1].add(target["platform"].split("-")[0])
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
        if not files or name == "libm":
            files += supplement(package, reviewed)
        numbers = []
        for path in files:
            text = path.read_text(encoding="utf-8").replace("\r\n", "\n").strip("\n")
            number = texts.setdefault(text, len(texts) + 1)
            numbers.append(f"[{number}] {path.name}")
        where = "all platforms" if len(systems) == 3 else ", ".join(sorted(systems))
        entries.append(f"{name} {version}\n  License: {package.get('license') or 'see files'}\n"
                       f"  Source: {package.get('repository') or 'https://crates.io/crates/' + name}\n"
                       f"  Used on: {where}\n  Texts: {'; '.join(numbers)}")
    body = ["Third-party notices for Auditory Lab",
            "This file combines the CLI dependency union across release targets with\n"
            "complete upstream Rust standard-library and Windows native-runtime notice\n"
            "bundles. Those bundles can cover additional build/platform material not\n"
            "linked into a particular executable. Ordinary crate license/notice texts\n"
            "are numbered and printed once; complete upstream bundles are not filtered.",
            "== Rust standard library ==\n" + RUST.format(version=channel, texts="; ".join(rust), components=components),
            "== Windows native startup and runtime support ==\n" + windows_runtime_notices(channel),
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
