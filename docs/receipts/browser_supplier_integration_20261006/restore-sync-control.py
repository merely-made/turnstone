"""Restore only the archived diagnostic lane's temporary consumer changes."""
import datetime
import hashlib
import json
import pathlib
import zipfile

root = pathlib.Path(__file__).resolve().parents[3]
receipt = pathlib.Path(__file__).resolve().parent
control = receipt / "sync-diagnostic"
before = json.loads((receipt / "source-manifest-all3-sync-control.json").read_text())
expected = {item["path"]: item["sha256"] for item in before["files"]}
previous = json.loads((receipt / "source-manifest-all3-pixel-control.json").read_text())
restore_paths = (
    "src/shell/servo.rs",
    "docs/receipts/browser_supplier_integration_20261006/native-runner.ps1",
    "docs/receipts/browser_supplier_integration_20261006/run-servo.ps1",
)
for name in restore_paths:
    assert hashlib.sha256((root / name).read_bytes()).hexdigest() == expected[name], name
original_lock = (control / "Cargo.lock.git-candidate").read_bytes()
supplier = json.loads((control / "supplier-source-manifest.json").read_text())
assert hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest() == supplier["control_lock_sha256"]
assert (root / "Cargo.toml").read_bytes() == (control / "Cargo.toml.git-candidate").read_bytes()
config = (root / ".cargo/config.toml").resolve()
assert config.is_relative_to(root.resolve()) and config.name == "config.toml"
prepare = json.loads((control / "prepare.json").read_text())
assert hashlib.sha256(config.read_bytes()).hexdigest() == prepare["config_sha256"]
with zipfile.ZipFile(receipt / "source-inputs-all3-pixel-control.zip") as archive:
    for name in restore_paths:
        raw = archive.read(name)
        assert hashlib.sha256(raw).hexdigest() == next(p["sha256"] for p in previous["files"] if p["path"] == name)
        (root / name).write_bytes(raw)
(root / "Cargo.lock").write_bytes(original_lock)
config.unlink()
assert all(hashlib.sha256((root / item["path"]).read_bytes()).hexdigest() == item["sha256"] for item in previous["files"])
record = {
    "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "restored_consumer_files": list(restore_paths) + ["Cargo.lock"],
    "removed_owned_config": str(config),
    "all175_prior_candidate_inputs_exactly_restored": True,
    "diagnostic_preserved": "source-inputs-all3-sync-control.zip and sync-diagnostic/supplier-source-inputs.zip",
    "scope": "Experimental host control archived; portable Git candidate restored. Existing config.local.toml and local directory untouched. Final U9 adoption and default reopen correctness remain held.",
}
with (control / "restoration.json").open("x", encoding="utf-8") as stream:
    json.dump(record, stream, indent=2)
    stream.write("\n")
print("Restored all 175 prior candidate inputs; removed only the owned diagnostic config; preserved complete experimental sources.")
