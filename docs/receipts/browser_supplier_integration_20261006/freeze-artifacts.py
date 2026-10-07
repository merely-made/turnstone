"""Archive verified frozen inputs and hash explicitly selected completed evidence.

Run only after the selected logs and native processes have finished. --select is
an explicit declaration that an evidence file/directory is complete; unselected
logs are never swept into a receipt. This script does not run applications or builds.
"""
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
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--phase", required=True, choices=("scry-weld", "all3"))
parser.add_argument("--source-label", required=True, help="Previously frozen executable/source label")
parser.add_argument("--select", action="append", default=[], metavar="COMPLETED_PATH",
                    help="Completed receipt-relative file/directory; repeat for each selection")
parser.add_argument("--running-log", action="append", default=[], metavar="PATH",
                    help="Explicitly exclude a still-running receipt-relative log")
parser.add_argument("--verify", action="store_true", help="Verify the existing archive/evidence manifest only")
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9-]{0,63}", args.source_label):
    parser.error("--source-label must be a lowercase filename component")
artifact_manifest = receipt / f"artifact-manifest-{args.phase}.json"
source_manifest = receipt / f"source-manifest-{args.source_label}.json"
archive = receipt / f"source-inputs-{args.phase}.zip"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def relative_path(directory, value):
    path = pathlib.Path(value)
    if path.is_absolute() or ".." in path.parts:
        raise SystemExit("Selection must remain relative to its owner: " + value)
    target = (directory / path).resolve(strict=True)
    if not target.is_relative_to(directory.resolve()):
        raise SystemExit("Selection escapes its owner: " + value)
    return target


def excluded(path, running):
    relative = path.relative_to(receipt)
    return ("profile" in relative.parts or path in running or
            path.name.startswith(("artifact-manifest", "source-manifest", "source-inputs")))


def frozen_source_record():
    return json.loads(source_manifest.read_text(encoding="utf-8"))


def verify_archive(record):
    source = json.loads((receipt / record["source_manifest"]).read_text(encoding="utf-8"))
    expected = {item["path"]: item["sha256"] for item in source["files"]}
    with zipfile.ZipFile(receipt / record["source_archive"]) as bundle:
        names = bundle.namelist()
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise SystemExit("Archive entries differ from the exact frozen source input list")
        for name, sha256 in expected.items():
            if hashlib.sha256(bundle.read(name)).hexdigest() != sha256:
                raise SystemExit("Archived frozen input differs: " + name)


def verify_artifacts(record):
    for item in record["files"]:
        path = relative_path(receipt, item["path"])
        if path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
            raise SystemExit("Changed completed evidence: " + item["path"])
    verify_archive(record)


if args.verify:
    record = json.loads(artifact_manifest.read_text(encoding="utf-8"))
    if record["phase"] != args.phase or record["source_label"] != args.source_label:
        raise SystemExit("Artifact phase/source label mismatch")
    # This checks archived bytes and immutable evidence, so a later source edit
    # or replacement production executable does not invalidate an earlier phase.
    verify_artifacts(record)
    print(f"{args.phase}: archived frozen inputs and completed evidence hashes verified")
    raise SystemExit(0)

if not args.select:
    parser.error("Freezing requires explicit --select completed evidence paths")
if artifact_manifest.exists() or archive.exists():
    raise SystemExit("Refusing to overwrite a phase archive or artifact manifest")
source = frozen_source_record()
required_features = {"scry", "weld"} if args.phase == "scry-weld" else {"scry", "weld", "servo"}
if set(source["features"]) != required_features:
    raise SystemExit("Frozen source features do not match the selected phase")
# The live input list, every frozen source byte, executable and toolchain must
# pass immediately before the first archive is made. This cannot qualify an
# executable from an earlier phase after the source or executable has changed.
subprocess.run([
    sys.executable, str(receipt / "fingerprint.py"), "--verify",
    "--label", args.source_label, "--features", ",".join(source["features"]),
    "--exe", source["exe_path"],
], cwd=root, check=True)

running = {relative_path(receipt, name) for name in args.running_log}
selected = set()
for name in args.select:
    path = relative_path(receipt, name)
    candidates = [path] if path.is_file() else [item for item in path.rglob("*") if item.is_file()]
    selected.update(item for item in candidates if not excluded(item, running))
if not selected:
    raise SystemExit("Selection contains no completed evidence after exclusions")
