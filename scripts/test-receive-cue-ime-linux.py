#!/usr/bin/env python3
"""Guard the native-event evidence boundary of the physical IME collector."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("ime_receiving", Path(__file__).with_name("receive-cue-ime-linux.py"))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class EvidenceBoundary(unittest.TestCase):
    def test_collector_reads_tracing_stdout_separately_from_wayland_stderr(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prefix = "before stage\n"
            event = 'DEBUG turnstone::ime: native IME event window=0 kind="commit" utf8_bytes=6\n'
            (root / "stdout.log").write_text(prefix + event)
            (root / "stderr.log").write_text('zwp_text_input_v3#20.commit()\n')
            self.assertEqual(driver.native_events(driver.read_native_trace(root, len(prefix))), [
                {"window": 0, "kind": "commit", "utf8_bytes": 6},
            ])

    def test_delivery_or_wayland_protocol_commit_is_not_native_ime_commit(self):
        self.assertEqual(driver.native_events(
            'DEBUG turnstone::ime: IME delivery window=0 handled=true\n'
            '[01:02:03] -> zwp_text_input_v3#20.commit()\n'
            'scenario: insert hello\n'), [])

    def test_native_events_preserve_empty_preedit_and_window_identity(self):
        self.assertEqual(driver.native_events(
            'DEBUG turnstone::ime: native IME event window=1 kind="preedit" utf8_bytes=6 selection=Some((0, 6))\n'
            'DEBUG turnstone::ime: native IME event window=1 kind="preedit" utf8_bytes=0 selection=None\n'
            'DEBUG turnstone::ime: native IME event window=0 kind="commit" utf8_bytes=6 selection=None\n'), [
                {"window": 1, "kind": "preedit", "utf8_bytes": 6},
                {"window": 1, "kind": "preedit", "utf8_bytes": 0},
                {"window": 0, "kind": "commit", "utf8_bytes": 6},
            ])

    def test_composition_scenario_has_no_text_injection(self):
        for name in ("cue_ime_linux.scn", "cue_ime_sky_linux.scn"):
            with self.subTest(scenario=name):
                scenario = Path(__file__).resolve().parent.parent / "scenarios" / name
                verbs = [line.split()[0] for line in scenario.read_text().splitlines()
                         if line.strip() and not line.lstrip().startswith("#")]
                self.assertFalse(set(verbs) & {"type", "insert", "input-text"})


if __name__ == "__main__":
    unittest.main()
