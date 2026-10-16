# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Run the I5 native scenario against a test-owned GPU resident.

Build both binaries first. Arguments name the exact compiled inference_route
test executable and the pinned model directory. Private generated profiles stay
under the approved Turnstone target. Evidence is copied only after completion.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time


HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TURNSTONE = Path("C:/t/cargo-targets/turnstone/debug/turnstone.exe")
WORK = Path("C:/t/cargo-targets/turnstone/i5-headed")
NOTES = {
    "/pelican": "The Pelican observatory's access word is amberstone. It has no other access word.",
    "/sandpiper": "The Sandpiper observatory's access word is indigo. It has no other access word.",
}


class Notes(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path not in NOTES:
            self.send_error(404)
            return
        body = ("<!doctype html><html><head><title>Retained observatory note</title>"
                "</head><body><article><p>" + NOTES[self.path] +
                "</p></article></body></html>").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


def wait_file(path, process, seconds=180):
    until = time.monotonic() + seconds
    while not path.exists() or not path.stat().st_size:
        if process.poll() is not None:
            raise RuntimeError(f"Resident exited before {path.name}: {process.returncode}")
        if time.monotonic() > until:
            raise TimeoutError(path)
        time.sleep(.1)


def sha(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("resident_test", type=Path)
    parser.add_argument("model", type=Path)
    args = parser.parse_args()
    # Preserve a failed attempt for diagnosis. This harness never deletes it.
    WORK.mkdir(exist_ok=False)
    control = WORK / "resident"
    control.mkdir()
    env = os.environ.copy()
    env.update(ESP_SMOLLM2_DIR=str(args.model), DJINN_HEADED_DIR=str(control),
               TURNSTONE_ROOT=str(WORK / "profile"), MERE_ROOT=str(WORK / "mere"),
               TURNSTONE_THEME_LIBRARY=str(WORK / "themes.json"),
               TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT="1", RUST_BACKTRACE="1",
               RUST_LOG="warn,turnstone=debug")
    for key in ("TURNSTONE_SURFACE_POLL_MS", "TURNSTONE_A4_DIAGNOSTICS",
                "TURNSTONE_THEME_SCENARIO", "TURNSTONE_THEME_CAPTURE_DIR",
                "TURNSTONE_THEME_RECEIPT"):
        env.pop(key, None)
    web = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Notes)
    threading.Thread(target=web.serve_forever, daemon=True).start()
    env["I5_FIXTURE_ORIGIN"] = f"http://127.0.0.1:{web.server_port}"
    with (WORK / "resident.log").open("w", encoding="utf-8") as log:
        host = subprocess.Popen([str(args.resident_test),
                                 "serve_trained_model_for_headed_turnstone",
                                 "--ignored", "--exact", "--nocapture"],
                                env=env, stdout=log, stderr=subprocess.STDOUT,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        try:
            wait_file(control / "ready-0.json", host)
            env["GRAPHSHELL_APP_ENDPOINT"] = (control / "endpoint.txt").read_text()
            for phase, scenario in enumerate(("answer", "reopen")):
                capture = WORK / scenario
                capture.mkdir()
                env.update(TURNSTONE_SCENARIO=str(HERE / f"{scenario}.scn"),
                           TURNSTONE_CAPTURE_DIR=str(capture))
                with (WORK / f"{scenario}.log").open("w", encoding="utf-8") as app_log:
                    app = subprocess.Popen([str(TURNSTONE)], cwd=REPO, env=env,
                                           stdout=app_log, stderr=subprocess.STDOUT)
                    try:
                        code = app.wait(timeout=600)
                    except subprocess.TimeoutExpired:
                        app.terminate()
                        app.wait(timeout=15)
                        raise
                done = (capture / "scenario.done").read_text(encoding="utf-8")
                if code or not done.startswith("RESULT ok"):
                    raise RuntimeError(f"{scenario} failed: exit={code}\n{done}")
                (control / ("restart.request" if phase == 0 else "stop.request")).write_text("ready\n")
                if phase == 0:
                    wait_file(control / "ready-1.json", host)
            if host.wait(timeout=60):
                raise RuntimeError("Resident assertions failed")
        finally:
            web.shutdown()
            if host.poll() is None:
                host.terminate()
                host.wait(timeout=15)
    destination = HERE / "native"
    destination.mkdir(exist_ok=False)
    for name in ("answer", "reopen"):
        shutil.copytree(WORK / name, destination / name)
        shutil.copy2(WORK / f"{name}.log", destination)
    for name in ("resident.log",):
        shutil.copy2(WORK / name, destination)
    shutil.copy2(control / "resident-receipt.json", destination)
    receipt = {
        "turnstone_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, encoding="utf-8").strip(),
        "turnstone_binary_sha256": sha(TURNSTONE),
        "resident_test_sha256": sha(args.resident_test),
        "cargo_lock_sha256": sha(REPO / "Cargo.lock"),
        "scenarios": {name: sha(HERE / f"{name}.scn") for name in ("answer", "reopen")},
        "fixture_notes": NOTES,
        "boundary": "Separate headed Turnstone and in-process native Djinn resident; pinned trained GPU model; actual app connection; public fictional retained text",
    }
    (destination / "execution.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
