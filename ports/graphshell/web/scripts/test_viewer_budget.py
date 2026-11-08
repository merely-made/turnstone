# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""The delivery gate must reject incompressible oversized payloads."""

import importlib.util
from pathlib import Path
import random
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("build_viewer", Path(__file__).with_name("build-viewer.py"))
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class DeliveryBudget(unittest.TestCase):
    def test_compressed_budget_not_raw_length(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload"
            path.write_bytes(b"\0" * (builder.CAPS["wasm"] + 4096))
            rows = builder.measure_assets({"wasm": path})
            self.assertGreater(rows["wasm"]["raw_bytes"], builder.CAPS["wasm"])
            builder.require_budget(rows)

    def test_oversized_incompressible_payload_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload"
            path.write_bytes(random.Random(157).randbytes(builder.CAPS["wasm"] + 4096))
            rows = builder.measure_assets({"wasm": path})
            with self.assertRaisesRegex(RuntimeError, f"wasm: gzip6=.*cap={builder.CAPS['wasm']}"):
                builder.require_budget(rows)

    def test_wasm_cap_is_marks_2026_11_08_ratchet_step(self):
        self.assertEqual(builder.CAPS["wasm"], 3670016)

    def test_the_viewer_ships_one_artifact_and_webgpu_is_only_an_opt_in_comparison(self):
        self.assertEqual(set(builder.ARTIFACTS), {"webgl2"})
        self.assertIn("without-classic-vello", builder.ARTIFACTS["webgl2"])
        self.assertEqual(set(builder.WEBGPU_COMPARISON), {"webgpu"})

    def test_each_artifact_is_held_to_the_wasm_cap(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload"
            path.write_bytes(random.Random(158).randbytes(builder.CAPS["wasm"] + 4096))
            rows = builder.measure_assets({"wasm:webgl2": path})
            self.assertEqual(rows["wasm:webgl2"]["cap_bytes"], builder.CAPS["wasm"])
            with self.assertRaisesRegex(RuntimeError, f"wasm:webgl2: gzip6=.*cap={builder.CAPS['wasm']}"):
                builder.require_budget(rows)

    def test_the_viewer_is_engine_free_and_worker_default_unless_engine_is_asked_for(self):
        for features in [*builder.ARTIFACTS.values(), *builder.WEBGPU_COMPARISON.values()]:
            self.assertNotIn("physics-engine", features)
            self.assertIn("physics-worker-default", features)
        self.assertEqual(builder.ENGINE, ["physics-engine"])

    def test_the_physics_worker_module_has_its_own_cap_outside_the_first_load(self):
        self.assertIn("physics", builder.CAPS)
        self.assertLess(builder.CAPS["physics"], builder.CAPS["wasm"])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "payload"
            path.write_bytes(random.Random(159).randbytes(builder.CAPS["physics"] + 4096))
            rows = builder.measure_assets({"physics:wasm": path})
            self.assertEqual(rows["physics:wasm"]["cap_bytes"], builder.CAPS["physics"])
            with self.assertRaisesRegex(RuntimeError, f"physics:wasm: gzip6=.*cap={builder.CAPS['physics']}"):
                builder.require_budget(rows)


if __name__ == "__main__":
    unittest.main()
