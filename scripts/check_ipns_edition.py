#!/usr/bin/env python3
"""Verify three native Turnstone scenario receipts and their rendered fixture colors."""
import argparse
import json
from pathlib import Path

from PIL import Image


def check(root, seed, offline, current):
    # Fixture colors are unique to its stylesheet and to pixels decoded from its PNG.
    colors = {"A": ((118, 82, 209), (29, 203, 173)), "B": ((230, 133, 32), (231, 74, 119))}
    report = {}
    pids = set()
    builds = set()
    for run, shot, edition in [(seed, "live-a", "A"), (offline, "offline-a", "A"), (current, "current-b", "B")]:
        directory = root / run
        process = json.loads((directory / "process.json").read_text())
        assert process["success"] and not process["owned_pids_remaining"], run
        owned = set(process["owned_pids_observed"])
        assert owned and owned.isdisjoint(pids), run
        pids.update(owned)
        builds.add(process["binary_sha256"])
        with Image.open(directory / (shot + ".png")) as image:
            rgb = image.convert("RGB")
            histogram = {color: count for count, color in rgb.getcolors(rgb.width * rgb.height)}
        counts = {key: [histogram.get(color, 0) for color in pair] for key, pair in colors.items()}
        assert min(counts[edition]) > 1000, (run, counts)
        if edition == "A":
            assert max(counts["B"]) == 0, (run, counts)
        elif max(counts["A"]):
            # The kept pane can remain beside the current version. Verify both
            # resources in each pane, with no overlap between their columns.
            assert min(counts["A"]) > 1000, (run, counts)
            columns = {key: set() for key in colors}
            palette = {color: key for key, pair in colors.items() for color in pair}
            for index, pixel in enumerate(rgb.get_flattened_data()):
                owner = palette.get(pixel)
                if owner:
                    columns[owner].add(index % rgb.width)
            left, right = sorted(columns.values(), key=min)
            assert max(left) < min(right), (run, counts)
        report[shot] = {"pids": sorted(owned), "css_and_png_pixels": counts}
    assert len(builds) == 1, builds
    assert json.loads((root / (seed + ".moved")).read_text())["head"] == "B"
    offline_calls = (root / (offline + ".network.log")).read_text().splitlines()
    online_calls = (root / (current + ".network.log")).read_text().splitlines()
    assert offline_calls == ["observer active"], offline_calls
    assert "observer active" in online_calls and any(call in ("connect", "connectx") for call in online_calls), online_calls
    return {**report, "binary_sha256": builds.pop(), "offline_observed_network_calls": 0,
            "online_observed_network_calls": len(online_calls) - online_calls.count("observer active")}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt_directory", type=Path)
    parser.add_argument("seed")
    parser.add_argument("offline")
    parser.add_argument("current")
    args = parser.parse_args()
    print(json.dumps(check(args.receipt_directory, args.seed, args.offline, args.current), indent=2))
