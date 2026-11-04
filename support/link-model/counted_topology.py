#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Verify counted-topology receipts through the existing link evidence checks."""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import subprocess
import sys

import ordinary_paths as CHECK
REPOSITORY = Path(__file__).resolve().parents[2]

SEED = "p8_tree_counted_topology"
REOPEN = "p8_tree_counted_topology_reopen"
SEED_CAPTURES = {"p8_pair_topology_initial", "p8_pair_topology_exact_reason_hidden",
                 "p8_promoted_source_before_withdrawal", "p8_promoted_copy_after_withdrawal",
                 "p8_pair_topology_after_withdrawal"}
REOPEN_CAPTURES = {"p8_counted_topology_fresh_reopen"}


def git_bytes(*arguments: str) -> bytes:
    result = subprocess.run(["git", "-C", str(REPOSITORY), *arguments],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode != 0:
        CHECK.fail("Git could not verify the committed qualification input: " +
                   result.stderr.decode("utf-8", errors="replace").strip())
    return result.stdout


def main() -> int:
    global REPOSITORY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("qualification_root", type=Path)
    parser.add_argument("--repo", type=Path, default=REPOSITORY)
    args = parser.parse_args()
    REPOSITORY = args.repo.resolve()
    root = args.qualification_root.resolve()
    revision, records = CHECK.verify_build_manifest(root)
    for name, tokens in (("build", ("cargo", "build")), ("bindgen", ("bindgen",))):
        CHECK.verify_command_record(root, records[name], revision, True, tokens)
    canvas = CHECK.verify_cargo_qualification(root, records["canvas"], revision, "pictograph")
    graphshell = CHECK.verify_cargo_qualification(root, records["graphshell"], revision, "graphshell")
    engine = CHECK.verify_cargo_qualification(root, root / "seiche-final-2.json", revision, "seiche")
    if engine != 220 or canvas != 580:
        CHECK.fail("unexpected engine/canvas qualification totals")
    for name, tokens in (("viewer-final.json", ("cargo", "check")),
                         ("boundaries-final.json", ("check_port_boundaries.py",))):
        CHECK.verify_command_record(root, root / name, revision, True, tokens)
    seed = CHECK.verify_browser_scenario(root, SEED, SEED_CAPTURES)
    reopen = CHECK.verify_browser_scenario(root, REOPEN, REOPEN_CAPTURES)
    CHECK.verify_source_comparison(root, seed, reopen)
    inputs = CHECK.read_json(root / "build.json").get("qualification_inputs")
    input_revision = CHECK.read_json(root / "build.json").get("qualification_input_revision")
    continuity = CHECK.read_json(root / "qualification-input-continuity.json")
    if not isinstance(input_revision, str) or not CHECK.HEX40.fullmatch(input_revision):
        CHECK.fail("qualification input needs its committed revision")
    if (continuity.get("binary_source_revision") != revision or
            continuity.get("qualification_input_revision") != input_revision or
            continuity.get("changed_paths") != [f"ports/graphshell/web/scenarios/{REOPEN}.scn"]):
        CHECK.fail("input correction must be isolated from compiled product source")
    recorded_diff = CHECK.require_string(continuity, "git_diff", "qualification input continuity")
    if git_bytes("rev-parse", "--verify", input_revision + "^{commit}").decode().strip() != input_revision:
        CHECK.fail("qualification input revision is not a commit")
    changed = git_bytes("diff", "--name-only", revision, input_revision).decode().splitlines()
    if changed != continuity["changed_paths"]:
        CHECK.fail("actual Git input correction changes compiled source or another path")
    if git_bytes("diff", revision, input_revision, "--", changed[0]).decode() != recorded_diff:
        CHECK.fail("recorded qualification input correction differs from Git")
    expected_inputs = {f"ports/graphshell/web/scenarios/{name}.scn" for name in (SEED, REOPEN)}
    if not isinstance(inputs, list) or len(inputs) != 2 or any(not isinstance(row, dict) for row in inputs):
        CHECK.fail("exactly two qualification input objects are required")
    if {row.get("path") for row in inputs} != expected_inputs:
        CHECK.fail("exact seed and reopen qualification paths are required")
    for entry in inputs:
        path = CHECK.resolve_relative(root / "site", entry["path"], "qualification input", ".scn")
        if CHECK.sha256_file(path) != entry.get("sha256"):
            CHECK.fail("executed qualification input hash mismatch")
        committed = git_bytes("show", input_revision + ":" + entry["path"])
        if hashlib.sha256(committed).hexdigest() != entry.get("sha256"):
            CHECK.fail("executed qualification input differs from its committed revision")
    reviews = CHECK.read_json(root / "browser-review.json").get("images", [])
    expected = {f"browser-qualified/{SEED}/{name}.png" for name in SEED_CAPTURES}
    expected |= {f"browser-qualified/{REOPEN}/{name}.png" for name in REOPEN_CAPTURES}
    if {row["path"] for row in reviews} != expected or len(reviews) != len(expected):
        CHECK.fail("image review must cover all six accepted captures exactly once")
    for row in reviews:
        observations = row.get("reviews")
        if not isinstance(observations, list) or not observations:
            CHECK.fail("each capture needs named visual review observations")
        for observation in observations:
            if not isinstance(observation, dict):
                CHECK.fail("visual review must be an object")
            CHECK.require_string(observation, "reviewer", "visual review")
            CHECK.require_string(observation, "finding", "visual review")
            if observation.get("disposition") != "accepted":
                CHECK.fail("unaccepted image review")
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
    print(f"counted endpoint topology evidence verified; source {revision}")
    print(f"Seiche passes: {engine}; Pictograph passes: {canvas}; Graphshell passes: {graphshell}")
    print("5 seed and 1 reopen PNGs; zero scenario errors/gate failures; source observations equal")
    print("stored graph reopened in a distinct native process; both exits observed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CHECK.VerificationError as exc:
        print(f"verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
