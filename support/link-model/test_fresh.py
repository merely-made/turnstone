# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Admission controls for new qualifications; old records keep their own rules."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
import fresh


class Fresh(unittest.TestCase):
    def test_setup_and_executed_scenario_cannot_be_substituted(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            folder = root / 'browser-qualified/example'
            folder.mkdir(parents=True)
            source = root / 'site/ports/graphshell/web/scenarios/example.scn'
            source.parent.mkdir(parents=True)
            source.write_bytes(b'original assertions\n')
            setup = {'script': fresh.BROWSER_SETUP,
                     'sha256': hashlib.sha256(fresh.BROWSER_SETUP.encode()).hexdigest(),
                     'scenario': {'state': 'ok', 'errors': [], 'gate_failures': [],
                                  'captures': [], 'result': {'result': 'ok'}}}
            (folder / 'setup.json').write_text(json.dumps(setup))
            result = {'executed_scenario_sha256': fresh.check.sha256_file(source)}
            fresh.verify_browser_setup(root, {'name': 'example'}, result)
            with self.assertRaisesRegex(fresh.check.VerificationError, 'Inspector setup'):
                fresh.verify_browser_setup(root, {'name': 'example'}, result, 'topology')
            with self.assertRaisesRegex(fresh.check.VerificationError, 'scenario bytes'):
                fresh.verify_browser_setup(root, {'name': 'example'}, {})
            setup['scenario']['result']['result'] = 'fail'
            (folder / 'setup.json').write_text(json.dumps(setup))
            with self.assertRaisesRegex(fresh.check.VerificationError, 'Inspector setup'):
                fresh.verify_browser_setup(root, {'name': 'example'}, result)

    def test_canvas_feature_and_complete_targets_cannot_be_omitted(self):
        valid = ['cargo', '+1.99.1', 'test', '--locked', '-p', 'pictograph', '--features', 'canvas', '-j', '2', '--', '--test-threads=1']
        fresh.complete_test_scope(valid, 'canvas', 'pictograph')
        for invalid in (valid[:-2] + ['--lib', *valid[-2:]],
                        [p for p in valid if p != 'canvas'],
                        valid[:-2] + ['one_test', *valid[-2:]],
                        valid[:-1] + ['one_test']):
            with self.subTest(invalid=invalid), self.assertRaises(fresh.check.VerificationError):
                fresh.complete_test_scope(invalid, 'canvas', 'pictograph')

    def test_locked_is_allowed_but_not_required(self):
        for command in (['cargo', 'test', '-p', 'seiche', '--', '--test-threads=1'],
                        ['cargo', 'test', '--locked', '-p', 'seiche', '--', '--test-threads=1']):
            fresh.complete_test_scope(command, 'engine', 'seiche')
        with self.assertRaises(fresh.check.VerificationError):
            fresh.complete_test_scope(['cargo', 'test', '-p', 'other', '--', '--test-threads=1'], 'engine', 'seiche')

    def test_graphshell_native_and_partial_engine_scopes_are_not_web_proof(self):
        command = ['cargo', 'test', '--locked', '-p', 'graphshell', '--no-default-features', '--features', 'web', '--lib', '--', '--test-threads=1']
        fresh.complete_test_scope(command, 'graphshell', 'graphshell')
        with self.assertRaises(fresh.check.VerificationError):
            fresh.complete_test_scope([p for p in command if p != '--no-default-features'], 'graphshell', 'graphshell')
        with self.assertRaises(fresh.check.VerificationError):
            fresh.complete_test_scope(['cargo', 'test', '--locked', '-p', 'seiche', '--lib', '--', '--test-threads=1'], 'engine', 'seiche')

    def test_listing_is_separate_from_execution_and_has_no_name_filter(self):
        prefix = ['cargo', 'test', '--locked', '-p', 'seiche', '--']
        fresh.complete_test_scope(prefix + ['--list', '--format', 'terse'], 'engine', 'seiche', False)
        fresh.complete_test_scope(prefix + ['--list', '--ignored', '--format', 'terse'], 'engine', 'seiche', True)
        with self.assertRaises(fresh.check.VerificationError):
            fresh.complete_test_scope(prefix + ['only_one', '--list', '--format', 'terse'], 'engine', 'seiche', False)

    def test_descriptor_cannot_replace_a_scenario_or_its_capture_set(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            data = {'schema': 'fresh-link-family/v1', 'family': 'ordinary',
                    'scenarios': fresh.definition('ordinary')}
            data['scenarios'][0]['captures'] = []
            (root / 'fresh.json').write_text(json.dumps(data))
            with self.assertRaisesRegex(fresh.check.VerificationError, 'definition changed'):
                fresh.verify(root, root)

    def test_inventory_cannot_disagree_with_the_actual_listing_log(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            def record(name, text, ignored=False):
                path = root / (name + '.json')
                path.with_suffix('.log').write_bytes(text.encode())
                command = ['cargo', 'test', '--locked', '-p', 'seiche', '--', '--list']
                if ignored:
                    command += ['--ignored']
                command += ['--format', 'terse']
                path.write_text(json.dumps({'command': command, 'exit_code': 0,
                    'revision': '1' * 40, 'cwd': '/source', 'started': 'start', 'finished': 'finish',
                    'status_before': '', 'log_sha256': hashlib.sha256(text.encode()).hexdigest()}))
            record('all', 'first: test\nsecond: test\n')
            record('ignored', 'second: test\n', True)
            inventory = {'package': 'seiche', 'tests': ['first', 'second'], 'ignored': ['second'],
                         'list_record': 'all.json', 'ignored_record': 'ignored.json'}
            self.assertEqual(fresh.test_inventory(root, '1' * 40, 'engine', inventory), (2, 1))
            inventory['tests'] = ['first']
            with self.assertRaisesRegex(fresh.check.VerificationError, 'listing log'):
                fresh.test_inventory(root, '1' * 40, 'engine', inventory)


if __name__ == '__main__':
    unittest.main()
