"""Archive a verified current executable's inputs and explicitly completed evidence."""
import argparse
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import zipfile

receipt = pathlib.Path(__file__).resolve().parent
root = receipt.parents[2]
supplier = root / "docs/receipts/browser_supplier_integration_20261006"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--label", required=True)
parser.add_argument("--select", action="append", default=[], help="Completed repository-relative evidence")
parser.add_argument("--native", action="append", default=[], help="Completed repository-relative native gate")
parser.add_argument("--verify", action="store_true")
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9-]{0,63}", args.label):
    parser.error("label must be a lowercase filename component")
manifest = receipt / f"artifact-manifest-{args.label}.json"
archive = receipt / f"source-inputs-{args.label}.zip"
source_path = supplier / f"source-manifest-{args.label}.json"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def within_root(name):
    path = pathlib.Path(name)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit("Expected a repository-relative path: " + name)
    resolved = (root / path).resolve(strict=True)
    if not resolved.is_relative_to(root.resolve()):
        raise SystemExit("Evidence escapes repository: " + name)
    return resolved


def verify(record):
    source = json.loads(within_root(record["source_manifest"]).read_text())
    expected = {item["path"]: item["sha256"] for item in source["files"]}
    with zipfile.ZipFile(within_root(record["source_archive"])) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise SystemExit("Archived input list differs from source manifest")
        for name, sha256 in expected.items():
            if hashlib.sha256(bundle.read(name)).hexdigest() != sha256:
                raise SystemExit("Archived input changed: " + name)
        for item in record["files"]:
            if item["path"] in expected:
                # A selected runner/helper is an input too. Verify its frozen
                # bytes, so later source edits cannot invalidate old evidence.
                data = bundle.read(item["path"])
                size, sha256 = len(data), hashlib.sha256(data).hexdigest()
            else:
                path = within_root(item["path"])
                size, sha256 = path.stat().st_size, digest(path)
            if size != item["bytes"] or sha256 != item["sha256"]:
                raise SystemExit("Changed evidence: " + item["path"])


if args.verify:
    verify(json.loads(manifest.read_text()))
    print(f"{args.label}: immutable input archive and completed evidence verified")
    raise SystemExit(0)
if not args.select or manifest.exists() or archive.exists():
    parser.error("select completed evidence; existing archives cannot be overwritten")
source = json.loads(source_path.read_text())
if set(source["features"]) != {"scry", "weld", "servo"}:
    raise SystemExit("Expected all three browser features")
subprocess.run([sys.executable, str(supplier / "fingerprint.py"), "--verify",
                "--label", args.label, "--features", ",".join(source["features"]),
                "--exe", source["exe_path"]], cwd=root, check=True)
selected = {source_path, pathlib.Path(__file__).resolve()}
for name in args.select:
    path = within_root(name)
    candidates = [path] if path.is_file() else path.rglob("*")
    for candidate in candidates:
        if not candidate.is_file() or candidate in (manifest, archive):
            continue
        if any(part in ("profile", ".profiles", "__pycache__") for part in candidate.relative_to(root).parts):
            continue
        selected.add(candidate)
native = []
for name in args.native:
    directory = within_root(name)
    done = directory / "scenario.done"
    process_path = directory / "process-result.json"
    if not {done, process_path}.issubset(selected):
        raise SystemExit("Select the whole completed native directory: " + name)
    process = json.loads(process_path.read_text(encoding="utf-8-sig"))
    result = process["scenario_result"]
    # Windows PowerShell 5 can serialize Get-Content's annotated string as
    # a value/PSPath object. The original completed receipt stays untouched.
    if isinstance(result, dict):
        result = result.get("value")
    if (done.read_text(encoding="utf-8-sig").splitlines()[0] != "RESULT ok"
            or process["native_exit"] != 0 or process["timed_out"]
            or result != "RESULT ok"
            or process["executable_sha256"] != source["exe_sha256"]):
        raise SystemExit("Native completion/executable identity refused: " + name)
    native.append({"directory": name, "native_exit": 0, "scenario_result": "RESULT ok"})
with archive.open("xb") as output:
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for item in source["files"]:
            data = within_root(item["path"]).read_bytes()
            if hashlib.sha256(data).hexdigest() != item["sha256"]:
                raise SystemExit("Source changed during archive: " + item["path"])
            bundle.writestr(item["path"], data)
selected.add(archive)
files = []
for path in sorted(selected):
    before = path.stat()
    sha256 = digest(path)
    after = path.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise SystemExit("Evidence still changing: " + str(path))
    files.append({"path": path.relative_to(root).as_posix(), "bytes": after.st_size, "sha256": sha256})
record = {"recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "label": args.label, "source_manifest": source_path.relative_to(root).as_posix(),
          "source_archive": archive.relative_to(root).as_posix(),
          "features": source["features"], "exe_sha256": source["exe_sha256"],
          "explicit_completed_selection": args.select, "native_completion": native,
          "files": files, "limits": ["Profiles excluded", "Native sentinel does not prove pixels or accessibility",
              "Failed controls may be retained; their actual result determines scope",
              "Verification of archived inputs does not qualify later source or executables"]}
verify(record)
with manifest.open("x", encoding="utf-8", newline="\n") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
verify(json.loads(manifest.read_text()))
print(f"{args.label}: {len(source['files'])} archived inputs; {len(files)} completed evidence files")
