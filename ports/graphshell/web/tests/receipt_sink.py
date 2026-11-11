# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Serve the Graphshell web page and collect a scenario receipt.

    python graphshell-web-sink.py --web <ports/graphshell/web> --out <dir> [--port 8731]

GET serves the web directory as `python -m http.server` would. POST
/scenario-receipt takes the JSON body `loader.js` sends when a scenario
completes and writes, under --out:

    scenario.done   first line `RESULT ok|fail`, then the step log
    result.json     the whole body minus the capture pixels
    <name>.png      each capture, decoded from its data URL

A driver launches a real browser at
`index.html?scenario=<path>&sink=http://127.0.0.1:<port>/scenario-receipt`
and waits for scenario.done. The page drives itself; this only listens.
"""

import argparse
import base64
import json
import pathlib
import re
import sys
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--web", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--port", type=int, default=8731)
    parser.add_argument("--allowed-origin")
    args = parser.parse_args()
    allowed_origin = args.allowed_origin or f"http://127.0.0.1:{args.port}"
    origin_match = re.fullmatch(r"http://127\.0\.0\.1:([0-9]+)", allowed_origin)
    if not origin_match or not 1024 <= int(origin_match.group(1)) < 65534:
        parser.error("--allowed-origin must be an exact loopback HTTP origin with a bounded port")
    if allowed_origin != f"http://127.0.0.1:{int(origin_match.group(1))}":
        parser.error("--allowed-origin must use its canonical decimal port")
    web = pathlib.Path(args.web).resolve()
    out = pathlib.Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)

    class Handler(SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(web), **kw)

        def end_headers(self):
            # The receipt profile persists across runs; nothing served here
            # may be reused from the browser's cache on the next one.
            self.send_header("Cache-Control", "no-store")
            if getattr(self, "_cors_allowed", False) and self.command in ("OPTIONS", "POST"):
                self.send_header("Access-Control-Allow-Origin", allowed_origin)
                self.send_header("Vary", "Origin")
                if self.command == "OPTIONS":
                    self.send_header("Access-Control-Allow-Methods", "POST")
                    self.send_header("Access-Control-Allow-Headers", "content-type")
            super().end_headers()

        def log_message(self, fmt, *rest):  # quiet GETs; the receipt is the log
            if self.command == "POST":
                sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % rest))

        def receipt_origin(self):
            return self.headers.get_all("Origin") == [allowed_origin]

        def do_OPTIONS(self):
            self._cors_allowed = False
            if self.path not in ("/scenario-progress", "/scenario-receipt"):
                self.send_error(404)
                return
            if not self.receipt_origin():
                self.send_error(403)
                return
            requested_headers = self.headers.get_all("Access-Control-Request-Headers")
            if (self.headers.get_all("Access-Control-Request-Method") != ["POST"]
                    or requested_headers is None or len(requested_headers) != 1
                    or requested_headers[0].strip().lower() != "content-type"):
                self.send_error(400)
                return
            self._cors_allowed = True
            self.send_response(204)
            self.end_headers()

        def do_POST(self):
            self._cors_allowed = False
            if self.path in ("/scenario-progress", "/scenario-receipt"):
                if not self.receipt_origin():
                    self.send_error(403)
                    return
                if self.headers.get_all("Content-Type") != ["application/json"]:
                    self.send_error(415)
                    return
                self._cors_allowed = True
            if self.path == "/scenario-progress":
                # The page's heartbeat while a run is alive: overwritten each
                # time, so a run that never completes leaves its last state.
                length = int(self.headers.get("content-length", "0"))
                (out / "progress.json").write_bytes(self.rfile.read(length))
                self.send_response(204)
                self.end_headers()
                return
            if self.path != "/scenario-receipt":
                self.send_error(404)
                return
            length = int(self.headers.get("content-length", "0"))
            body = json.loads(self.rfile.read(length))
            captures = body.pop("captures", [])
            names = []
            for capture in captures:
                name = capture["name"]
                header, _, data = capture["dataUrl"].partition(",")
                if not header.startswith("data:image/png"):
                    continue
                (out / f"{name}.png").write_bytes(base64.b64decode(data))
                names.append(name)
            body["capture_files"] = names
            (out / "result.json").write_text(json.dumps(body, indent=2), encoding="utf-8")
            scenario = body.get("scenario") or {}
            result = (scenario.get("result") or {})
            lines = [f"RESULT {result.get('result', 'fail')}"]
            lines.extend(result.get("log") or [])
            (out / "scenario.done").write_text("\n".join(lines) + "\n", encoding="utf-8")
            self.send_response(204)
            self.end_headers()
            sys.stderr.write(f"receipt written to {out} ({lines[0]}, {len(names)} capture(s))\n")

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    sys.stderr.write(f"serving {web} on http://127.0.0.1:{args.port}, receipts to {out}\n")
    server.serve_forever()


if __name__ == "__main__":
    main()
