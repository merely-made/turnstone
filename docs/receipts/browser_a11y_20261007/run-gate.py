#!/usr/bin/env python3
"""Record one source-bound command using the stable browser target."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def inputs():
    paths = subprocess.check_output(
        ["git", "ls-files", "src", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"],
        cwd=ROOT, text=True,
    ).splitlines()
    paths += ["src/foreign_a11y.rs", "src/shell/foreign_a11y.rs",
              "scenarios/browser_servo_a11y_windows.scn",
              "scenarios/browser_servo_a11y_uia_windows.scn",
              "scenarios/fixtures/browser_servo/fixture.css",
              "scenarios/fixtures/browser_servo/a11y.html"]
    return {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
            for p in sorted(set(paths))}


def main():
    name, *command = sys.argv[1:]
    if not name.replace("-", "").isalnum() or not command:
        raise ValueError("Expected a receipt name and command")
    paths = [HERE / f"{name}.{suffix}" for suffix in
             ("stdout.log", "stderr.log", "result.json")]
    if any(p.exists() for p in paths):
        raise FileExistsError("Preserving an existing command receipt")
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR="C:/t/cargo-targets/turnstone",
               CARGO_BUILD_JOBS="1", CARGO_NET_GIT_FETCH_WITH_CLI="true",
               CEF_PATH="C:/Users/mark_/Code/cef-cache/wgpu-weld/154.0.34/cef_windows_x86_64",
               LIBCLANG_PATH="C:/Program Files/LLVM/bin",
               CMAKE_GENERATOR="Visual Studio 17 2022")
    before = inputs()
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    with paths[0].open("wb") as out, paths[1].open("wb") as err:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=out,
                                   stderr=err,
                                   creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        print(f"{name}: owned PID {process.pid}", flush=True)
        code = process.wait()
    after = inputs()
    changed = sorted(p for p in before.keys() | after.keys()
                     if before.get(p) != after.get(p))
    result = dict(command=command, cwd=str(ROOT), started_utc=started,
                  finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  elapsed_seconds=time.monotonic() - clock, exit_code=code,
                  priority="BelowNormal", owned_pid=process.pid,
                  environment={k: env.get(k) for k in (
                      "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CEF_PATH",
                      "LIBCLANG_PATH", "CMAKE_GENERATOR", "RUSTFLAGS",
                      "CARGO_ENCODED_RUSTFLAGS", "CARGO_PROFILE_DEV_DEBUG",
                      "CARGO_PROFILE_TEST_DEBUG")},
                  inputs_before=before, changed_inputs=changed)
    paths[2].write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(f"{name}: exit {code}; changed inputs {changed}", flush=True)
    return code if code else int(bool(changed))


if __name__ == "__main__":
    sys.exit(main())
