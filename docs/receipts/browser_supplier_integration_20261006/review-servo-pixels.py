"""Check visible Servo fixture orientation against recorded content geometry."""
import argparse
import datetime
import hashlib
import json
import pathlib

from PIL import Image

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("directory", type=pathlib.Path)
parser.add_argument("--output", required=True, type=pathlib.Path)
args = parser.parse_args()
checks = (
    ("01_two_upstream_servo_pages", "two-servo-pages"),
    ("02_a_native_input_and_navigation", "independent-input"),
    ("03_b_native_input_and_navigation", "independent-input"),
    ("04_resize_and_orientation", "servo-resized"),
    ("06_a_servo_reconstructed", "servo-reconstructed"),
    ("08_reopened_in_same_servo_process", "servo-reopened"),
)
record = {
    "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "directory": args.directory.as_posix(),
    "method": "Each of two recorded live content rectangles must show the fixture's red top and blue bottom, 16 pixels each within RGB channel tolerance 8. Samples avoid text and the host toolbar.",
    "limits": "Fixture-specific orientation/blank-page check; does not prove text, accessibility, physical input, DPI or arbitrary page correctness. Reader refusal and closed-view captures are reviewed separately.",
    "checks": [],
}
for capture, geometry in checks:
    path = args.directory / (capture + ".png")
    frame_path = args.directory / (geometry + ".surface-frames.json")
    frame = json.loads(frame_path.read_text(encoding="utf-8"))
    item = {
        "capture": path.name,
        "capture_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "geometry": frame_path.name,
        "geometry_sha256": hashlib.sha256(frame_path.read_bytes()).hexdigest(),
        "surfaces": [],
    }
    with Image.open(path) as source:
        pixels = source.convert("RGB")
        for surface in frame["content_surfaces"]:
            x, y = round(surface["x"]), round(surface["y"])
            height = round(surface["height"])
            bands = []
            for name, top, expected in (
                ("top", y + 20, (211, 40, 48)),
                ("bottom", y + height - 10, (31, 87, 207)),
            ):
                matches = 0
                for dx in range(4, 8):
                    for dy in range(4):
                        actual = pixels.getpixel((x + dx, top + dy))
                        matches += all(abs(a - e) <= 8 for a, e in zip(actual, expected))
                bands.append({"band": name, "matched_pixels": matches, "required_pixels": 16})
            item["surfaces"].append({"node": surface["node"], "bands": bands})
    item["passed"] = (
        frame["live_producers"] == frame["cached_frames"] == frame["servo_active_views"] == 2
        and len(item["surfaces"]) == 2
        and all(b["matched_pixels"] == b["required_pixels"] for s in item["surfaces"] for b in s["bands"])
    )
    record["checks"].append(item)
record["passed"] = all(item["passed"] for item in record["checks"])
with args.output.open("x", encoding="utf-8", newline="\n") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
print("Servo fixture pixels:", "PASS" if record["passed"] else "FAIL")
for item in record["checks"]:
    if not item["passed"]:
        print("Failed capture:", item["capture"])
raise SystemExit(0 if record["passed"] else 1)