native_names = ("native-weld-input", "native-scry-input", "native-scry-restart", "native-weld-permission") if args.phase == "scry-weld" else ("native-servo",)
for name in native_names:
    done = receipt / name / "scenario.done"
    process_file = receipt / name / "process-result.json"
    if done not in selected or process_file not in selected:
        raise SystemExit("Select the completed native gate directory: " + name)
native_directories = sorted({path.parent for path in selected if path.name in ("scenario.done", "process-result.json")})
native_validation = []
for directory in native_directories:
    name = directory.relative_to(receipt).as_posix()
    done = directory / "scenario.done"
    process_file = directory / "process-result.json"
    if done not in selected or process_file not in selected:
        raise SystemExit("Select both scenario.done and process-result.json for native gate: " + name)
    result = done.read_text(encoding="utf-8-sig").splitlines()
    process = json.loads(process_file.read_text(encoding="utf-8"))
    if not result or result[0] != "RESULT ok" or process["native_exit"] != 0 or process["timed_out"] or process["scenario_result"] != "RESULT ok":
        raise SystemExit("Native gate did not complete successfully: " + name)
    if pathlib.Path(process["executable"]).resolve() != pathlib.Path(source["exe_path"]).resolve():
        raise SystemExit("Native gate used a different executable path: " + name)
    hash_fields = [field for field in ("exe_sha256", "executable_sha256") if field in process]
    for field in hash_fields:
        if not isinstance(process[field], str) or process[field].lower() != source["exe_sha256"].lower():
            raise SystemExit("Native gate used a different executable SHA-256: " + name)
    native_validation.append({"directory": name, "native_exit": 0, "scenario_result": "RESULT ok",
                              "executable_hash_fields_verified": hash_fields,
                              "hash_limit": None if hash_fields else "Process receipt has no executable hash; labeled source/executable fingerprint verified before archive"})
for path in selected:
    if path.name.endswith("-tests.log"):
        text = path.read_text(encoding="utf-8-sig")
        if not re.search(r"test result: ok\. [1-9][0-9]* passed; 0 failed;", text) or re.search(r"test result: FAILED", text):
            raise SystemExit("Selected focused test log is not a passing completed gate: " + path.name)
dependency_files = {path for path in (receipt / "dependency-compatibility").rglob("*") if path.is_file() and not excluded(path, running)}
if not dependency_files.issubset(selected):
    raise SystemExit("Select dependency-compatibility to freeze the exact supplier receipts")

# Validate bytes as they are archived, rather than trusting an earlier check
# across a concurrent source edit. Zip contains only frozen paths/raw bytes.
with archive.open("xb") as stream:
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for item in source["files"]:
            path = relative_path(root, item["path"])
            data = path.read_bytes()
            if hashlib.sha256(data).hexdigest() != item["sha256"]:
                raise SystemExit("Frozen source changed during archive: " + item["path"])
            bundle.writestr(item["path"], data)
selected.update((source_manifest, archive, pathlib.Path(__file__).resolve()))
files = []
for path in sorted(selected):
    before = path.stat()
    sha256 = digest(path)
    after = path.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise SystemExit("Selected evidence is still changing: " + str(path))
    files.append({"path": path.relative_to(receipt).as_posix(), "bytes": after.st_size, "sha256": sha256})
record = {
    "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "phase": args.phase, "source_label": args.source_label,
    "source_manifest": source_manifest.name, "source_archive": archive.name,
    "source_commit": source["head"], "features": source["features"], "exe_sha256": source["exe_sha256"],
    "explicit_completed_selection": args.select,
    "explicit_running_log_exclusions": args.running_log,
    "freezer_command": [sys.executable, *sys.argv],
    "freezer_python_version": sys.version,
    "native_gates_validated": native_validation,
    "files": files,
    "limits": ["Profile bytes excluded", "No applications or builds run by freezer", "Completion of explicitly selected non-native logs is declared by the caller", "Archive verification does not claim later working source or executable remains unchanged"],
}
verify_artifacts(record)
with artifact_manifest.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(record, stream, indent=2)
    stream.write("\n")
verify_artifacts(json.loads(artifact_manifest.read_text(encoding="utf-8")))
print(f"{args.phase}: {len(files)} completed evidence files hashed; {len(source['files'])} frozen input paths archived and verified")
