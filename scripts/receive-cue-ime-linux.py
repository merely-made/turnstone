#!/usr/bin/env python3
"""Collect physical IME observations on native Wayland, with a disposable profile.

The scenario performs setup and captures only. A human types and selects OS
candidates; native event counts and their checklist remain separate evidence.
Private Wayland logs and desktop captures must be reviewed before publication.
"""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import threading
import time


CHECKS = [
    "I physically typed using the ThinkPad keyboard",
    "Composing text was visible before commit",
    "The OS candidate popup was visible",
    "The candidate popup appeared beside the caret",
    "F1 committed one candidate without duplicate text",
    "Escape cancelled zzcancel without inserting it",
    "Switching away and back left no stale composition",
]
STAGES = [
    ("01", "Address field", "Click the address field in the main Turnstone window."),
    ("02", "Native inline pane", "Click the Observer label field in the Sky pane. The existing observer label is synthetic; append your test word."),
    ("03", "Genet document", "Click the large Document IME field in the main Turnstone window. Use the Other field to check focus changes too."),
    ("04", "Genet document in a lens", "Click the Document IME field in the separate Turnstone lens window. Append your test word. Observe where candidates appear in this window."),
]


def native_events(text):
    events = []
    for line in text.splitlines():
        if "native IME event" not in line:
            continue
        kind = re.search(r'kind="([a-z]+)"', line)
        size = re.search(r"utf8_bytes=(\d+)", line)
        window = re.search(r"window=(\d+)", line)
        if kind and size and window:
            events.append({"kind": kind[1], "utf8_bytes": int(size[1]),
                           "window": int(window[1])})
    return events


