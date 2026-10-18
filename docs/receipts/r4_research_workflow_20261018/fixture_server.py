#!/usr/bin/env python3
"""Local R4 fixture. /swap-a revises A at the same address; all requests are logged."""

from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
from urllib.parse import urlsplit


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ready-file", type=Path, required=True)
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    state_file = args.output / "requests.json"
    state = json.loads(state_file.read_text()) if state_file.exists() else {"a_revision": 1, "requests": []}
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, _format: str, *_args: object) -> None:
            return

        def send(self, code: int, body: bytes) -> None:
            self.send_response(code)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self) -> None:
            path = urlsplit(self.path).path
            with lock:
                state["requests"].append({"method": "GET", "path": path})
                if path == "/swap-a":
                    state["a_revision"] = 2
                revision = state["a_revision"]
                (args.output / "requests.json").write_text(json.dumps(state, indent=2) + "\n")
            if path == "/swap-a":
                self.send_response(303)
                self.send_header("Location", "/A")
                self.send_header("Content-Length", "0")
                self.end_headers()
            elif path == "/A":
                body = (
                    "<!doctype html><meta charset=utf-8><title>Source A</title><main>"
                    "<p>The Pelican observatory's access word is café.</p>"
                    "</main>" if revision == 1 else
                    "<!doctype html><meta charset=utf-8><title>Source A</title><main>"
                    "<h1>Source A revision 2</h1><p>Live A2 replacement. "
                    "The brisk — 大阪 🦊 passage records violet geometry and changing rivers.</p>"
                    "</main>"
                ).encode("utf-8")
                self.send(200, body)
            elif path == "/B":
                self.send(200, "<!doctype html><meta charset=utf-8><title>Source B</title><p>amber harbor main-session sentinel</p>".encode())
            elif path == "/Huge":
                self.send(200, ("<!doctype html><meta charset=utf-8><title>Oversize source</title><p>oversize saved marker " + ("\\ " * 50000) + "</p>").encode())
            elif path == "/Foreign":
                self.send(200, "<!doctype html><meta charset=utf-8><title>Foreign Source</title><p>copperfield foreign-session sentinel</p>".encode())
            elif path == "/X":
                self.send(200, "<!doctype html><meta charset=utf-8><title>Visited only X</title><p>unkept orchid-only sentinel</p>".encode())
            else:
                self.send(404, b"not found")

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    host, port = server.server_address
    args.ready_file.write_text(json.dumps({"base_url": f"http://{host}:{port}", "host": host, "port": port}) + "\n")
    try:
        server.serve_forever(poll_interval=0.1)
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
