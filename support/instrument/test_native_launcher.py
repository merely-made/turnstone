# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Regression controls for early LaunchServices return and exact app ownership."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('native_launcher',
    Path(__file__).resolve().parents[2] / 'scripts/run_macos_scenario.py')
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class LauncherTests(unittest.TestCase):
    def test_early_open_exit_keeps_bundle_until_observed_app_exits(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, scenario, output = root / 'binary', root / 'test.scn', root / 'receipt'
            binary.write_bytes(b'owned test executable')
            binary.chmod(0o755)
            scenario.write_text('log control\n')
            state = {'polls': 0}

            class EarlyOpen:
                returncode = 0
                def __init__(self, command, **kwargs):
                    state['executable'] = Path(command[-2]) / 'Contents/MacOS/product'
                def poll(self):
                    return 0
                def communicate(self, **kwargs):
                    return '', 'Unable to block on application\n'

            def processes(*args, **kwargs):
                executable = state['executable']
                self.assertTrue(executable.exists(), 'Bundle removed while app still needs it')
                state['polls'] += 1
                if state['polls'] == 1:
                    return f'47 {executable} --library {root / "owned.json"}\n'
                (output / 'scenario.done').write_text('RESULT ok\n')
                return ''

            argv = ['launcher', '--binary', str(binary), '--scenario', str(scenario),
                    '--prefix', 'TABARD', '--output', str(output)]
            with patch.object(launcher.sys, 'platform', 'darwin'), patch.object(launcher.sys, 'argv', argv), \
                    patch.object(launcher.os, 'access', return_value=True), \
                    patch.object(launcher.subprocess, 'Popen', EarlyOpen), \
                    patch.object(launcher.subprocess, 'check_output', processes), \
                    patch.object(launcher.time, 'sleep'):
                self.assertEqual(launcher.main(), 0)
            result = json.loads((output / 'process.json').read_text())
            self.assertTrue(result['success'])
            self.assertEqual(result['owned_pids_observed'], [47])
            self.assertEqual(result['owned_pids_remaining'], [])
            self.assertFalse(state['executable'].exists())

    def test_other_bundle_and_path_prefix_are_not_owned(self):
        executable = Path('/tmp/mesquite-native-owner/App.app/Contents/MacOS/product')
        self.assertTrue(launcher.owns_executable(str(executable) + ' --library own', executable))
        self.assertFalse(launcher.owns_executable(str(executable) + '-other', executable))
        self.assertFalse(launcher.owns_executable('/tmp/another/App.app/Contents/MacOS/product', executable))


if __name__ == '__main__':
    unittest.main()
