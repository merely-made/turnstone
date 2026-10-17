#!/usr/bin/env python3
"""Run R1 source-keep scenarios in two fresh headed native processes.

This runner never builds the product. It freezes the supplied executable and
scenario inputs, serves local public fixtures, launches each scenario through
the existing LaunchServices driver, and stops at the first unavailable or
timed-out native run. The output directory must be new and outside the repo.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
LAUNCHER = ROOT / "docs/receipts/graph_folds_20261016/run_macos_scenario.py"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_hashes(root: Path) -> dict[str, str]:
    return {
        str(path.relative_to(root)): sha(path)
        for path in root.rglob("*")
        if path.is_file() and path.name != "stop"
    }


def wait_ready(path: Path, proc: subprocess.Popen[str], timeout: float) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            return json.loads(path.read_text())
        if proc.poll() is not None:
            raise RuntimeError(f"fixture server exited with {proc.returncode}")
        time.sleep(0.1)
    raise TimeoutError("fixture server did not publish readiness")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--keeper-binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=Path, required=True)
    parser.add_argument("--reopen", type=Path, required=True)
    parser.add_argument("--timeout", type=int, default=240)
    parser.add_argument("--prefix", default="TURNSTONE")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("headed native receipt requires macOS LaunchServices")
    if not 1 <= args.timeout <= 600:
        parser.error("--timeout must be 1..600 seconds")
    binary = args.binary.resolve(strict=True)
    keeper_binary = args.keeper_binary.resolve(strict=True)
    seed = args.seed.resolve(strict=True)
    reopen = args.reopen.resolve(strict=True)
    out = args.output.resolve()
    if out.exists() or out.is_relative_to(ROOT):
        parser.error("--output must be a new directory outside the repository")
    out.mkdir(parents=True)
    frozen = out / "frozen"
    frozen.mkdir()
    frozen_binary = frozen / "turnstone"
    shutil.copy2(binary, frozen_binary)
    frozen_keeper = frozen / "tucket-custody-receipt-host"
    shutil.copy2(keeper_binary, frozen_keeper)
    frozen_seed = frozen / "r1_seed.scn"
    frozen_reopen = frozen / "r1_reopen.scn"
    shutil.copy2(seed, frozen_seed)
    shutil.copy2(reopen, frozen_reopen)

    tracked = subprocess.check_output(
        ["git", "ls-files", "-c", "-o", "--exclude-standard", "src", "scenarios", "Cargo.toml", "Cargo.lock"],
        cwd=ROOT,
        text=True,
    ).splitlines()
    source_hashes = {p: sha(ROOT / p) for p in sorted(set(tracked)) if (ROOT / p).is_file()}
    receipt = {
        "captured_before_launch_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "binary_sha256": sha(frozen_binary),
        "keeper_binary_sha256": sha(frozen_keeper),
        "seed_scenario_sha256": sha(frozen_seed),
        "reopen_scenario_sha256": sha(frozen_reopen),
        "runner_sha256": sha(Path(__file__)),
        "fixture_sha256": sha(HERE / "fixture_server.py"),
        "source_sha256": source_hashes,
        "timeout_seconds_per_phase": args.timeout,
        "os": subprocess.check_output(["sw_vers"], text=True),
        "architecture": subprocess.check_output(["uname", "-m"], text=True).strip(),
    }
    (out / "inputs.json").write_text(json.dumps(receipt, indent=2) + "\n")

    ready = out / "fixture-ready.json"
    server_log = (out / "fixture-server.log").open("w")
    fixture = subprocess.Popen(
        [sys.executable, str(HERE / "fixture_server.py"), "--output", str(out / "fixture"), "--ready-file", str(ready)],
        stdout=server_log,
        stderr=subprocess.STDOUT,
        text=True,
    )
    keeper_root = out / "custody"
    endpoint = f"/tmp/turnstone-r1-{uuid.uuid4().hex[:12]}.sock"
    keeper_log = (out / "custody-host.log").open("w")
    keeper = subprocess.Popen(
        [str(frozen_keeper), endpoint, str(keeper_root), "no-wallet"],
        stdout=keeper_log,
        stderr=subprocess.STDOUT,
        text=True,
    )
    awake = subprocess.Popen([
        "/usr/bin/caffeinate", "-d", "-i", "-t", str(2 * args.timeout + 60)
    ])
    runs = []
    custody_before: dict[str, str] = {}
    try:
        fixture_info = wait_ready(ready, fixture, 10)
        (out / "fixture-info.json").write_text(json.dumps(fixture_info, indent=2) + "\n")
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if (keeper_root / "ready").exists():
                break
            if keeper.poll() is not None:
                raise RuntimeError(f"isolated custody fixture exited with {keeper.returncode}; see {out / 'custody-host.log'}")
            time.sleep(0.1)
        else:
            raise TimeoutError("isolated custody fixture did not become ready")
        custody_before = tree_hashes(keeper_root)
        (out / "custody-info.json").write_text(json.dumps({"endpoint": endpoint, "root": str(keeper_root), "mode": "no-wallet"}, indent=2) + "\n")
        for phase, scenario in [("seed", frozen_seed), ("reopen", frozen_reopen)]:
            if sha(frozen_binary) != receipt["binary_sha256"]:
                raise RuntimeError("frozen product binary changed between native processes")
            if any(sha(ROOT / p) != h for p, h in source_hashes.items()):
                raise RuntimeError("tracked or untracked source changed during headed qualification")
            phase_dir = out / phase
            command = [
                sys.executable,
                str(LAUNCHER),
                "--binary",
                str(frozen_binary),
                "--prefix",
                args.prefix,
                "--scenario",
                str(scenario),
                "--output",
                str(phase_dir),
                "--timeout",
                str(args.timeout),
                "--env",
                f"{args.prefix}_ROOT={out / 'profile'}",
                "--env",
                f"{args.prefix}_CAPTURE_DIR={phase_dir}",
                "--env",
                f"{args.prefix}_R1_FIXTURE_URL={fixture_info['base_url']}",
                "--env",
                f"{args.prefix}_OFFLINE={'1' if phase == 'reopen' else '0'}",
                "--env",
                f"{args.prefix}_A11Y_PUBLIC_FIXTURE_RECEIPT=1",
                "--env",
                f"{args.prefix}_THEME_LIBRARY={out / 'themes.json'}",
                "--env",
                f"GRAPHSHELL_APP_ENDPOINT={endpoint}",
                "--env",
                f"MERE_ROOT={out / 'mere'}",
                "--env",
                f"APPDATA={out / 'profile'}",
                "--env",
                f"XDG_DATA_HOME={out / 'data'}",
                "--env",
                f"XDG_CONFIG_HOME={out / 'config'}",
                "--env",
                f"XDG_CACHE_HOME={out / 'cache'}",
                "--env",
                "TMPDIR=/tmp",
                "--env",
                "RUST_LOG=warn,turnstone=debug,mesquite=info,taproot=info,swarm_discovery=error",
            ]
            (out / f"{phase}-command.json").write_text(json.dumps(command, indent=2) + "\n")
            log_path = out / f"launch-{phase}.log"
            with log_path.open("w") as log:
                run = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            process_path = phase_dir / "process.json"
            process = json.loads(process_path.read_text()) if process_path.exists() else None
            runs.append({"phase": phase, "exit_code": run.returncode, "process": process})
            (out / "runs.json").write_text(json.dumps(runs, indent=2) + "\n")
            if run.returncode != 0 or not process or not process.get("success"):
                print(f"STOP: {phase} headed process unavailable or unsuccessful; see {log_path}", file=sys.stderr)
                return run.returncode or 1
            if phase == "seed":
                request_log = out / "fixture/requests.json"
                if request_log.exists():
                    shutil.copy2(request_log, out / "fixture-requests-after-seed.json")
        request_log = out / "fixture/requests.json"
        if request_log.exists():
            shutil.copy2(request_log, out / "fixture-requests-final.json")
        print(json.dumps({"result": "native-runs-complete", "runs": runs}, indent=2))
        return 0
    finally:
        if keeper_root.exists():
            (keeper_root / "stop").touch()
        try:
            keeper.wait(timeout=10)
        except subprocess.TimeoutExpired:
            keeper.terminate()
            keeper.wait(timeout=5)
        keeper_log.close()
        awake.terminate()
        awake.wait(timeout=5)
        fixture.terminate()
        try:
            fixture.wait(timeout=5)
        except subprocess.TimeoutExpired:
            fixture.kill()
            fixture.wait(timeout=5)
        server_log.close()
        if fixture.poll() is None:
            raise RuntimeError("fixture process did not exit")
        (out / "fixture-process.json").write_text(json.dumps({"exited": True, "exit_code": fixture.returncode}, indent=2) + "\n")
        custody_after = tree_hashes(keeper_root)
        (out / "custody-process.json").write_text(json.dumps({"exited": keeper.poll() is not None, "exit_code": keeper.returncode, "endpoint": endpoint, "awake_assertion_process_exited": awake.poll() is not None, "before_sha256": custody_before, "after_sha256": custody_after, "original_files_unchanged": all(custody_after.get(path) == digest for path, digest in custody_before.items())}, indent=2) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
