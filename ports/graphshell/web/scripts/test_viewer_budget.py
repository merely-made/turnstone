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
            with self.assertRaisesRegex(RuntimeError, "wasm: gzip6=.*cap=3407872"):
                builder.require_budget(rows)


if __name__ == "__main__":
    unittest.main()
