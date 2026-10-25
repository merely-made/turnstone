#!/usr/bin/env python3
"""Receiving guards must reject the wrong frame direction and sequential mixing."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("receiving", Path(__file__).with_name("receive-cue-ui-linux.py"))
receiving = importlib.util.module_from_spec(spec)
spec.loader.exec_module(receiving)

def entry(direction, sampled=1, surface=1):
    return {"surface":surface,"direction": direction, "scope": "1:1", "sampled_layers": sampled}

class Directions(unittest.TestCase):
    def test_native_case_rejects_retained_content_and_unsampled_native_content(self):
        for entries in [[entry("host-device")], [entry("native-frame", 0)]]:
            with self.assertRaises(AssertionError):
                receiving.require_directions({"frame":entries}, "native-frame")
        receiving.require_directions({"frame":[entry("native-frame")]}, "native-frame")

    def test_mixed_case_requires_both_directions_in_the_same_checkpoint(self):
        with self.assertRaises(AssertionError):
            receiving.require_directions({"first":[entry("native-frame")], "later":[entry("host-device")]}, "mixed")
        receiving.require_directions({"frame":[entry("native-frame"),entry("host-device")]}, "mixed")

    def test_cached_entries_require_actual_source_rasters(self):
        frames = {"frame":[entry("native-frame", surface=1), entry("host-device", surface=2)]}
        with self.assertRaises(AssertionError):
            receiving.require_directions(frames, "mixed", {"frame":{2}})
        receiving.require_directions(frames, "mixed", {"frame":{1,2}})

if __name__ == "__main__":
    unittest.main()
