#!/usr/bin/env python3
"""Check the article fixture's producer rasters before and after a 120px scroll.

This is source-texture evidence, separate from OS compositor acceptance.
Requires Pillow, already installed on the ThinkPad receiving host.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageChops


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receiving", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    assert not args.output.exists(), "preserve existing pixel evidence"
    images = []
    observations = []
    scopes = []
    expected = (244, 239, 227, 255)
    for name in ["02_retained_zoomed", "03_retained_scrolled"]:
        rows = json.loads((args.receiving / (name + ".sources.json")).read_text())
        assert len(rows) == 1, "fixture requires one retained source raster"
        row = rows[0]
        path = args.receiving / row["file"]
        with Image.open(path) as raw:
            image = raw.convert("RGBA")
        assert list(image.size) == row["dimensions"]
        assert row["adapter"]["backend"] == "Vulkan"
        images.append(image)
        observations.append({
            "checkpoint": name, "file": path.name,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "dimensions": list(image.size), "adapter": row["adapter"],
            "sample_position": [8, image.height - 8],
            "sample_rgba8": list(image.getpixel((8, image.height - 8))),
            "wrong_canvas_rows": [y for y in range(image.height)
                                  if image.getpixel((8, y)) != expected],
        })
        entries = json.loads((args.receiving / (name + ".cue-frames.json")).read_text())["entries"]
        scopes.append({e["scope"] for e in entries if e["scope"] and e["sampled_layers"] > 0})
    before, after = images
    assert before.size == after.size
    width, height = before.size
    assert width > 256 and height > 120
    # Check movement as well as fill. The article's centre must match the
    # prior frame shifted upward, and must differ from the unscrolled image.
    box = (128, 0, width - 128, height - 120)
    shifted = before.crop((128, 120, width - 128, height))
    current = after.crop(box)
    difference = ImageChops.difference(shifted, current)
    differing = sum(any(pixel) for pixel in difference.getdata())
    stationary = ImageChops.difference(before.crop(box), current)
    changed = sum(any(pixel) for pixel in stationary.getdata())
    count = current.width * current.height
    same_scope = bool(scopes[0]) and scopes[0] == scopes[1]
    passed = (all(not row["wrong_canvas_rows"] for row in observations)
              and differing / count < 0.02 and changed / count > 0.01 and same_scope)
    result = {
        "passed": passed, "expected_canvas_rgba8": list(expected),
        "source_rasters": observations, "shift_pixels": 120,
        "shift_mismatch_fraction": differing / count,
        "unshifted_change_fraction": changed / count,
        "scopes": [sorted(s) for s in scopes], "same_live_scope": same_scope,
        "scope": "Synthetic article source-raster colour and scroll check; no physical compositor, IME or human AT claim",
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
