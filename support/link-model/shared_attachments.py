#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Verify shared attachment receipts using the existing link evidence checks."""
from __future__ import annotations

import argparse
from pathlib import Path
import sys

import ordinary_paths as CHECK

SEED = "p4_tree_link_attachments"
REOPEN = "p4_tree_link_attachments_reopen"
PREFIX = "ordinary_link_attachments_"
SEED_CAPTURES = {PREFIX + name for name in (
    "flat", "response", "tilted", "rounded", "circle", "suggestion", "lifted", "lifted_selected",
)}
REOPEN_CAPTURES = {PREFIX + name for name in ("reopened_default", "reopened_tilted")}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("qualification_root", type=Path)
    root = parser.parse_args().qualification_root.resolve()
    revision, records = CHECK.verify_build_manifest(root)
    for name, tokens in (("build", ("cargo", "build")), ("bindgen", ("bindgen",))):
        CHECK.verify_command_record(root, records[name], revision, True, tokens)
    canvas = CHECK.verify_cargo_qualification(root, records["canvas"], revision, "pictograph")
    graphshell = CHECK.verify_cargo_qualification(root, records["graphshell"], revision, "graphshell")
    seed = CHECK.verify_browser_scenario(root, SEED, SEED_CAPTURES)
    reopen = CHECK.verify_browser_scenario(root, REOPEN, REOPEN_CAPTURES)
    CHECK.verify_source_comparison(root, seed, reopen)
    for entry in CHECK.read_json(root / "build.json").get("qualification_inputs", []):
        path = CHECK.resolve_relative(root / "site", entry["path"], "qualification input", ".scn")
        if CHECK.sha256_file(path) != entry["sha256"]:
            CHECK.fail("executed qualification input hash mismatch")
    reviews = CHECK.read_json(root / "browser-review.json").get("images", [])
    expected = {f"browser-qualified/{SEED}/{name}.png" for name in SEED_CAPTURES}
    expected |= {f"browser-qualified/{REOPEN}/{name}.png" for name in REOPEN_CAPTURES}
    if {row["path"] for row in reviews} != expected or len(reviews) != len(expected):
        CHECK.fail("image review must cover all ten accepted captures exactly once")
    for row in reviews:
        if row.get("reviewed") is not True or CHECK.sha256_file(root / row["path"]) != row["sha256"]:
            CHECK.fail("image review does not match the accepted PNG")
    if reopen["scenario"]["result"]["saved_graph"].get("reopened") is not True:
        CHECK.fail("fresh process did not report reopening stored graph data")
    processes = CHECK.read_json(root / "browser-processes.json")
    if processes.get("source_revision") != revision:
        CHECK.fail("browser process observations have a different source revision")
    observed = {row["phase"]: row["chrome_pids"] for row in processes.get("observations", [])}
    before, after = observed.get("seed-before-navigation", []), observed.get("reopen-before-navigation", [])
    if len(before) != 1 or len(after) != 1 or before == after:
        CHECK.fail("seed and reopen need distinct recorded native Chrome process IDs")
    if observed.get("seed-exit") != [] or observed.get("reopen-exit") != []:
        CHECK.fail("both native Chrome exits must be observed")
    print(f"shared attachment evidence verified; source {revision}")
    print(f"Pictograph passes: {canvas}; Graphshell passes: {graphshell}")
    print("8 seed and 2 reopen PNGs; zero scenario errors/gate failures; source observations equal")
    print("stored graph reopened in a distinct native process; both exits observed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CHECK.VerificationError as exc:
        print(f"verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
