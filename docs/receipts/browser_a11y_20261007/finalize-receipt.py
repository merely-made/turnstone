#!/usr/bin/env python3
"""Freeze raw qualification inputs and index retained artifacts after all gates."""
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    archive = HERE / "qualified-source.zip"
    manifest = HERE / "qualified-source.json"
    index = HERE / "artifact-index.json"
    if any(path.exists() for path in (archive, manifest, index)):
        raise FileExistsError("Preserving an existing frozen receipt")
    paths = subprocess.check_output(
        ["git", "ls-files", "src", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"],
        cwd=ROOT, text=True,
    ).splitlines()
    paths += ["src/foreign_a11y.rs", "src/shell/foreign_a11y.rs",
              "scenarios/browser_servo_a11y_windows.scn",
              "scenarios/browser_servo_a11y_uia_windows.scn",
              "scenarios/fixtures/browser_servo/a11y.html",
              "scenarios/fixtures/browser_servo/fixture.css",
              "docs/receipts/browser_supplier_integration_20261006/native-runner.ps1"]
    paths += [str(p.relative_to(ROOT)).replace("\\", "/")
              for p in HERE.iterdir() if p.suffix in (".py", ".ps1")]
    inputs = {}
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as frozen:
        for path in sorted(set(paths)):
            data = (ROOT / path).read_bytes()
            inputs[path] = sha(data)
            frozen.writestr(path, data)
    with zipfile.ZipFile(archive) as frozen:
        assert {p: sha(frozen.read(p)) for p in frozen.namelist()} == inputs
    manifest.write_text(json.dumps({
        "base_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "archive_sha256": sha(archive.read_bytes()), "inputs": inputs,
    }, indent=2) + "\n", encoding="utf-8")
    artifact_paths = [p for p in HERE.rglob("*") if p.is_file() and p != index]
    for prefix in ("a11y", "a11y-admission", "a11y-uia", "a11y-upstream"):
        native = ROOT / "docs/receipts/browser_supplier_integration_20261006" / f"native-{prefix}-servo"
        if native.is_dir():
            artifact_paths.extend(p for p in native.rglob("*") if p.is_file())
    artifacts = {str(p.relative_to(ROOT)).replace("\\", "/"): sha(p.read_bytes())
                 for p in sorted(set(artifact_paths))}
    index.write_text(json.dumps({"artifacts": artifacts}, indent=2) + "\n", encoding="utf-8")
    print(f"Frozen {len(inputs)} inputs and indexed {len(artifacts)} artifacts")


if __name__ == "__main__":
    main()
