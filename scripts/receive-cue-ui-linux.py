#!/usr/bin/env python3
"""Run real Turnstone Cue UI receiving on a hardware Vulkan desktop.

Use a fresh absolute output directory and an already-built executable. Profiles,
raw output and the copied executable stay local; select synthetic evidence for
publication only after inspecting the complete outcome.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import time


def require_directions(checkpoints, required, sources=None):
    """Require sampled content in the named frame direction, within one frame."""
    observed = [{entry["direction"] for entry in entries
                 if entry["scope"] and entry["sampled_layers"] > 0
                 and (sources is None or entry["surface"] in sources.get(name, set()))}
                for name, entries in checkpoints.items()]
    wanted = {"native-frame", "host-device"} if required == "mixed" else {required}
    assert any(wanted <= directions for directions in observed), (
        f"no checkpoint samples required frame directions: {sorted(wanted)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("scenario", type=Path)
    parser.add_argument("run_root", type=Path)
    parser.add_argument("fixture_base")
    parser.add_argument("--budget", type=int, default=480)
    parser.add_argument("--require-direction", choices=["native-frame", "host-device", "mixed"])
    args = parser.parse_args()
    assert sys.platform == "linux", "run on the Linux receiving host"
    assert args.budget > 0
    assert args.binary.is_absolute() and os.access(args.binary, os.X_OK)
    assert args.scenario.is_absolute() and args.scenario.is_file()
    assert args.run_root.is_absolute() and not args.run_root.exists()
    assert os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY"), "a desktop session is required"
    root = args.run_root
    root.mkdir(parents=True)
    receiving = root / "receiving"
    receiving.mkdir()
    exe = root / "turnstone"
    shutil.copy2(args.binary, exe)
    shutil.copy2(args.scenario, root / "scenario.scn")
    shutil.copy2(__file__, root / "driver.py")
    digest = hashlib.sha256(exe.read_bytes()).hexdigest()
    (root / "binary.sha256").write_text(f"{digest}  turnstone\n")
    env = os.environ.copy()
    env.update(WGPU_BACKEND="vulkan", TURNSTONE_ROOT=str(root / "profile"),
               MERE_ROOT=str(root / "mere"), TURNSTONE_SCENARIO=str(root / "scenario.scn"),
               TURNSTONE_CAPTURE_DIR=str(receiving), SERVO_FIXTURE_BASE=args.fixture_base,
               TURNSTONE_A4_DIAGNOSTICS="1", RUST_LOG="turnstone=info,wgpu_core=info")
    result = {"platform": platform.platform(), "binary_sha256": digest,
              "requested_backend": "Vulkan", "budget_seconds": args.budget,
              "session_type": env.get("XDG_SESSION_TYPE"),
              "wayland_display": env.get("WAYLAND_DISPLAY"), "display": env.get("DISPLAY"),
              "vulkan_driver_files": env.get("VK_DRIVER_FILES"),
              "passed": False, "timed_out": False}
    start = time.monotonic()
    with (root / "stdout.log").open("wb") as stdout, (root / "stderr.log").open("wb") as stderr:
        child = subprocess.Popen([str(exe)], env=env, stdout=stdout, stderr=stderr,
                                 start_new_session=True)
        def interrupted(signum, _frame):
            raise InterruptedError(f"receiving driver interrupted by signal {signum}")
        for signum in (signal.SIGTERM, signal.SIGHUP):
            signal.signal(signum, interrupted)
        try:
            result["exit_code"] = child.wait(timeout=args.budget)
        except subprocess.TimeoutExpired:
            result["timed_out"] = True
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
            result["exit_code"] = child.returncode
            result["elapsed_seconds"] = round(time.monotonic() - start, 3)
            (root / "run.json").write_text(json.dumps(result, indent=2) + "\n")
    result["elapsed_seconds"] = round(time.monotonic() - start, 3)
    (root / "run.json").write_text(json.dumps(result, indent=2) + "\n")
    assert not result["timed_out"], "bounded run expired; incomplete evidence preserved"
    assert result["exit_code"] == 0, result
    outcome = receiving / "scenario.done"
    assert outcome.exists(), "process exited without a scenario outcome"
    assert outcome.read_text().splitlines()[0] == "RESULT ok", outcome.read_text()
    frames = list(receiving.glob("*.cue-frames.json"))
    assert frames, "no shared-presentation observations"
    checkpoints = {path.name: json.loads(path.read_text())["entries"] for path in frames}
    entries = [entry for rows in checkpoints.values() for entry in rows]
    actual_sources = {}
    for name in checkpoints:
        path = receiving / name.replace(".cue-frames.json", ".sources.json")
        actual_sources[name] = {row["surface_id"] for row in json.loads(path.read_text())} if path.exists() else set()
    assert any(e["sampled_layers"] > 0 and e["scope"] for e in entries), "no sampled shared frame"
    if args.require_direction:
        require_directions(checkpoints, args.require_direction, actual_sources)
    for path in receiving.glob("*lens.cue-frames.json"):
        assert any(e["sampled_layers"] > 0 and e["scope"]
                   for e in json.loads(path.read_text())["entries"]), f"blank content in lens: {path}"
        lens_sources = path.with_name(path.name.replace(".cue-frames.json", ".sources.json"))
        assert lens_sources.exists() and json.loads(lens_sources.read_text()), f"lens has no actual source rasters: {path}"
        if args.require_direction and args.require_direction != "mixed":
            require_directions({path.name: checkpoints[path.name]}, args.require_direction, actual_sources)
    assert list(receiving.glob("*.png")), "no composed captures"
    sources = [row for path in receiving.glob("*.sources.json") for row in json.loads(path.read_text())]
    assert sources, "no actual receiving adapter observations"
    adapters = {json.dumps(row["adapter"], sort_keys=True) for row in sources}
    for adapter in adapters:
        info = json.loads(adapter)
        assert info["backend"] == "Vulkan", info
        assert info["device_type"] in ("IntegratedGpu", "DiscreteGpu"), info
    result.update(passed=True, adapters=[json.loads(a) for a in sorted(adapters)],
                  required_direction=args.require_direction,
                  observed_directions=sorted({e["direction"] for e in entries if e["scope"] and e["sampled_layers"] > 0}))
    (root / "run.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
