# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Reproduce the two-process HTTP proof; headed browser verification is separate."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--comparison", required=True)
    parser.add_argument("--assets", required=True)
    parser.add_argument("--output", required=True, help="New directory for stores and receipt")
    parser.add_argument("--port", type=int, default=8868)
    args = parser.parse_args()
    output = Path(args.output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = str(Path(args.binary).resolve())
    processes, logs, states = [], [], {}

    def request(port, path, data=None):
        body = None if data is None else json.dumps(data).encode()
        req = Request(f"http://127.0.0.1:{port}{path}", data=body, headers={"content-type": "application/json"})
        try:
            with urlopen(req, timeout=25) as response:
                return json.load(response)
        except HTTPError as error:
            error.msg = error.read().decode()
            raise

    def action(port, name):
        return request(port, "/api/action", {"action": name})["state"]

    def snapshot(label):
        pair = [request(args.port + i, "/api/state") for i in range(2)]
        states[label] = pair
        return pair

    try:
        for i, role in enumerate(("founder", "member")):
            log = (output / f"{role}.log").open("w", encoding="utf-8")
            logs.append(log)
            processes.append(subprocess.Popen([
                sys.executable, str(Path(__file__).with_name("co_op_gateway.py")),
                "--binary", binary, "--store", str(output / role), "--role", role,
                "--port", str(args.port + i), "--peer-port", str(args.port + 1 - i),
                "--assets", str(Path(args.assets).resolve()),
                "--comparison", str(Path(args.comparison).resolve()),
                "--receipts", str(output / "browser"),
            ], stdout=log, stderr=log, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0)))
        for i in range(2):
            deadline = time.monotonic() + 30
            while True:
                try:
                    request(args.port + i, "/api/state")
                    break
                except Exception:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(.1)

        initial = snapshot("initial")
        assert all(s["retained_count"] == 0 for s in initial)
        try:
            action(args.port + 1, "join")
            raise AssertionError("Member joined before founder created an invitation")
        except HTTPError as error:
            assert error.code == 409
        try:
            action(args.port + 1, "contribute")
            raise AssertionError("Member contributed before admission")
        except HTTPError as error:
            assert error.code == 409
        action(args.port, "found")
        action(args.port + 1, "join")
        action(args.port + 1, "contribute")
        shared = snapshot("shared")
        assert shared[0]["records"] == shared[1]["records"]
        assert all(len(s["records"]) == 1 and s["retained_count"] == 1 for s in shared)
        expected = json.loads(Path(args.comparison).read_text())
        for state in shared:
            payload = bytes.fromhex(state["records"][0]["body_utf8_hex"])
            assert json.loads(payload) == expected
        assert shared[0]["service"]["local_root"] != shared[1]["service"]["local_root"]
        assert shared[0]["service"]["operations"] == shared[1]["service"]["operations"]

        for i in range(2):
            closed = action(args.port + i, "disconnect")
            assert not closed["process_open"] and not closed["online"]
        closed = snapshot("closed")
        for i in range(2):
            action(args.port + i, "reopen")
        reopened = snapshot("reopened_offline")
        for index, state in enumerate(reopened):
            assert state["records"] == shared[index]["records"]
            assert state["process_open"] and not state["online"]
            assert state["traffic"] == closed[index]["traffic"]
            assert state["service"]["authority_installed"]
            assert state["process_id"] != shared[index]["process_id"]
            assert state["process_generation"] == shared[index]["process_generation"] + 1

        # Bring A online; B is still offline, so this first reconciliation must fail.
        try:
            action(args.port, "connect")
            raise AssertionError("Sync succeeded while other peer was offline")
        except HTTPError as error:
            assert error.code == 409
        action(args.port + 1, "connect")
        assert all(s["retained_count"] == 1 for s in snapshot("resynced"))

        # A valid signature alone must not put an outsider's record on the canvas.
        outsider = subprocess.Popen([binary, "--role", "intruder", "--store", str(output / "outsider"), "--container", "57" * 32], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8", creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        commands = [
            {"cmd": "mint_unauthorized", "comparison_utf8_hex": json.dumps(expected).encode().hex()},
            {"cmd": "close"},
        ]
        stdout, _ = outsider.communicate("".join(json.dumps(c) + "\n" for c in commands), timeout=20)
        minted = json.loads(stdout.splitlines()[0])
        assert minted["ok"]
        request(args.port, "/wire", {"operations": [minted["operation_record_hex"]]})
        action(args.port, "sync")
        for state in snapshot("outsider_retained_but_not_effective"):
            assert state["retained_count"] == 2
            assert len(state["records"]) == 1
            assert state["service"]["pending_authority_count"] == 1

        checks = subprocess.run([binary, "--role", "founder", "--store", str(output / "negative-checks"), "--self-check"], capture_output=True, text=True, check=True)
        negative = json.loads(checks.stdout)
        assert negative["ok"]
        receipt = {"result": "ok", "kind": "two Rust peers in two HTTP gateway processes; no browser automation", "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(), "comparison_sha256": hashlib.sha256(Path(args.comparison).read_bytes()).hexdigest(), "negative_checks": negative, "states": states}
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        print(json.dumps({"result": "ok", "receipt": str(output / "receipt.json"), "negative_checks": negative}))
    finally:
        for i, process in enumerate(processes):
            try:
                action(args.port + i, "disconnect")
            except Exception:
                pass
            process.terminate()
            process.wait(timeout=10)
        for log in logs:
            log.close()


if __name__ == "__main__":
    main()
