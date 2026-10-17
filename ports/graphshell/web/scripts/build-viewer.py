#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Build and package the read-only viewer, enforcing R157's delivery caps.

The viewer ships one artifact per presentation route, and `mount.js` loads
exactly one of them per page: `webgpu/` and `webgl2/` (the latter without
Classic Vello). Each artifact's Wasm and glue is held to the unchanged caps.
"""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess


# R157's caps. Mark amended the Wasm cap to 3,460,300 B (3.30 MiB) on
# 2026-10-15 and to 3,670,016 B (3.50 MiB) on 2026-10-16, applied to each
# route artifact.
CAPS = {"wasm": 3670016, "glue": 20480, "mount": 8192, "history": 131072}
# Route artifact name -> the viewer crate's features for that artifact.
ARTIFACTS = {
    "webgpu": ["presentation-webgpu"],
    "webgl2": ["presentation-webgl2", "without-classic-vello"],
}


def measure_assets(assets):
    """Check both reproducible gzip9 and ordinary gzip6 delivery sizes."""
    rows = {}
    for name, path in assets.items():
        data = path.read_bytes()
        row = {"raw_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
               "cap_bytes": CAPS[name.split(":")[0]]}
        for level in (6, 9):
            row[f"gzip{level}_bytes"] = len(gzip.compress(data, compresslevel=level, mtime=0))
        row["cap_pass"] = max(row["gzip6_bytes"], row["gzip9_bytes"]) <= row["cap_bytes"]
        rows[name] = row
    return rows


def require_budget(rows):
    failures = [f"{name}: gzip6={r['gzip6_bytes']}, gzip9={r['gzip9_bytes']}, cap={r['cap_bytes']}"
                for name, r in rows.items() if not r["cap_pass"]]
    if failures:
        raise RuntimeError("Viewer delivery budget exceeded: " + "; ".join(failures))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--wasm-bindgen", default="wasm-bindgen")
    parser.add_argument("--terser", default="terser", help="Terser CLI; install pinned scripts/package-lock.json dependencies")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    web = Path(__file__).resolve().parents[1]
    repo = web.parents[2]
    target = (args.target_dir or web / "target").resolve()
    out = args.out_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    commands = {}
    minifiers = {}
    assets = {}
    for artifact, features in ARTIFACTS.items():
        command = [args.cargo, "build", "--locked", "--profile", "viewer",
                   "--target", "wasm32-unknown-unknown", "--no-default-features",
                   "--features", ",".join(features),
                   "--target-dir", str(target), "-j1"]
        if args.offline:
            command.append("--offline")
        subprocess.run(command, cwd=web, check=True)
        commands[artifact] = command
        raw = target / "wasm32-unknown-unknown/viewer/graphshell_web.wasm"
        artifact_out = out / artifact
        artifact_out.mkdir(parents=True, exist_ok=True)
        subprocess.run([args.wasm_bindgen, "--target", "web", "--remove-name-section",
                        "--out-name", "graphshell_web", "--out-dir", str(artifact_out), str(raw)],
                       check=True)
        glue = artifact_out / "graphshell_web.js"
        glue_input_sha256 = hashlib.sha256(glue.read_bytes()).hexdigest()
        compact = artifact_out / "graphshell_web.min.js"
        minify_command = [args.terser, str(glue), "--module", "--compress", "--mangle",
                          "--output", str(compact)]
        subprocess.run(minify_command, check=True)
        compact.replace(glue)
        minifiers[artifact] = {"command": minify_command, "input_sha256": glue_input_sha256}
        assets[f"wasm:{artifact}"] = artifact_out / "graphshell_web_bg.wasm"
        assets[f"glue:{artifact}"] = glue
    assets["mount"] = web / "viewer.js"
    assets["history"] = web / "fixtures/site-repository-host-history.json"
    rows = measure_assets(assets)
    report = {"source": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo,
                                                text=True).strip(),
              "profile": "viewer", "artifacts": ARTIFACTS,
              "commands": commands, "rustc": subprocess.check_output(
                  [os.environ.get("RUSTC", "rustc"), "--version"], text=True).strip(),
              "manifest_sha256": hashlib.sha256((web / "Cargo.toml").read_bytes()).hexdigest(),
              "minifiers": minifiers,
              "minifier_version": subprocess.check_output([args.terser, "--version"], text=True).strip(),
              "sizes": rows, "all_caps_pass": all(r["cap_pass"] for r in rows.values())}
    (out / "viewer-delivery.json").write_text(json.dumps(report, indent=2) + "\n")
    require_budget(rows)
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
