# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Loopback proof carrier. Each process owns one Rust peer and one disk store.

Rust Commons/Gemot validates and retains signed operations. This gateway only
carries their canonical bytes and serves the Graphshell browser host. Fixture
identities and plaintext graph carriage make this unsuitable for deployment.
"""
import argparse
import base64
import hashlib
import json
import re
from pathlib import Path
import subprocess
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError
from urllib.request import Request, urlopen

# Proof-only: one past the fixture grant's expires_at_ms.
PROOF_EXPIRED_MS = 1001
# Reopen still shows a left or never-joined store offline; these are refused.
REFUSED_ON_REOPEN = ("expired", "revoked")


def refusal_reason(error):
    try:
        return json.loads(error.read()).get("error", str(error))
    except Exception:
        return str(error)


class Peer:
    def __init__(self, args):
        self.args = args
        self.process = None
        self.lock = threading.RLock()
        self.online = True
        self.generation = 0
        self.last_sync_error = None
        self.last_recheck = None
        self.last = {}
        self.traffic = []
        self.comparison = json.loads(Path(args.comparison).read_text(encoding="utf-8"))
        self.invitation_path = Path(args.store).with_suffix(".invitation.json")
        self.invitation = json.loads(self.invitation_path.read_text()) if self.invitation_path.exists() else None
        self.start()

    def start(self):
        with self.lock:
            if self.process is not None:
                return
            self.process = subprocess.Popen(
                [self.args.binary, "--role", self.args.role, "--store", self.args.store,
                 "--container", "57" * 32],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                text=True, encoding="utf-8", bufsize=1,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            self.generation += 1

    def rpc(self, cmd, **fields):
        with self.lock:
            if self.process is None:
                raise RuntimeError("Store closed. Reopen it from disk first.")
            self.process.stdin.write(json.dumps({"cmd": cmd, **fields}) + "\n")
            self.process.stdin.flush()
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("Rust peer exited without a response")
            result = json.loads(line)
            if result.get("error") or result.get("ok") is False:
                raise RuntimeError(str(result.get("error", result)))
            return result

    def close(self):
        with self.lock:
            if self.process is not None:
                self.last = self.rpc("status")
                self.rpc("close")
                self.process.wait(timeout=10)
                self.process = None

    def state(self):
        with self.lock:
            if self.process is not None:
                self.last = self.rpc("status")
            records = self.last.get("records", [])
            return {
                "role": self.args.role, "online": self.online,
                "process_open": self.process is not None,
                "process_id": self.process.pid if self.process else None,
                "process_generation": self.generation,
                "last_sync_error": self.last_sync_error,
                "lifecycle": self.last.get("lifecycle"), "now_ms": self.last.get("now_ms"),
                "contract": self.last.get("contract"),
                "last_recheck": self.last_recheck,
                "comparison": self.comparison, "service": self.last,
                "records": records, "retained_count": self.last.get("retained_count", 0),
                "traffic": self.traffic,
            }

    def log(self, direction, method, path, body):
        """Traffic is logged when attempted, so a refusal can prove nothing was sent."""
        entry = {"direction": direction, "method": method, "path": path, "bytes": len(body),
                 "sha256": hashlib.sha256(body).hexdigest(), "result": "pending"}
        self.traffic.append(entry)
        return entry

    def network(self, path, data=None):
        if not self.online:
            raise RuntimeError("Peer connection disabled")
        body = None if data is None else json.dumps(data).encode()
        entry = self.log("outgoing", "GET" if body is None else "POST", path, body or b"")
        request = Request(f"http://127.0.0.1:{self.args.peer_port}{path}", data=body,
                          headers={"content-type": "application/json"})
        try:
            with urlopen(request, timeout=15) as response:
                result = json.load(response)
        except HTTPError as error:
            reason = refusal_reason(error)
            entry["result"] = f"refused: {reason}"
            raise RuntimeError(f"Peer refused {path}: {reason}") from error
        except Exception as error:
            entry["result"] = f"error: {error}"
            raise
        entry["result"] = "ok"
        return result

    def import_operations(self, operations):
        for operation in operations:
            self.rpc("import", operation_record_hex=operation)

    def recheck(self, action):
        """Evaluate installed authority at the store clock before any exchange."""
        try:
            verdict = self.rpc("recheck")
            self.last_recheck = {"action": action, "admitted": True,
                                 "lifecycle": verdict["lifecycle"], "reason": None}
        except RuntimeError as error:
            lifecycle = self.rpc("status")["lifecycle"]
            self.last_recheck = {"action": action, "admitted": False,
                                 "lifecycle": lifecycle, "reason": str(error)}
            if action != "reopen" or lifecycle in REFUSED_ON_REOPEN:
                raise

    def sync(self, check=True):
        try:
            if check:
                self.recheck("sync")
            own = self.rpc("export")["operations"]
            self.network("/wire", {"operations": own})
            other = self.network("/wire")["operations"]
            self.import_operations(other)
            self.last_sync_error = None
        except Exception as error:
            self.last_sync_error = str(error)
            raise

    def try_sync(self):
        if not self.online:
            return ""
        try:
            self.sync()
            return " Synced with the other peer."
        except Exception as error:
            return f" Peer sync unavailable: {error}"

    def action(self, action):
        if action == "found":
            result = self.rpc("init")
            self.invitation = result["invitation"]
            self.invitation_path.parent.mkdir(parents=True, exist_ok=True)
            self.invitation_path.write_text(json.dumps(self.invitation), encoding="utf-8")
            message = "Created space and signed invitation for Bea."
        elif action == "join":
            invitation = self.network("/invitation")["invitation"]
            self.rpc("install_authority", invitation=invitation)
            self.sync()
            message = "Joined using the founder's signed invitation."
        elif action == "contribute":
            payload = json.dumps(self.comparison, sort_keys=True, separators=(",", ":")).encode()
            self.rpc("contribute", comparison_utf8_hex=payload.hex())
            message = "Comparison retained in this peer's signed log." + self.try_sync()
        elif action == "sync":
            self.sync()
            message = "Signed operations exchanged; effective projection refreshed."
        elif action == "leave":
            left = self.rpc("leave")
            message = (f"Left the space. {left['retained_count']} retained operation(s) kept; "
                       "join again with a new invitation.")
        elif action == "advance_clock":
            clock = self.rpc("advance_clock", now_ms=PROOF_EXPIRED_MS)
            message = (f"Proof only: store clock set to {clock['now_ms']} ms, past the grant expiry. "
                       f"Lifecycle: {clock['lifecycle']}.")
        elif action == "revoke":
            if self.args.role != "founder":
                raise ValueError("Only the founder may revoke a member")
            self.rpc("revoke_member")
            message = "Revoked Bea's delegation with a signed Gemot revocation." + self.try_sync()
        elif action == "disconnect":
            self.online = False
            self.close()
            message = "Disconnected. Rust peer exited and released its store."
        elif action == "reopen":
            self.online = False
            self.close()
            self.start()
            self.recheck("reopen")
            message = "Fresh Rust process reopened its retained work from disk, offline."
        elif action == "connect":
            self.start()
            self.online = False
            self.recheck("connect")  # a refused grant never reaches the wire
            self.online = True
            self.sync(check=False)
            message = "Admission rechecked; reconnected and reconciled retained operations."
        else:
            raise ValueError("Unknown action")
        return {"message": message, "state": self.state()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--store", required=True)
    parser.add_argument("--role", choices=["founder", "member"], required=True)
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--peer-port", type=int, required=True)
    parser.add_argument("--assets", required=True)
    parser.add_argument("--comparison", required=True)
    parser.add_argument("--receipts", required=True)
    args = parser.parse_args()
    peer = Peer(args)

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=args.assets, **kw)

        def json_response(self, data, status=200):
            body = json.dumps(data).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("cache-control", "no-store")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def carried(self, method, body, work):
            entry = peer.log("incoming", method, "/wire", body)
            try:
                if not peer.online:
                    raise RuntimeError("Peer disconnected")
                result = work()
            except Exception as error:
                entry["result"] = f"refused: {error}"
                raise
            entry["result"] = "ok"
            return result

        def do_GET(self):
            try:
                if self.path == "/api/state":
                    self.json_response(peer.state())
                elif self.path == "/wire":
                    self.json_response(self.carried("GET", b"", lambda: peer.rpc("export")))
                elif self.path == "/invitation":
                    if not peer.online:
                        raise RuntimeError("Peer disconnected")
                    if peer.invitation is None:
                        raise RuntimeError("Founder has not created an invitation")
                    self.json_response({"invitation": peer.invitation})
                else:
                    super().do_GET()
            except Exception as error:
                self.json_response({"error": str(error)}, 409)

        def do_POST(self):
            try:
                origin = self.headers.get("origin")
                if origin and origin != f"http://127.0.0.1:{args.port}":
                    raise ValueError("Expected this loopback origin")
                if self.headers.get_content_type() != "application/json":
                    raise ValueError("Expected JSON")
                length = int(self.headers.get("content-length", 0))
                if not 0 < length <= 4 * 1024 * 1024:
                    raise ValueError("Invalid request size")
                payload = self.rfile.read(length)
                data = json.loads(payload)
                if self.path == "/api/action":
                    self.json_response(peer.action(data["action"]))
                elif self.path == "/wire":
                    self.carried("POST", payload, lambda: peer.import_operations(data["operations"]))
                    self.json_response({"ok": True})
                elif self.path == "/scenario-receipt":
                    destination = Path(args.receipts)
                    destination.mkdir(parents=True, exist_ok=True)
                    captures = data.get("captures", [])
                    for capture in captures:
                        name = capture["name"]
                        if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
                            raise ValueError("Invalid capture name")
                        (destination / f"{args.role}_{name}.png").write_bytes(
                            base64.b64decode(capture["dataUrl"].split(",", 1)[1]))
                    data["peer_state"] = peer.state()
                    (destination / f"{args.role}_browser.json").write_text(json.dumps(data, indent=2), encoding="utf-8")
                    self.json_response({"ok": True})
                elif self.path == "/scenario-progress":
                    self.json_response({"ok": True})
                else:
                    self.send_error(404)
            except Exception as error:
                body = {"error": str(error)}
                if self.path == "/api/action":
                    try:
                        body["state"] = peer.state()  # refusals still show the verdict
                    except Exception:
                        pass
                self.json_response(body, 409)

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    try:
        server.serve_forever()
    finally:
        peer.close()


if __name__ == "__main__":
    main()
