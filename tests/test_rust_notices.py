# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Independent preservation checks for the official Rust component document."""
from collections import Counter
from html import unescape
from pathlib import Path
import re
import subprocess
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import notices


def document(body):
    return '<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Notices</title></head><body>' + body + '</body></html>'


def assert_blocks_preserved(test, source, rendered):
    # This oracle does not use HTMLParser or production extraction/rendering.
    blocks = [unescape(block) for block in re.findall(r'<pre(?:\s[^>]*)?>(.*?)</pre\s*>', source, re.S)]
    test.assertTrue(blocks)
    position = 0
    for block in blocks:
        at = rendered.find(block, position)
        test.assertGreaterEqual(at, 0, "decoded source legal block omitted, altered or reordered")
        position = at + len(block)
    return blocks


class RustNoticeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], text=True, encoding="utf-8").strip())
        cls.licenses = sysroot / "share" / "doc" / "rust" / "licenses"
        cls.source = (sysroot / "share" / "doc" / "rust" / "COPYRIGHT-library.html").read_bytes().decode("utf-8")
        cls.generated = notices.generate()
        cls.components = cls.generated.split("Complete official Rust standard-library component notices:\n\n", 1)[1].split("\nEnd of official Rust standard-library component notices.", 1)[0]

    def test_official_loader_rejects_empty_unrelated_or_missing_legal_material(self):
        for source in (document(''), document('<h1>Unrelated</h1><pre>Some text</pre>'),
                       document('<h2 id="in-tree-files">Files</h2><h2 id="out-of-tree-dependencies">Crates</h2>'),
                       document('<h2 id="in-tree-files">Files</h2><h2 id="out-of-tree-dependencies">Crates</h2><pre>   </pre>')):
            with self.assertRaisesRegex(ValueError, 'official sections or nonempty legal blocks'):
                notices.rust_component_notices(source)

    def test_generator_rejects_a_compiler_other_than_the_pinned_version(self):
        with mock.patch.object(notices.subprocess, 'check_output', return_value='rustc 0.0.0 (different toolchain)'):
            with self.assertRaisesRegex(ValueError, 'pinned toolchain'):
                notices.generate()

    def test_every_official_legal_block_reaches_generated_output_in_order(self):
        assert_blocks_preserved(self, self.source, self.components)
        self.assertIn(self.components, self.generated)

    def test_omission_and_alteration_of_a_real_block_fail_the_independent_oracle(self):
        blocks = assert_blocks_preserved(self, self.source, self.components)
        counts = Counter(blocks)
        block = next(block for block in reversed(blocks) if counts[block] == 1)
        for replacement in ("", "ALTERED LEGAL BLOCK"):
            with self.assertRaises(AssertionError):
                assert_blocks_preserved(self, self.source, self.components.replace(block, replacement, 1))

    def test_official_visible_text_headings_and_nonfragment_urls_survive(self):
        without_pre = re.sub(r'<pre(?:\s[^>]*)?>.*?</pre\s*>', '', self.source, flags=re.S)
        for chunk in re.split(r'<[^>]*>', without_pre):
            text = ' '.join(unescape(chunk).split())
            if text:
                self.assertIn(text, self.components)
        position = 0
        for href in re.findall(r'href="([^"]+)"', self.source):
            if not href.startswith('#'):
                displayed = '[' + unescape(href) + ']'
                at = self.components.find(displayed, position)
                self.assertGreaterEqual(at, 0, "source link omitted or reordered")
                position = at + len(displayed)
        for attribution in ('2014 Alex Crichton', '2019 The Crossbeam Project Developers',
                            '1991-2024 Unicode, Inc', '2016 The Fuchsia Authors'):
            self.assertIn(attribution, self.components)

    def test_in_tree_spdx_expressions_have_the_toolchains_own_license_texts(self):
        in_tree = self.source.split('<h2 id="in-tree-files">', 1)[1].split('<h2 id="out-of-tree-dependencies">', 1)[0]
        expressions = re.findall(r'<b>License:</b> (.*?)</p>', in_tree)
        identifiers = {token for expression in expressions for token in re.findall(r'[A-Za-z0-9.-]+', unescape(expression)) if token not in {'AND', 'OR', 'WITH'}}
        self.assertTrue(identifiers)
        self.assertTrue({identifier + '.txt' for identifier in identifiers}.issubset(notices.RUST_TEXTS))
        for identifier in identifiers:
            text = (self.licenses / (identifier + '.txt')).read_text(encoding='utf-8').replace('\r\n', '\n').strip('\n')
            self.assertTrue(text in self.generated, f'{identifier} toolchain license text omitted or altered')

    def test_inline_entities_links_and_legal_whitespace_are_preserved(self):
        legal = '\r\n  Permission <software> & authors\nline with two spaces  \n\tindented\n'
        source = document('<h3>crate&amp;name</h3><p><b>Copyright:</b> A &amp; B <a href="https://example.invalid/?a=1&amp;b=2">terms</a></p><pre>' + legal.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;') + '</pre>')
        rendered = notices.RustComponentNotices().render(source)
        self.assertIn(legal, rendered)
        self.assertIn('### crate&name', rendered)
        self.assertIn('Copyright: A & B terms [https://example.invalid/?a=1&b=2]', rendered)

    def test_semicolonless_and_literal_entity_like_text_is_not_changed(self):
        source = document('<p>Copyright AT&T and A & B and &notit</p><pre>AT&T &unknown &#65 &#x42 &ampersand</pre>')
        rendered = notices.RustComponentNotices().render(source)
        self.assertIn(unescape('Copyright AT&T and A & B and &notit'), rendered)
        self.assertIn(unescape('AT&T &unknown &#65 &#x42 &ampersand'), rendered)

    def test_exception_hierarchy_retains_file_associations(self):
        source = document('<div><p>File/Directory: library</p><p>Exceptions:</p><div><p>File/Directory: unicode</p><p>License: Unicode-3.0</p><div><p>File/Directory: mod.rs</p><p>License: MIT</p></div></div><div><p>File/Directory: backtrace</p><p>Copyright: Alex</p></div></div>')
        rendered = notices.RustComponentNotices().render(source)
        self.assertIn('  File/Directory: library\n  Exceptions:\n    File/Directory: unicode\n    License: Unicode-3.0\n      File/Directory: mod.rs\n      License: MIT\n    File/Directory: backtrace\n    Copyright: Alex\n', rendered)

    def test_unknown_nested_malformed_and_truncated_markup_is_rejected(self):
        for source in [document('<script>terms</script>'), document('<pre>one<p>two</p></pre>'),
                       document('<pre>one<!--lost-->two</pre>'), document('<pre>one<?lost?>two</pre>'),
                       document('<p>terms</b>'), document('<p/>'), document('<pre>terms'),
                       document('<p title="truncated'), document('<pre>raw < software</pre>'),
                       document('<p>terms</p>')[:-7], '<pre>terms</pre>',
                       '<!DOCTYPE html><html><head></head></html>',
                       '<!DOCTYPE html><html><body></body><head></head></html>']:
            with self.subTest(source=source), self.assertRaises(ValueError):
                notices.RustComponentNotices().render(source)


if __name__ == '__main__':
    unittest.main()
