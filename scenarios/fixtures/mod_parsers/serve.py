"""Serve this folder on 127.0.0.1 with the fixture parsers' media types (mod projects MP7).

    python scenarios/fixtures/mod_parsers/serve.py 43119
"""
import functools
import http.server
import pathlib
import sys

TYPES = {
    ".notes": "application/x-fixture-notes",
    ".trap": "application/x-fixture-trap",
    ".refuse": "application/x-fixture-refuse",
}


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, **TYPES}


port = int(sys.argv[1]) if len(sys.argv) > 1 else 43119
here = pathlib.Path(__file__).parent
server = http.server.ThreadingHTTPServer(
    ("127.0.0.1", port), functools.partial(Handler, directory=str(here))
)
server.serve_forever()
