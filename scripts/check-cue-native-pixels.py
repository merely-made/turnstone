#!/usr/bin/env python3
"""Check native Servo fixture source pixels, separate from compositor acceptance."""
import argparse
import hashlib
import json
from pathlib import Path
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receiving", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    assert not args.output.exists(), "preserve existing pixel evidence"
    observations = []
    for checkpoint in sorted(args.receiving.glob("*.cue-frames.json")):
        entries = json.loads(checkpoint.read_text())["entries"]
        native = {e["surface"] for e in entries if e["direction"] == "native-frame"
                  and e["scope"] and e["sampled_layers"] > 0}
        source = checkpoint.with_name(checkpoint.name.replace(".cue-frames.json", ".sources.json"))
        for row in json.loads(source.read_text()):
            if row["surface_id"] not in native:
                continue
            path = args.receiving / row["file"]
            with Image.open(path) as raw:
                image = raw.convert("RGBA")
            width, height = image.size
            assert list(image.size) == row["dimensions"] and width > 32 and height > 64
            samples = [list(image.getpixel((width - 8, y))) for y in (8, height // 2, height - 8)]
            expected = [[211, 40, 48, 255], [255, 255, 255, 255], [31, 87, 207, 255]]
            observations.append({"checkpoint": checkpoint.name, "file": path.name,
                                 "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                                 "dimensions": list(image.size), "window": row["window"],
                                 "device_generation": row["device_generation"],
                                 "adapter": row["adapter"], "samples_rgba8": samples,
                                 "passed": samples == expected})
    passed = bool(observations) and all(row["passed"] for row in observations)
    result = {"passed": passed, "native_source_rasters": observations,
              "scope": "Synthetic Servo top-red, centre-white, bottom-blue source-raster orientation and colour; no transparent-colour or OS compositor qualification"}
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
