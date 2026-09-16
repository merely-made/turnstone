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
    processes, logs, states, steps = [], [], {}, []

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

    def refused(port, name, reason):
        """The action must fail with a 409 naming `reason`; returns that reason."""
        try:
            action(port, name)
        except HTTPError as error:
            assert error.code == 409, error.code
            message = json.loads(error.msg)["error"]
            assert reason in message, message
            return message
        raise AssertionError(f"{name} was not refused as {reason}")

    def snapshot(label):
        pair = [request(args.port + i, "/api/state") for i in range(2)]
        states[label] = pair
        return pair

    def traffic():
        return [len(request(args.port + i, "/api/state")["traffic"]) for i in range(2)]

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

        founder, member = args.port, args.port + 1

        # Leave: history stays, participation is refused as not joined, a new invitation re-joins.
        left = action(member, "leave")
        assert left["lifecycle"] == "left" and left["process_open"] and left["online"]
        assert left["retained_count"] == 2 and left["records"] == []
        before = traffic()
        contribute_refused = refused(member, "contribute", "not joined")
        sync_refused = refused(member, "sync", "not joined")
        after_refused_sync = traffic()
        assert after_refused_sync == before, "a not-joined sync reached the wire"
        founder_sync_refused = refused(founder, "sync", "not joined")
        left_pair = snapshot("left")
        member_incoming = [t for t in left_pair[1]["traffic"] if t["direction"] == "incoming"][-1]
        assert member_incoming["result"].startswith("refused: not joined"), member_incoming
        assert left_pair[1]["lifecycle"] == "left" and left_pair[0]["lifecycle"] == "joined"
        action(member, "join")
        rejoined = snapshot("rejoined")
        assert all(s["lifecycle"] == "joined" and s["retained_count"] == 2 for s in rejoined)
        assert rejoined[0]["records"] == rejoined[1]["records"] and len(rejoined[0]["records"]) == 1
        steps.append({
            "step": "leave_and_rejoin",
            "left_lifecycle": left["lifecycle"], "left_retained_count": left["retained_count"],
            "left_effective_records": len(left["records"]),
            "contribute_refused": contribute_refused, "sync_refused": sync_refused,
            "wire_entries_during_refused_sync": [a - b for a, b in zip(after_refused_sync, before)],
            "founder_sync_into_left_member_refused": founder_sync_refused,
            "member_import_result": member_incoming["result"],
            "rejoined_lifecycles": [s["lifecycle"] for s in rejoined],
            "rejoined_retained_counts": [s["retained_count"] for s in rejoined],
        })

        # Revocation: the founder revokes while the member is offline. The unaware member's
        # later write is retained on both peers but withdrawn, and its next reconnect is refused.
        action(founder, "contribute")
        action(member, "disconnect")
        member_reopened = action(member, "reopen")
        assert member_reopened["last_recheck"]["admitted"] and not member_reopened["online"]
        revoking = request(founder, "/api/action", {"action": "revoke"})
        member_root = member_reopened["service"]["local_root"]
        assert member_root in revoking["state"]["service"]["revoked_members"]
        known = {op["operation_id"] for op in member_reopened["service"]["operations"]}
        later = action(member, "contribute")
        later_ids = [op["operation_id"] for op in later["service"]["operations"] if op["operation_id"] not in known]
        assert len(later_ids) == 1
        later_id = later_ids[0]
        before_admitted = traffic()
        admitted = action(member, "connect")
        after_admitted = traffic()
        assert admitted["last_recheck"]["admitted"]
        assert all(a > b for a, b in zip(after_admitted, before_admitted)), "positive control: an admitted reconnect reaches the wire"
        synced = snapshot("revocation_synced")
        for state in synced:
            authority = {op["operation_id"]: op["authority"] for op in state["service"]["operations"]}
            assert authority[later_id] == "revoked"
            assert [r["id"].split(":")[1] for r in state["records"]] == ["founder"]
            assert member_root in state["service"]["revoked_members"]
        assert synced[0]["retained_count"] == synced[1]["retained_count"] == 4
        assert synced[0]["service"]["operations"] == synced[1]["service"]["operations"]
        assert synced[1]["lifecycle"] == "revoked" and synced[0]["lifecycle"] == "joined"
        revoked_contribute = refused(member, "contribute", "revoked")
        action(member, "disconnect")
        revoked_reopen = refused(member, "reopen", "revoked")
        before = traffic()
        revoked_connect = refused(member, "connect", "revoked")
        after = traffic()
        assert after == before, "a revoked reconnect reached the wire"
        refused_state = snapshot("revoked_reconnect_refused")[1]
        assert not refused_state["online"] and refused_state["process_open"]
        assert refused_state["last_recheck"]["action"] == "connect"
        assert not refused_state["last_recheck"]["admitted"] and refused_state["last_recheck"]["lifecycle"] == "revoked"
        steps.append({
            "step": "revocation",
            "revocation_message": revoking["message"],
            "revoked_member_root": member_root,
            "later_operation_id": later_id,
            "later_operation_authority": [
                {op["operation_id"]: op["authority"] for op in s["service"]["operations"]}[later_id] for s in synced],
            "retained_counts": [s["retained_count"] for s in synced],
            "retained_delegation_counts": [s["service"]["retained_delegation_count"] for s in synced],
            "revoked_authority_counts": [s["service"]["revoked_authority_count"] for s in synced],
            "effective_record_ids": [[r["id"] for r in s["records"]] for s in synced],
            "admitted_reconnect_wire_entries": [a - b for a, b in zip(after_admitted, before_admitted)],
            "lifecycles": [s["lifecycle"] for s in synced],
            "contribute_refused": revoked_contribute, "reopen_refused": revoked_reopen,
            "reconnect_refused": revoked_connect,
            "wire_entries_during_refused_reconnect": [a - b for a, b in zip(after, before)],
            "refused_reconnect_recheck": refused_state["last_recheck"],
        })

        # Expiry: past the grant's expires_at_ms the founder's contribution and reconnect are refused.
        clock = action(founder, "advance_clock")
        assert clock["now_ms"] == 1001 and clock["lifecycle"] == "expired"
        expired_contribute = refused(founder, "contribute", "expired")
        action(founder, "disconnect")
        expired_reopen = refused(founder, "reopen", "expired")
        before = traffic()
        expired_connect = refused(founder, "connect", "expired")
        after = traffic()
        assert after == before, "an expired reconnect reached the wire"
        expired = snapshot("expired_reconnect_refused")[0]
        assert expired["lifecycle"] == "expired" and not expired["online"] and expired["now_ms"] == 1001
        assert expired["last_recheck"]["action"] == "connect" and not expired["last_recheck"]["admitted"]
        assert expired["retained_count"] == 4 and expired["records"] == []
        steps.append({
            "step": "expiry",
            "now_ms": clock["now_ms"], "lifecycle": expired["lifecycle"],
            "contribute_refused": expired_contribute, "reopen_refused": expired_reopen,
            "reconnect_refused": expired_connect,
            "wire_entries_during_refused_reconnect": [a - b for a, b in zip(after, before)],
            "refused_reconnect_recheck": expired["last_recheck"],
            "retained_count": expired["retained_count"],
            "effective_records": len(expired["records"]),
            "pending_authority_count": expired["service"]["pending_authority_count"],
        })

        checks = subprocess.run([binary, "--role", "founder", "--store", str(output / "negative-checks"), "--self-check"], capture_output=True, text=True, check=True)
        negative = json.loads(checks.stdout)
        assert negative["ok"] and len(negative) == 10
        receipt = {"result": "ok", "kind": "two Rust peers in two HTTP gateway processes; no browser automation", "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(), "comparison_sha256": hashlib.sha256(Path(args.comparison).read_bytes()).hexdigest(), "negative_checks": negative, "lifecycle_steps": steps, "states": states}
        (output / "receipt.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        print(json.dumps({"result": "ok", "receipt": str(output / "receipt.json"), "negative_checks": negative, "lifecycle_steps": steps}, indent=2))
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
