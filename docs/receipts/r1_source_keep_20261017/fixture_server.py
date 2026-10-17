#!/usr/bin/env python3
"""Deterministic local HTTP pages for the R1 source-keep headed receipt.

The fixture is intentionally small and uses only Python's standard library.
The /A and /B responses carry unique text. A user-visible link on A replaces
its response at the same address (through an ordinary redirect) so a native
scenario can exercise document-generation invalidation without test-only app
actions. Request metadata is recorded for the independent receipt verifier.
"""

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
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    state = {"a_revision": 1, "requests": []}
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, _format: str, *_args: object) -> None:
            return

        def send(self, code: int, body: bytes, content_type: str = "text/html; charset=utf-8") -> None:
            self.send_response(code)
            self.send_header("Content-Type", content_type)
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
                return
            if path == "/A":
                text = "amberstone source body" if revision == 1 else "blueheron replacement body"
                body = f"""<!doctype html><meta charset="utf-8"><title>Source A</title>
<main><h1>Source A revision {revision}</h1><p>{text}</p>
<p><a href="/swap-a">Refresh this address with its replacement revision</a></p></main>""".encode()
                self.send(200, body)
            elif path == "/B":
                self.send(200, b"<!doctype html><meta charset=utf-8><title>Source B</title><main><h1>Source B</h1><p>copperfield cursor sentinel</p></main>")
            elif path == "/X":
                self.send(200, b"<!doctype html><meta charset=utf-8><title>Visited X</title><main><h1>Visited X</h1><p>unkept orchid-only sentinel</p></main>")
            elif path == "/health":
                self.send(200, b"ready", "text/plain")
            else:
                self.send(404, b"not found", "text/plain")

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    host, port = server.server_address
    args.ready_file.write_text(json.dumps({"base_url": f"http://{host}:{port}"}) + "\n")
    try:
        server.serve_forever(poll_interval=0.1)
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
