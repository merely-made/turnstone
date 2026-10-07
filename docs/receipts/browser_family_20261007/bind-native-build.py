"""Bind a completed native build to unchanged inputs and its actual ANGLE output."""
import argparse
import datetime
import hashlib
import json
import pathlib
import re
import shutil

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("label")
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9-]{0,63}", args.label):
    parser.error("Expected a lowercase receipt label")
receipt = pathlib.Path(__file__).resolve().parent
root = receipt.parents[2]
binding = receipt / f"native-build-binding-{args.label}.json"
if binding.exists():
    raise SystemExit("Refusing to overwrite completed build binding")
result_path = receipt / f"all3-{args.label}-native-build.result.json"
log = receipt / f"all3-{args.label}-native-build.stdout.log"
result = json.loads(result_path.read_text())
if result["exit_code"] != 0:
    raise SystemExit("Native build did not succeed")
pre = json.loads((receipt / f"pre-{args.label}-inputs.json").read_text())


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


for item in pre["files"]:
    path = root / item["path"]
    if path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
        raise SystemExit("Build input changed: " + item["path"])
events = [json.loads(line) for line in log.read_text().splitlines() if line.startswith("{")]
angle = [event for event in events if event.get("reason") == "build-script-executed"
         and "#mozangle@" in event["package_id"]]
if len(angle) != 1:
    raise SystemExit("Expected one exact mozangle build-script event")
target = pathlib.Path(result["environment"]["CARGO_TARGET_DIR"]) / "debug"
dlls = []
for name in ("libEGL.dll", "libGLESv2.dll"):
    source = pathlib.Path(angle[0]["out_dir"]) / name
    staged = target / name
    previous = digest(staged) if staged.exists() else None
    shutil.copyfile(source, staged)
    if digest(staged) != digest(source):
        raise SystemExit("Staged DLL differs from completed build output")
    dlls.append({"name": name, "source": str(source), "staged": str(staged),
                 "sha256": digest(staged), "previous_staged_sha256": previous})
cef_source = pathlib.Path(result["environment"]["CEF_PATH"]) / "libcef.dll"
if digest(cef_source) != digest(target / "libcef.dll"):
    raise SystemExit("Staged CEF differs from the selected SDK")
exe = target / "turnstone.exe"
record = {"recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "completed_build": result, "build_log_sha256": digest(log),
          "pre_native_source_comparison": "PASS", "pre_native_input_count": len(pre["files"]),
          "mozangle_build_event": angle[0], "dlls": dlls,
          "cef_source": str(cef_source), "cef_sha256": digest(cef_source),
          "exe": str(exe), "exe_sha256": digest(exe)}
with binding.open("x", encoding="utf-8", newline="\n") as stream:
    json.dump(record, stream, indent=2)
    stream.write("\n")
print(f"{args.label}: {len(pre['files'])} unchanged inputs; exact native build/DLL/executable binding")
