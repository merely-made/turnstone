# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Check actual selected raster readbacks; never reconstruct producer pixels."""
import json
from pathlib import Path

import numpy as np
from PIL import Image

receipt = Path(__file__).resolve().parent
for case, scenario in (
    ("fixed-a4-scoped", "a4_inline_components"),
    ("fixed-alpha", "a4_native_overlap"),
):
    directory = receipt / "native" / case / "captures" / scenario
    results = []
    for raw_path in sorted(directory.glob("*.raw.png")):
        stem = raw_path.name.removesuffix(".raw.png")
        raw = np.array(Image.open(raw_path).convert("RGBA"))
        final = np.array(Image.open(directory / f"{stem}.png").convert("RGBA"))
        assert raw.shape == final.shape
        assert raw[:, :, 3].min() == final[:, :, 3].min() == 255
        scene = json.loads((directory / f"{stem}.scene.json").read_text())
        root = scene["ops"][0]["Rect"]
        assert [root["x0"], root["y0"], root["x1"], root["y1"]] == [
            0, 0, raw.shape[1], raw.shape[0]
        ]
        assert root["color"][3] == 1
        scoped = np.ones(raw.shape[:2], dtype=bool)
        if case == "fixed-alpha":
            # Independent chrome overlays are deliberately composed afterward.
            scoped[:64] = False
            caption_top = 632 if raw.shape[0] == 1300 else 782
            scoped[caption_top:caption_top + 23, 8:132] = False
            assert np.allclose(root["color"], [24 / 255, 38 / 255, 60 / 255, 1])
        assert np.array_equal(raw[scoped], final[scoped])
        results.append({
            "capture": stem,
            "dimensions": [raw.shape[1], raw.shape[0]],
            "raw_minimum_alpha": int(raw[:, :, 3].min()),
            "final_minimum_alpha": int(final[:, :, 3].min()),
            "authored_root": root,
            "scoped_raw_final_exact": True,
            "independent_overlay_differences": int(np.any(raw != final, axis=2).sum()),
        })
    assert len(results) == 4
    (directory / "pixel-review.json").write_text(json.dumps(results, indent=2))
    print(case, "4 actual raw/final pairs passed")
