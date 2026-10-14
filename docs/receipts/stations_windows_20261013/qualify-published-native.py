# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Bind completed headed cases to the normal published consumer build."""
import hashlib
import json
import re
from pathlib import Path

import numpy as np
from PIL import Image

receipt = Path(__file__).resolve().parent
repo = receipt.parents[2]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

binary = json.loads((receipt / "published-native-executable.json").read_text())
assert digest(Path(binary["executable"])) == binary["sha256"]
assert digest(repo / "Cargo.toml") == binary["manifest_sha256"]
assert digest(repo / "Cargo.lock") == binary["lock_sha256"]
source = json.loads((receipt / "final-a4-source-qualification.json").read_text())
for entry in source["source_paths"] + source["fixture_paths"] + source.get("documentation_paths", []):
    assert digest(repo / entry["path"]) == entry["sha256"], entry["path"]

expected = {
    "a4_inline_components": 5, "a4_native_overlap": 4,
    "a5_floating_pane": 7, "frame_stations": 3,
    "frame_stations_reopen": 2, "frame_controls_keyboard": 2,
    "frame_controls_accessibility": 2, "frame_controls_reopen": 1,
    "frame_lens_accessibility": 3, "frame_lens_scroll": 3,
    "frame_appearance": 4, "frame_appearance_reopen": 2,
    "frame_appearance_missing": 1, "frame_appearance_missing_reopen": 1,
}
cases = []
artifacts = []
for execution_path in sorted((receipt / "native").glob("published-*/*.execution.json")):
    execution = json.loads(execution_path.read_text())
    scenario = execution["scenario"]
    assert scenario in expected and execution["result_ok"] and execution["exit"] == 0
    assert execution["executable_sha256"] == binary["sha256"]
    assert execution["primary_lock_sha256"] == binary["lock_sha256"]
    assert digest(repo / "scenarios" / f"{scenario}.scn") == execution["fixture_sha256"]
    directory = execution_path.parent / "captures" / scenario
    done = (directory / "scenario.done").read_text()
    assert done.startswith("RESULT ok")
    captures = []
    for path in sorted(directory.glob("*.png")):
        if ".raw." in path.name or ".source-" in path.name:
            continue
        rgba = np.array(Image.open(path).convert("RGBA"))
        assert np.any(rgba[:, :, :3] != rgba[0, 0, :3]), path
        captures.append({"path": str(path.relative_to(receipt)).replace("\\", "/"),
                         "sha256": digest(path),
                         "dimensions": [rgba.shape[1], rgba.shape[0]]})
    assert len(captures) == expected[scenario], (scenario, len(captures))
    if execution["workshop"]:
        workshop_done = execution_path.parent / "workshop" / scenario / "scenario.done"
        assert workshop_done.read_text().startswith("RESULT ok")
    frames, ledger_captures, blank = map(int, re.search(
        r"presentation frames=(\d+) captures=(\d+) blank=(\d+)", done).groups())
    assert blank == 0
    cases.append({**execution, "presented_primary_frames": frames,
                  "primary_ledger_captures": ledger_captures,
                  "actual_primary_and_lens_captures": captures})
    for path in sorted(directory.iterdir()):
        if path.is_file():
            artifacts.append({"path": str(path.relative_to(receipt)).replace("\\", "/"),
                              "sha256": digest(path)})
assert len(cases) == len(expected) == 14
assert sum(len(case["actual_primary_and_lens_captures"]) for case in cases) == 40

result = {
    "status": "passed", "local_path_override": False,
    "mere": "77fd235fe659c513dd0463ed57be32f02fb2e71a",
    "genet": "b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084",
    "netrender": "6a24ae08f55129b474282b8e4911564f875ce5fd",
    "executable": binary, "source_qualification": "final-a4-source-qualification.json",
    "source_qualification_sha256": digest(receipt / "final-a4-source-qualification.json"),
    "cases": cases, "actual_primary_and_lens_pngs": 40,
    "theme_editor_pngs": 1, "cases_passed": 14,
    "presented_primary_frames": sum(case["presented_primary_frames"] for case in cases),
    "artifact_hashes": artifacts,
    "pixel_reviews": ["native/published-a4-scoped/captures/a4_inline_components/pixel-review.json",
                      "native/published-alpha/captures/a4_native_overlap/pixel-review.json"],
    "visual_review": "All40 target captures inspected in three review sheets; font, alpha and custom-leaf examples inspected at full size",
    "lens_scroll": {"actual_body_changed_pixels": 35977, "early_bar_pixels_qualified": False,
                    "wheel_utc": "2026-10-14T10:10:32.717963Z",
                    "next_capture_dispatch_utc": "2026-10-14T10:10:34.019176Z",
                    "dispatch_gap_ms": 1301, "existing_hold_ms": 900, "existing_fade_ms": 250,
                    "qualified_control": "scrollbar_continuation_is_scoped_to_actual_window_pane_plan"},
    "open_gates": ["physical OS activation", "human assistive technology",
                   "monitor DPI transition", "early headed scrollbar fade pixel timing"],
    "isolated_targets_homes_worktrees": [],
}
(receipt / "published-final-native-qualification.json").write_text(json.dumps(result, indent=2))
print("14 published-source cases /40 primary+lens captures passed", result["presented_primary_frames"])
