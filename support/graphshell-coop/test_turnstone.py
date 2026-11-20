# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0

import json
from pathlib import Path
import struct
import tempfile
import unittest
from types import SimpleNamespace

import native_machine
import run
from verify import never_ready, verify_graphics, verify_native_phase


class TurnstoneReceiptTests(unittest.TestCase):
    def test_negative_requires_timeout_while_still_unavailable(self):
        result = {
            'viewer': 'turnstone', 'ok': False, 'final': {'status': 'error', 'attached': ''},
            'errors': [
                'wait-coop timed out. Expected {"attached": "chosen", "status": "ready"}; '
                'observed [("attached", Some("")), ("status", Some("error"))]'
            ],
        }
        self.assertTrue(never_ready(result, 'chosen'))
        result['errors'][0] = result['errors'][0].replace('Some("error")', 'Some("ready")')
        self.assertFalse(never_ready(result, 'chosen'))
        result['errors'] = ['scoped click failed']
        self.assertFalse(never_ready(result, 'chosen'))

    def test_existing_graphshell_control_receipt_still_verifies(self):
        result = {
            'ok': True,
            'final': {'status': 'error', 'attached': '',
                      'await-failed': 'status expected "ready", was "error"'},
        }
        self.assertTrue(never_ready(result, 'chosen'))

    def test_graphics_requires_the_recorded_staged_runtime(self):
        runtime = {'ld-linux-x86-64.so.2': 'loader', 'libc.so.6': 'libc', 'libm.so.6': 'libm'}
        icd = '/usr/share/vulkan/icd.d/radeon_icd.json'
        machine = {
            'graphical_environment': {'VK_DRIVER_FILES': icd},
            'gpu': {'vendor': '0x1002', 'vulkan_library': 'libvulkan_radeon.so'},
            'native_loader': {'files': runtime},
        }
        graphics = {
            'VK_DRIVER_FILES': icd, 'render_nodes': [{'node': '/dev/dri/renderD128', 'vendor': '0x1002'}],
            'driver_libraries': ['/usr/lib/libvulkan_radeon.so'], 'loader_files': dict(runtime),
        }
        verify_graphics(graphics, machine)
        graphics['loader_files']['libc.so.6'] = 'different'
        with self.assertRaises(AssertionError):
            verify_graphics(graphics, machine)

    def test_capture_index_preserves_order_and_binds_native_observations(self):
        with tempfile.TemporaryDirectory() as directory:
            worker = native_machine.Machine.__new__(native_machine.Machine)
            worker.public = Path(directory)
            worker.phases = {'baseline': SimpleNamespace(pid=42)}
            phase = worker.phase_path('baseline')
            phase.mkdir(parents=True)
            scenario = run.baseline_script('chosen', 'private')
            (worker.public / 'baseline.scn').write_text(scenario)
            for stem, session in [('private-before', 'private'), ('existing-graph', 'chosen')]:
                (phase / (stem + '.png')).write_bytes(
                    b'\x89PNG\r\n\x1a\n\0\0\0\rIHDR' + struct.pack('>II', 800, 600)
                )
                fields = {'pid': '42', 'status': 'ready', 'attached': session}
                (phase / (stem + '.coop.json')).write_text(json.dumps(fields))
            (phase / 'scenario.done').write_text('RESULT ok\npresentation frames=5 captures=2 blank=0\n')
            result = worker.receipt('baseline')
            self.assertEqual([c['fields']['attached'] for c in result['captures']], ['private', 'chosen'])
            self.assertEqual(result['final']['attached'], 'chosen')
            verify_native_phase(worker.public, 'baseline', result)
            (phase / 'existing-graph.coop.json').write_text('{"attached":"private"}')
            with self.assertRaises(AssertionError):
                verify_native_phase(worker.public, 'baseline', result)


if __name__ == '__main__':
    unittest.main()
