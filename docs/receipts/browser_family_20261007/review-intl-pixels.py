"""Check the native Intl fixture's imported green body against recorded geometry.

Usage: python review-intl-pixels.py NATIVE_DIRECTORY --output NEW_RESULT.json
Requires Pillow. The directory must contain 01_native_intl_bridge.png and
intl-servo-live.surface-frames.json from the same completed native run.
Native title assertions qualify the seven page APIs separately. This check
qualifies visible imported pixels and the recorded live/cached/Servo counts.
"""

import argparse
import datetime
import hashlib
import io
import json
import math
import pathlib

from PIL import Image


EXPECTED_RGB = (23, 109, 53)  # Fixture success body: #176d35.
CHANNEL_TOLERANCE = 8
MIN_GREEN_FRACTION = 0.75
MIN_REGION_PIXELS = 4096
EDGE_INSET = 4


def inspect(directory, fixture="intl"):
    capture_name, geometry_name = {
        "intl": ("01_native_intl_bridge.png", "intl-servo-live.surface-frames.json"),
        "viewport": ("02_inspector_closed_viewport.png", "viewport-inspector-closed.surface-frames.json"),
    }[fixture]
    record = {
        "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "directory": directory.resolve().as_posix(),
        "helper_sha256": hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),
        "method": {
            "fixture": fixture,
            "expected_rgb": EXPECTED_RGB,
            "channel_tolerance": CHANNEL_TOLERANCE,
            "minimum_green_fraction": MIN_GREEN_FRACTION,
            "minimum_region_pixels": MIN_REGION_PIXELS,
            "content_edge_inset_pixels": EDGE_INSET,
            "required_live_producers_cached_frames_servo_active_views": 1,
        },
        "limits": "Fixture-specific imported-body check. Native title assertions qualify page APIs separately. This does not establish text correctness, accessibility, arbitrary page correctness or native teardown.",
        "inputs": [],
        "failures": [],
    }
    try:
        blobs = {}
        for name in (capture_name, geometry_name):
            blob = (directory / name).read_bytes()
            blobs[name] = blob
            record["inputs"].append({
                "path": name,
                "bytes": len(blob),
                "sha256": hashlib.sha256(blob).hexdigest(),
            })
        frame = json.loads(blobs[geometry_name])
        counts = {key: frame[key] for key in
                  ("live_producers", "cached_frames", "servo_active_views")}
        record["counts"] = counts
        if any(type(value) is not int or value != 1 for value in counts.values()):
            record["failures"].append("Expected exactly one live producer, cached frame and Servo view")
        surfaces = frame["content_surfaces"]
        if len(surfaces) != 1:
            raise ValueError("Expected exactly one recorded content rectangle")
        surface = surfaces[0]
        rect = {key: float(surface[key]) for key in ("x", "y", "width", "height")}
        record["surface"] = {"node": surface["node"], "recorded_rectangle": rect}
        if not all(math.isfinite(value) for value in rect.values()):
            raise ValueError("Content rectangle must contain finite coordinates")
        x, y, width, height = (rect[key] for key in ("x", "y", "width", "height"))
        if width <= 0 or height <= 0:
            raise ValueError("Content rectangle must have positive dimensions")
        with Image.open(io.BytesIO(blobs[capture_name])) as source:
            image = source.convert("RGB")
            record["capture_size"] = list(image.size)
            if x < 0 or y < 0 or x + width > image.width or y + height > image.height:
                raise ValueError("Full recorded content rectangle must lie inside the capture")
            box = (math.ceil(x) + EDGE_INSET, math.ceil(y) + EDGE_INSET,
                   math.floor(x + width) - EDGE_INSET, math.floor(y + height) - EDGE_INSET)
            region_width, region_height = box[2] - box[0], box[3] - box[1]
            if region_width < 64 or region_height < 64:
                raise ValueError("Recorded content region is too small for a meaningful body check")
            area = region_width * region_height
            if area < MIN_REGION_PIXELS:
                raise ValueError("Content region has insufficient pixel area")
            matched = sum(all(abs(actual - expected) <= CHANNEL_TOLERANCE
                              for actual, expected in zip(pixel, EXPECTED_RGB))
                          for pixel in image.crop(box).getdata())
            fraction = matched / area
            record["pixels"] = {
                "sample_box": list(box),
                "sampled_pixels": area,
                "matched_green_pixels": matched,
                "matched_green_fraction": fraction,
            }
            if fraction < MIN_GREEN_FRACTION:
                record["failures"].append("Green body coverage is below the required fraction")
    except (OSError, ValueError, KeyError, TypeError, OverflowError) as error:
        record["failures"].append(str(error))
    record["passed"] = not record["failures"]
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument("--fixture", choices=("intl", "viewport"), default="intl")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("Output already exists; choose a new receipt path")
    record = inspect(args.directory, args.fixture)
    with args.output.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(record, output, indent=2)
        output.write("\n")
    print("Native imported green-body pixels:", "PASS" if record["passed"] else "FAIL")
    for failure in record["failures"]:
        print(failure)
    return 0 if record["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
