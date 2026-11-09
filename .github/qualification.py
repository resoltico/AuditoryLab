"""Fail closed on required CI outcomes; ordinary unchanged-core mutations are optional."""
import json
import os
import sys


def require(results, release, core_changed):
    expected = {'lint', 'domain', 'matrix', 'icons', 'native', 'package', 'package-macos', 'mutation', 'native-graphical-linux', 'native-graphical-windows'}
    if set(results) != expected:
        raise ValueError('required job set differs from qualification contract')
    for name, job in results.items():
        optional = name == 'mutation' and not release and not core_changed
        if job['result'] != 'success' and not (optional and job['result'] == 'skipped'):
            raise ValueError(f"{name}: required result is {job['result']}")


def test():
    import unittest
    from pathlib import Path

    class Qualification(unittest.TestCase):
        def test_every_required_failure_and_skip_blocks(self):
            names = ['lint', 'domain', 'matrix', 'icons', 'native', 'package', 'package-macos', 'mutation', 'native-graphical-linux', 'native-graphical-windows']
            good = {name: {'result': 'success'} for name in names}
            require(good, True, True)
            for name in names:
                for state in ['failure', 'cancelled', 'skipped']:
                    with self.subTest(name=name, state=state), self.assertRaises(ValueError):
                        require({**good, name: {'result': state}}, True, True)
            optional = {**good, 'mutation': {'result': 'skipped'}}
            require(optional, False, False)
            with self.assertRaises(ValueError):
                require(optional, False, True)
            with self.assertRaises(ValueError):
                require({}, True, True)

        def test_icon_failure_is_local_to_macos_packaging_and_blocks_release(self):
            # Inspect the actual declared dependency edges, not a second graph.
            text = Path('.github/workflows/ci.yml').read_text()
            import re
            blocks = dict(re.findall(r'^  ([a-z-]+):\n(.*?)(?=^  [a-z-]+:|\Z)', text, re.M | re.S))
            for name in ['native', 'package', 'mutation', 'lint', 'domain']:
                needs = re.search(r'^    needs: (.+)$', blocks[name], re.M)
                self.assertNotIn('icons', needs[1] if needs else '')
            self.assertIn('needs: [matrix, icons]', blocks['package-macos'])
            # Simulate GitHub's failed prerequisite -> skipped consumer result.
            result = {name: {'result': 'success'} for name in ['lint', 'domain', 'matrix', 'icons', 'native', 'package', 'package-macos', 'mutation', 'native-graphical-linux', 'native-graphical-windows']}
            result['icons']['result'] = 'failure'
            result['package-macos']['result'] = 'skipped'
            with self.assertRaisesRegex(ValueError, 'icons'):
                require(result, True, True)
            self.assertIn('needs: [lint, domain, matrix, icons, native, package, package-macos, mutation, native-graphical-linux, native-graphical-windows]', blocks['required'])
            self.assertIn('needs: [check, build]', Path('.github/workflows/release.yml').read_text())

    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Qualification)
    if not unittest.TextTestRunner().run(suite).wasSuccessful():
        raise SystemExit(1)


if __name__ == '__main__':
    if sys.argv[1:] == ['--test']:
        test()
    elif not sys.argv[1:]:
        require(json.loads(os.environ['RESULTS']), os.environ['RELEASE'] == 'true', os.environ['CORE_CHANGED'] == 'true')
    else:
        raise SystemExit('usage: qualification.py [--test]')
