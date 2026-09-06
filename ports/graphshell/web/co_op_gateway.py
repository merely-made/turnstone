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
from urllib.request import Request, urlopen


class Peer:
    def __init__(self, args):
        self.args = args
        self.process = None
        self.lock = threading.RLock()
        self.online = True
        self.generation = 0
        self.last_sync_error = None
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
                "comparison": self.comparison, "service": self.last,
                "records": records, "retained_count": self.last.get("retained_count", 0),
                "traffic": self.traffic,
            }

    def network(self, path, data=None):
        if not self.online:
            raise RuntimeError("Peer connection disabled")
        body = None if data is None else json.dumps(data).encode()
        request = Request(f"http://127.0.0.1:{self.args.peer_port}{path}", data=body,
                          headers={"content-type": "application/json"})
        with urlopen(request, timeout=15) as response:
            result = json.load(response)
        self.traffic.append({"direction": "outgoing", "path": path,
                             "bytes": len(body or b""),
                             "sha256": hashlib.sha256(body or b"").hexdigest()})
        return result

    def import_operations(self, operations):
        for operation in operations:
            self.rpc("import", operation_record_hex=operation)

    def sync(self):
        try:
            own = self.rpc("export")["operations"]
            self.network("/wire", {"operations": own})
            other = self.network("/wire")["operations"]
            self.import_operations(other)
            self.last_sync_error = None
        except Exception as error:
            self.last_sync_error = str(error)
            raise

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
            message = "Comparison retained in this peer's signed log."
            if self.online:
                try:
                    self.sync()
                    message += " Synced with the other peer."
                except Exception as error:
                    message += f" Peer sync unavailable: {error}"
        elif action == "sync":
            self.sync()
            message = "Signed operations exchanged; effective projection refreshed."
        elif action == "disconnect":
            self.online = False
            self.close()
            message = "Disconnected. Rust peer exited and released its store."
        elif action == "reopen":
            self.online = False
            self.close()
            self.start()
            self.rpc("status")
            message = "Fresh Rust process reopened its retained work from disk, offline."
        elif action == "connect":
            self.start()
            self.online = True
            self.sync()
            message = "Reconnected and reconciled retained operations."
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

        def do_GET(self):
            try:
                if self.path == "/api/state":
                    self.json_response(peer.state())
                elif self.path in ("/wire", "/invitation"):
                    if not peer.online:
                        raise RuntimeError("Peer disconnected")
                    if self.path == "/wire":
                        result = peer.rpc("export")
                    else:
                        if peer.invitation is None:
                            raise RuntimeError("Founder has not created an invitation")
                        result = {"invitation": peer.invitation}
                    self.json_response(result)
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
                    if not peer.online:
                        raise RuntimeError("Peer disconnected")
                    peer.import_operations(data["operations"])
                    peer.traffic.append({"direction": "incoming", "path": "/wire",
                                         "bytes": length, "sha256": hashlib.sha256(payload).hexdigest()})
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
                self.json_response({"error": str(error)}, 409)

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    try:
        server.serve_forever()
    finally:
        peer.close()


if __name__ == "__main__":
    main()