def read_native_trace(root, offset=0):
    # tracing_subscriber's default writer is stdout. stderr separately holds
    # WAYLAND_DEBUG; protocol commit() is not an Ime::Commit observation.
    return (root / "stdout.log").read_text(errors="replace")[offset:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("run_root", type=Path)
    parser.add_argument("--stage-budget", type=int, default=300)
    parser.add_argument("--sky-only", action="store_true", help="retest the contributed editor without repeating other human stages")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    root = args.run_root
    assert args.binary.is_absolute() and os.access(args.binary, os.X_OK)
    assert root.is_absolute() and not root.exists()
    assert args.stage_budget > 0 and os.environ.get("WAYLAND_DISPLAY")
    assert os.environ.get("XDG_SESSION_TYPE") == "wayland"
    assert shutil.which("zenity") and shutil.which("ibus")
    root.mkdir(parents=True)
    receiving = root / "receiving"
    receiving.mkdir()
    exchange = root / "exchange"
    exchange.mkdir()
    exe = root / "turnstone"
    shutil.copy2(args.binary, exe)
    shutil.copy2(__file__, root / "driver.py")
    scenario = repo / "scenarios" / ("cue_ime_sky_linux.scn" if args.sky_only else "cue_ime_linux.scn")
    shutil.copy2(scenario, root / "scenario.scn")
    patch = subprocess.check_output(["git", "diff", "--binary"], cwd=repo)
    (root / "source.patch").write_bytes(patch)
    shutil.copy2(repo / "scenarios/fixtures/document_a11y/ime.html", root / "fixture.html")
    result = {"binary_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
              "source_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
              "source_diff_sha256": hashlib.sha256(patch).hexdigest(),
              "receiving_inputs_sha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
                                          for name in ("driver.py", "scenario.scn", "fixture.html")},
              "stages": [], "complete": False, "qualified": False,
              "limits": "Human checklist and native events; app captures do not include OS candidate windows. No CJK, Servo IME, X11 or human accessibility qualification."}
    schema = "org.gnome.desktop.input-sources"
    booster = "org.freedesktop.ibus.engine.typing-booster:/org/freedesktop/ibus/engine/typing-booster/"
    saved = []
    child = dialog = None
    server = None
    engine = subprocess.check_output(["ibus", "engine"], text=True).strip()

    def setting(namespace, key, value):
        previous = subprocess.check_output(["gsettings", "get", namespace, key], text=True).strip()
        saved.append((namespace, key, previous))
        subprocess.run(["gsettings", "set", namespace, key, value], check=True)

    def interrupted(signum, _frame):
        raise InterruptedError(f"physical qualification interrupted by signal {signum}")

    for signum in (signal.SIGHUP, signal.SIGTERM):
        signal.signal(signum, interrupted)
    try:
        setting(schema, "sources", "[('xkb', 'us'), ('ibus', 'typing-booster')]")
        setting(schema, "mru-sources", "[]")
        setting(booster, "offtherecord", "true")
        (root / "input-settings-before.json").write_text(json.dumps({"engine": engine, "settings": saved}, indent=2) + "\n")
        # GNOME owns source selection. Selecting the engine concurrently with
        # its sources refresh can cancel SetGlobalEngine; the human uses the
        # ordinary Super+Space source switch after the window is ready.
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(
            http.server.SimpleHTTPRequestHandler, directory=str(repo / "scenarios/fixtures")))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        env = os.environ.copy()
        env.pop("DISPLAY", None)
        env.update(WGPU_BACKEND="vulkan", WAYLAND_DEBUG="client", NO_COLOR="1",
                   TURNSTONE_ROOT=str(root / "profile"), MERE_ROOT=str(root / "mere"),
                   TURNSTONE_SCENARIO=str(root / "scenario.scn"),
                   TURNSTONE_CAPTURE_DIR=str(receiving), CUE_IME_EXCHANGE=str(exchange),
                   SERVO_FIXTURE_BASE=f"http://127.0.0.1:{server.server_port}",
                   TURNSTONE_A4_DIAGNOSTICS="1", RUST_LOG="turnstone=info,turnstone::ime=debug")
        with (root / "stdout.log").open("w") as stdout, (root / "stderr.log").open("w") as stderr:
            child = subprocess.Popen([str(exe)], cwd=repo, env=env, stdout=stdout, stderr=stderr,
                                     start_new_session=True)
            stages = [s for s in STAGES if s[0] == "02"] if args.sky_only else STAGES
            for stage, title, target in stages:
                deadline = time.monotonic() + args.stage_budget
                while not (exchange / f"{stage}.ready").exists():
                    assert child.poll() is None, "Turnstone exited before stage readiness"
                    assert time.monotonic() < deadline, f"stage {stage} did not become ready"
                    time.sleep(0.1)
                baseline = len(read_native_trace(root))
                expected_window = 1 if stage == "04" else 0
                instructions = root / f"{stage}-instructions.txt"
                instructions.write_text(
                    f"STAGE {stage}: {title}\n\n{target}\n\n"
                    "Leave these instructions open; click the test window to type.\n"
                    "If the input source is US, press Super+Space to select Typing Booster.\n\n"
                    "1. Physically type hello, then pause for two seconds. Look for preedit\n"
                    "   and candidates near the caret. If needed, press Tab for candidates.\n"
                    "2. Press F1 to select the first candidate. Check for duplicate text.\n"
                    "3. Type zzcancel, then Escape. It must not enter the committed value.\n"
                    "4. Start another word; Alt+Tab away and back. Observe stale composition\n"
                    "   and whether blur cancels or commits. Cancel any remaining preedit.\n\n"
                    "Return here and click Stage complete. Record only what you observed\n"
                    "in the checklist. Leave failed/unseen items unchecked. Cancel aborts.\n"
                    "Use synthetic words only. Desktop screenshots, if any, stay private.\n")
                with (root / f"{stage}-dialog.log").open("w") as dialog_log:
                    dialog = subprocess.Popen(["zenity", "--text-info", f"--title=Turnstone IME: {title}",
                        f"--filename={instructions}", "--width=740", "--height=580", "--ok-label=Stage complete"],
                        stdout=dialog_log, stderr=dialog_log)
                    while dialog.poll() is None:
                        assert child.poll() is None, "Turnstone exited during human exercise"
                        assert time.monotonic() < deadline, f"stage {stage} human exercise expired"
                        events = native_events(read_native_trace(root, baseline))
                        if any(e["window"] == expected_window and e["kind"] == "preedit" and e["utf8_bytes"] for e in events):
                            sample = exchange / f"{stage}.sample"
                            if not sample.exists():
                                sample.write_text("native nonempty preedit observed; capture timing remains asynchronous\n")
                        time.sleep(0.1)
                    assert dialog.returncode == 0, f"human aborted stage {stage}"
                flags = subprocess.run(["zenity", "--list", "--checklist", f"--title=Observed: {title}",
                    "--text=Select only what you physically observed; leave failures unchecked.",
                    "--column=Observed", "--column=Check", "--width=860", "--height=420",
                    *[value for check in CHECKS for value in ("FALSE", check)]],
                    text=True, stdout=subprocess.PIPE, timeout=max(1, deadline-time.monotonic()))
                assert flags.returncode == 0, f"human aborted stage {stage} checklist"
                observed = flags.stdout.strip().split("|") if flags.stdout.strip() else []
                events = native_events(read_native_trace(root, baseline))
                events = [e for e in events if e["window"] == expected_window]
                row = {"stage": stage, "title": title, "human_observed": observed,
                       "native_events": events, "expected_window": expected_window,
                       "human_all_checks": set(CHECKS) <= set(observed),
                       "native_preedit": any(e["kind"] == "preedit" and e["utf8_bytes"] for e in events),
                       "native_commit": any(e["kind"] == "commit" and e["utf8_bytes"] for e in events)}
                result["stages"].append(row)
                (root / "run.json").write_text(json.dumps(result, indent=2) + "\n")
                sample = exchange / f"{stage}.sample"
                if not sample.exists():
                    sample.write_text("no native nonempty preedit observed; negative stage capture\n")
                (exchange / f"{stage}.done").write_text("human observations recorded\n")
            result["exit_code"] = child.wait(timeout=60)
            outcome = receiving / "scenario.done"
            result["complete"] = child.returncode == 0 and outcome.exists() and outcome.read_text().startswith("RESULT ok")
            # This is a bounded corroboration gate, not an inference from a
            # green scenario. Retained values/captures still require review.
            result["all_observation_gates"] = result["complete"] and all(
                r["human_all_checks"] and r["native_preedit"] and r["native_commit"] for r in result["stages"])
    except BaseException as error:
        result["error"] = f"{type(error).__name__}: {error}"
        raise
    finally:
        if dialog is not None and dialog.poll() is None:
            dialog.terminate()
            dialog.wait(timeout=5)
        if child is not None and child.poll() is None:
            os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
        if server is not None:
            server.shutdown()
            server.server_close()
        restored = []
        for namespace, key, value in saved:
            status = subprocess.run(["gsettings", "set", namespace, key, value]).returncode
            actual = subprocess.check_output(["gsettings", "get", namespace, key], text=True).strip()
            restored.append({"schema": namespace, "key": key, "exit": status, "value": actual,
                             "matches": actual == value})
        current_engine = subprocess.check_output(["ibus", "engine"], text=True).strip()
        status = 0 if current_engine == engine else subprocess.run(["ibus", "engine", engine]).returncode
        result["restoration"] = {"settings": restored, "engine_exit": status,
            "engine": subprocess.check_output(["ibus", "engine"], text=True).strip()}
        (root / "run.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
