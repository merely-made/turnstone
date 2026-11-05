# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Controls for source identity, fresh outputs and non-retroactive close claims."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from common import fresh_output, verify_sources, verify_release, copy_tree, console_unlocked


class Paths(unittest.TestCase):
    def test_collector_uses_separate_inputs_and_refuses_old_close_claim(self):
        from PIL import Image
        repo = Path(__file__).resolve().parents[2]
        revision = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package, browser, native = (root / n for n in ('package', 'browser', 'native'))
            for p in (package, browser, native):
                p.mkdir()
            def write(path, value):
                path.write_text(json.dumps(value), encoding='utf-8')
            def digest(path):
                return hashlib.sha256(path.read_bytes()).hexdigest()
            binary = package / 'unit-control-binary'
            binary.write_bytes(b'unit control; never executed')
            source = {'source': revision, 'build_source': revision, 'compiler': 'unit-control',
                      'source_sha256': {'Cargo.toml': digest(repo / 'Cargo.toml')}}
            for case in ('instrument', 'usability'):
                scenario = f'ports/tabard/desktop/scenarios/{case}.scn'
                source['source_sha256'][scenario] = digest(repo / scenario)
            assets = {}
            for name in ('workspace.html', 'app.js', 'app.wasm'):
                (package / name).write_bytes(b'unit control; never executed')
                assets[name] = digest(package / name)
            write(package / 'candidate.json', {**source, 'package_sha256': assets})
            write(package / 'native-candidate.json', {**source, 'binary': str(binary), 'binary_sha256': digest(binary)})
            for directory, name in ((browser, 'candidate.json'), (native, 'native-candidate.json')):
                write(directory / 'input.json', {'source': revision, 'candidate_sha256': digest(package / name)})
            image = Image.new('RGB', (1800, 600))
            for x in range(256):
                image.putpixel((x, 1), (x, 0, 0))
            runs = ['wide-entry', 'forme-original', 'forme-reopen', 'open-reading',
                    'light-workbench', 'narrow-workbench', 'narrow-graph', 'commands-focus']
            write(browser / 'summary.json', {'source': revision, 'headed': True,
                  'page_errors': 0, 'runs': runs, 'adapter': {'vendor': 'amd', 'architecture': 'gcn-5'}})
            release = {'schema': 'instrument-release/v2', 'context_closed': True,
                       'browser_closed': True, 'http_closed': True, 'close_errors': []}
            write(browser / 'release.json', release)
            runtime = {'passed': True, 'owned_pids_remaining': [], 'owned_pids_observed': [47],
                       'temporary_profile_removed': True, 'http_port_rebound': True,
                       'spawned': [{'exit_observed': True}]}
            write(browser / 'runtime.json', runtime)
            for run in runs:
                write(browser / (run + '.json'), {'scenario': {'result': {'result': 'ok'},
                      'gate_failures': []}, 'errors': [], 'hidden': False})
                image.save(browser / (run + '.png'))
            for label in ('native-instrument', 'native-usability'):
                folder = native / label
                folder.mkdir()
                write(folder / 'bundle-release.json', {'removed': True, 'remaining': [], 'retained_bundle': None})
                scenario = f'ports/tabard/desktop/scenarios/{label.removeprefix("native-")}.scn'
                write(folder / 'process.json', {'success': True, 'timeout': False,
                      'binary_sha256': digest(binary), 'scenario_sha256': source['source_sha256'][scenario],
                      'owned_pids_observed': [48], 'owned_pids_remaining': []})
                count = 2 if label == 'native-instrument' else 1
                (folder / 'scenario.done').write_text(f'RESULT ok\nframes: {count} captured, 0 blank, {count} distinct\npresentation: 1 frames, 1 redraws, 0 unpresented\n')
                if count == 2:
                    image.save(folder / 'instrument_light.png')
                    dark = image.copy()
                    dark.putpixel((1700, 500), (1, 1, 1))
                    dark.save(folder / 'instrument_dark.png')
                else:
                    image.save(folder / 'usability.png')
            write(native / 'native-release.json', {'inhibitor_closed': True, 'failure': None,
                  'runs': [{'label': label, 'exit': 0} for label in ('native-instrument', 'native-usability')]})
            command = [sys.executable, str(Path(__file__).with_name('collect.py')), '--repo', str(repo),
                       '--package', str(package), '--browser', str(browser), '--native', str(native)]
            result = subprocess.run(command + ['--output', str(root / 'collected')], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(len(json.loads((root / 'collected/qualification.json').read_text())['artifacts']), 11)
            self.assertEqual((root / 'collected/browser/wide-entry.png').read_bytes(), (browser / 'wide-entry.png').read_bytes())
            write(browser / 'release.json', {'browser_closed': True, 'http_closed': True})
            result = subprocess.run(command + ['--output', str(root / 'rejected')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'closure is unverified', result.stderr)
            self.assertFalse((root / 'rejected').exists())
            write(browser / 'release.json', release)
            write(browser / 'runtime.json', {**runtime, 'owned_pids_remaining': [47]})
            result = subprocess.run(command + ['--output', str(root / 'live-process')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / 'live-process').exists())

    def test_macos_console_plists_preserve_locked_and_unknown_refusals(self):
        self.assertTrue(console_unlocked([{'IOConsoleUsers': [{'CGSSessionScreenIsLocked': False}]}]))
        self.assertTrue(console_unlocked({'IOConsoleLocked': False}))
        self.assertFalse(console_unlocked([{'IOConsoleUsers': [{'CGSSessionScreenIsLocked': True}]}]))
        with self.assertRaises(ValueError):
            console_unlocked({})

    def test_existing_receipt_is_never_reused(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            receipt = root / 'receipt'
            receipt.mkdir()
            frozen = receipt / 'old.json'
            frozen.write_bytes(b'frozen')
            with self.assertRaises(FileExistsError):
                fresh_output(receipt, root / 'source')
            self.assertEqual(frozen.read_bytes(), b'frozen')

    def test_source_input_and_enclosing_output_are_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for output in (root / 'source/new', root / 'package/new', root):
                with self.subTest(output=output), self.assertRaises(ValueError):
                    fresh_output(output, root / 'source', root / 'package')

    def test_sources_are_bound_to_explicit_repo(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            (repo / 'input.rs').write_bytes(b'correct')
            manifest = {'source_sha256': {'input.rs': hashlib.sha256(b'correct').hexdigest()}}
            verify_sources(repo, manifest)
            (repo / 'input.rs').write_bytes(b'drift')
            with self.assertRaises(ValueError):
                verify_sources(repo, manifest)
            with self.assertRaises(ValueError):
                verify_sources(repo, {'source_sha256': {'../foreign.rs': '0' * 64}})
            with self.assertRaises(ValueError):
                verify_sources(repo, {'source_sha256': {}})

    def test_copy_budget_is_checked_before_creating_destination(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'source'
            source.mkdir()
            (source / 'input').write_bytes(b'1234')
            with self.assertRaises(ValueError):
                copy_tree(source, root / 'dest', 3)
            self.assertFalse((root / 'dest').exists())

    def test_old_unconditional_close_booleans_do_not_qualify(self):
        with self.assertRaises(ValueError):
            verify_release({'browser_closed': True, 'http_closed': True})
        record = {'schema': 'instrument-release/v2', 'context_closed': True,
                  'browser_closed': True, 'http_closed': True, 'close_errors': []}
        verify_release(record)
        for key in ('context_closed', 'browser_closed', 'http_closed'):
            with self.subTest(key=key), self.assertRaises(ValueError):
                verify_release({**record, key: False})
        with self.assertRaises(ValueError):
            verify_release({**record, 'close_errors': ['failed']})


if __name__ == '__main__':
    unittest.main()
