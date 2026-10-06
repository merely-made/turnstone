"""Freeze/verify the merged browser source, lock and production executable."""
import argparse
import datetime
import hashlib
import json
import pathlib
import subprocess
import tomllib

r = pathlib.Path(__file__).resolve().parent
root = r.parents[2]
parser = argparse.ArgumentParser()
parser.add_argument("--verify", action="store_true")
args = parser.parse_args()
manifest = r / "source-manifest.json"
exe = pathlib.Path("C:/t/cargo-targets/turnstone/debug/turnstone.exe")
def digest(p):
    with p.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()
if args.verify:
    record = json.loads(manifest.read_text())
    changes = [v["path"] for v in record["files"]
               if digest(root / v["path"]) != v["sha256"]]
    if digest(exe) != record["exe_sha256"]:
        changes.append(str(exe))
    if changes:
        raise SystemExit("Changed qualification inputs: " + ", ".join(changes))
    print("Merged source/scenario and executable fingerprints unchanged")
else:
    if manifest.exists():
        raise SystemExit("Refusing to overwrite frozen qualification")
    paths = [root / "Cargo.toml", root / "Cargo.lock",
             *sorted((root / "src").rglob("*.rs")),
             *[root / p for p in (
                 "scenarios/browser_scry_windows.scn",
                 "scenarios/browser_weld_direct_windows.scn",
                 "scenarios/browser_weld_permission_direct_windows.scn",
                 "scenarios/fixtures/browser_scry/restart_verify.scn",
                 "scenarios/fixtures/browser_decisions_server.ps1")],
             *sorted((root / "scenarios/fixtures/browser_scry").glob("*.html")),
             *sorted((root / "scenarios/fixtures/browser_scry").glob("*.css")),
             *sorted((root / "scenarios/fixtures/browser_scry").glob("*.js"))]
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    records = [dict(name=p["name"], version=p["version"], source=p.get("source"))
               for p in lock["package"] if p["name"] in (
                   "inker", "weld-engine", "scrying-engine", "welding", "scrying",
                   "grafting", "wgpu", "distillery", "redshank-model")]
    record = dict(
        recorded_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        browser_commit="a383cdd4fd690d73927d01db9ec21c3bfd975150",
        integrated_upstream="97e8e49667685346e4898a859364d37757f9996e",
        features=["scry", "weld"], dependencies=records,
        files=[dict(path=p.relative_to(root).as_posix(), sha256=digest(p)) for p in paths],
        exe_sha256=digest(exe))
    manifest.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    print(record["exe_sha256"])
