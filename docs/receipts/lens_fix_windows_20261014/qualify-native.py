# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Bind the post-fix Windows headed cases to the scripted App producer."""
import hashlib
import json
import re
import subprocess
from pathlib import Path

import numpy as np
from PIL import Image

receipt = Path(__file__).resolve().parent
repo = receipt.parents[2]
QUALIFIED = "b0b709a5"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def git(*args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True,
                          capture_output=True, text=True).stdout.strip()

head = git("rev-parse", "HEAD")
assert git("diff", "--stat", QUALIFIED, "HEAD", "--", ".", ":!design_docs") == ""
assert git("status", "--porcelain", "--", ".", ":!docs/receipts", ":!design_docs") == ""

binary = json.loads((receipt / "published-native-executable.json").read_text())
assert digest(Path(binary["executable"])) == binary["sha256"]
assert digest(repo / "Cargo.toml") == binary["manifest_sha256"]
assert digest(repo / "Cargo.lock") == binary["lock_sha256"]

# Published ten cases (14 scenarios), then the inferred original five.
expected = {
    "a4_inline_components": 5, "a4_native_overlap": 4,
    "a5_floating_pane": 7, "frame_stations": 3,
    "frame_stations_reopen": 2, "frame_controls_keyboard": 2,
    "frame_controls_accessibility": 2, "frame_controls_reopen": 1,
    "frame_lens_accessibility": 3, "frame_lens_scroll": 3,
    "frame_appearance": 4, "frame_appearance_reopen": 2,
    "frame_appearance_missing": 1, "frame_appearance_missing_reopen": 1,
}
inferred_five = {
    "document_a11y_gemini": 1, "document_a11y_gopher": 1,
    "document_a11y_micron": 2, "document_a11y_lens": 3, "reader_appearance": 4,
}
cases, artifacts = [], []
for execution_path in sorted((receipt / "native").glob("*/*.execution.json")):
    execution = json.loads(execution_path.read_text())
    scenario = execution["scenario"]
    assert scenario in expected or scenario in inferred_five, scenario
    assert execution["result_ok"] and execution["exit"] == 0, scenario
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
    want = expected.get(scenario, inferred_five.get(scenario))
    assert len(captures) == want, (scenario, len(captures))
    if execution["workshop"]:
        workshop_done = execution_path.parent / "workshop" / scenario / "scenario.done"
        assert workshop_done.read_text().startswith("RESULT ok")
    ledger = re.search(r"presentation frames=(\d+) captures=(\d+) blank=(\d+)", done)
    frames, ledger_captures, blank = map(int, ledger.groups()) if ledger else (None, None, None)
    assert blank in (0, None), scenario
    cases.append({**execution, "set": "published" if scenario in expected else "inferred_original_five",
                  "presented_primary_frames": frames, "primary_ledger_captures": ledger_captures,
                  "actual_primary_and_lens_captures": captures})
    for path in sorted(directory.iterdir()):
        if path.is_file():
            artifacts.append({"path": str(path.relative_to(receipt)).replace("\\", "/"),
                              "sha256": digest(path)})

published = [c for c in cases if c["set"] == "published"]
five = [c for c in cases if c["set"] != "published"]
assert {c["scenario"] for c in published} == set(expected)
result = {
    "status": "passed", "machine": "o-pc", "os": "windows",
    "qualified_source": QUALIFIED, "head": head, "local_path_override": False,
    "mere": "77fd235fe659c513dd0463ed57be32f02fb2e71a",
    "genet": "b6bfbeb556a6a17eddf8d5ad8254d3fcac95b084",
    "netrender": "6a24ae08f55129b474282b8e4911564f875ce5fd",
    "executable": binary,
    "published_cases_passed": len(published),
    "published_primary_and_lens_pngs": sum(len(c["actual_primary_and_lens_captures"]) for c in published),
    "inferred_original_five_passed": len(five),
    "inferred_original_five_pngs": sum(len(c["actual_primary_and_lens_captures"]) for c in five),
    "inferred_original_five_reading": "Reading, not ruled: no doc names the original five; identified as document_a11y_{gemini,gopher,micron,lens} + reader_appearance by their eleven captures, the lens label assertion and serve.py's Reader port.",
    "cases": cases, "artifact_hashes": artifacts,
}
(receipt / "final-native-qualification.json").write_text(json.dumps(result, indent=2))
print(len(published), "published scenarios /", result["published_primary_and_lens_pngs"], "captures;",
      len(five), "inferred-five scenarios /", result["inferred_original_five_pngs"], "captures")
