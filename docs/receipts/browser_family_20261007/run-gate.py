"""Run one explicitly named gate with preserved logs and a truthful exit result."""
import argparse
import datetime
import json
import os
import pathlib
import re
import subprocess
import sys
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("label")
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9-]{0,63}", args.label) or not args.command:
    parser.error("provide a lowercase unique label and an explicit command")
receipt = pathlib.Path(__file__).resolve().parent
root = receipt.parents[2]
outputs = {kind: receipt / f"{args.label}.{kind}" for kind in ("stdout.log", "stderr.log", "result.json")}
if any(path.exists() for path in outputs.values()):
    raise SystemExit("Refusing to overwrite a prior gate")
started = datetime.datetime.now(datetime.timezone.utc).isoformat()
clock = time.monotonic()
with outputs["stdout.log"].open("xb") as stdout, outputs["stderr.log"].open("xb") as stderr:
    completed = subprocess.run(
        args.command, cwd=root, stdout=stdout, stderr=stderr,
        creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS if sys.platform == "win32" else 0,
    )
record = {"command": args.command, "cwd": str(root), "started_utc": started,
          "finished_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
          "elapsed_seconds": time.monotonic() - clock, "exit_code": completed.returncode,
          "priority": "BelowNormal" if sys.platform == "win32" else "inherited"}
record["environment"] = {name: os.environ.get(name) for name in (
    "CEF_PATH", "LIBCLANG_PATH", "CMAKE_GENERATOR", "CARGO_TARGET_DIR",
    "CARGO_BUILD_JOBS", "CARGO_NET_GIT_FETCH_WITH_CLI", "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG",
)}
with outputs["result.json"].open("x", encoding="utf-8", newline="\n") as result:
    json.dump(record, result, indent=2)
    result.write("\n")
print(json.dumps(record))
raise SystemExit(completed.returncode)
