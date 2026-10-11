#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Serve a separately packaged host and collect its existing scenario receipts.

Browser navigation and input remain with the browser driver. This server binds
only loopback, leaves IndexedDB alone, and never replaces the shared web/pkg.
"""

import argparse
import base64
from datetime import datetime, timezone
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


PROBE = b'''<!doctype html><html><head><title>Forme browser qualification probe</title></head>
<body><h1>Forme browser qualification probe</h1><pre id="probe">Checking WebGPU...</pre>
<script type="module">
const result = {userAgent:navigator.userAgent, platform:navigator.platform,
  viewport:[innerWidth,innerHeight], devicePixelRatio, secureContext:isSecureContext,
  databases:await indexedDB.databases(), webgpu:!!navigator.gpu};
try {
  const adapter = await navigator.gpu?.requestAdapter();
  result.adapter = adapter ? {vendor:adapter.info.vendor, architecture:adapter.info.architecture,
    device:adapter.info.device, description:adapter.info.description,
    features:[...adapter.features]} : null;
} catch(error) { result.error = String(error); }
document.getElementById('probe').textContent = JSON.stringify(result,null,2);
await fetch('/qualification-probe',{method:'POST',headers:{'content-type':'application/json'},
  body:JSON.stringify(result)});
</script></body></html>'''


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, package, output, **kwargs):
        self.package = package
        self.output = output
        super().__init__(*args, **kwargs)

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()

    def translate_path(self, path):
        route = unquote(urlsplit(path).path)
        if route.startswith('/pkg/'):
            target = (self.package / route.removeprefix('/pkg/')).resolve()
            if not target.is_relative_to(self.package):
                return str(self.package / '__refused__')
            return str(target)
        return super().translate_path(path)

    def do_GET(self):
        if urlsplit(self.path).path == '/qualification-probe.html':
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(PROBE)))
            self.end_headers()
            self.wfile.write(PROBE)
            return
        super().do_GET()

    def do_POST(self):
        route = urlsplit(self.path).path
        if route not in ('/qualification-probe', '/scenario-progress', '/scenario-receipt'):
            self.send_error(404)
            return
        length = int(self.headers.get('Content-Length', '0'))
        if not 0 < length <= 64 * 1024 * 1024:
            self.send_error(413)
            return
        try:
            body = json.loads(self.rfile.read(length))
            label = Path(urlsplit(self.headers.get('Referer', '')).query.split('scenario=')[-1].split('&')[0]).stem
            label = re.sub(r'[^a-zA-Z0-9_-]', '_', label) or 'probe'
            directory = self.output / label
            directory.mkdir(parents=True, exist_ok=True)
            for capture in body.pop('captures', []):
                name = re.sub(r'[^a-zA-Z0-9_-]', '_', capture['name'])
                prefix, encoded = capture['dataUrl'].split(',', 1)
                if prefix != 'data:image/png;base64':
                    raise ValueError('expected a PNG capture')
                (directory / f'{name}.png').write_bytes(base64.b64decode(encoded, validate=True))
            body['collected_at_utc'] = datetime.now(timezone.utc).isoformat()
            name = {'/qualification-probe':'environment.json', '/scenario-progress':'progress.json',
                    '/scenario-receipt':'result.json'}[route]
            (directory / name).write_text(json.dumps(body, indent=2) + '\n', encoding='utf-8')
        except (ValueError, KeyError, TypeError) as error:
            self.send_error(400, str(error))
            return
        self.send_response(204)
        self.end_headers()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, required=True)
    args = parser.parse_args()
    package = args.package.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    handler = partial(Handler, directory=str(Path(__file__).parent), package=package, output=output)
    with ThreadingHTTPServer(('127.0.0.1', args.port), handler) as server:
        print(f'Qualification origin: http://127.0.0.1:{args.port}', flush=True)
        server.serve_forever()


if __name__ == '__main__':
    main()
