#!/usr/bin/env python3
"""Serve the fixed Gemini and Gopher accessibility fixtures.

Supply a local TLS certificate and key. Ports are explicit, configurable, and
printed as scenario environment assignments. The Micron URI is a local captured
page: this fixture does not claim live Reticulum transport or human AT use.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import socketserver
import ssl
import threading

ROOT = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--certificate", required=True)
    parser.add_argument("--key", required=True)
    parser.add_argument("--gemini-port", type=int, default=19770)
    parser.add_argument("--gopher-port", type=int, default=19771)
    parser.add_argument("--reader-port", type=int, default=43118)
    args = parser.parse_args()

    class Gemini(socketserver.StreamRequestHandler):
        def handle(self):
            request = self.rfile.readline(4097)
            if not request.endswith(b"\r\n"):
                return
            print(f"REQUEST gemini {request.decode('utf-8', errors='replace').strip()}", flush=True)
            self.wfile.write(b"20 text/gemini; charset=utf-8\r\n" + (ROOT / "capsule.gmi").read_bytes())

    class Gopher(socketserver.StreamRequestHandler):
        def handle(self):
            request = self.rfile.readline(4097)
            if not request.endswith(b"\r\n"):
                return
            print(f"REQUEST gopher {request.decode('utf-8', errors='replace').strip()}", flush=True)
            body = (ROOT / "menu.gopher").read_bytes().replace(b"\t19771\r\n", f"\t{self.server.server_address[1]}\r\n".encode())
            self.wfile.write(body)

    class Reader(BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path != "/article":
                self.send_error(404)
                return
            body = (ROOT / "article.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
            print(f"REQUEST reader {self.path}", flush=True)

    class Server(socketserver.ThreadingTCPServer):
        allow_reuse_address = True
        daemon_threads = True

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.certificate, args.key)
    with Server(("127.0.0.1", args.gemini_port), Gemini) as gemini, Server(("127.0.0.1", args.gopher_port), Gopher) as gopher, ThreadingHTTPServer(("127.0.0.1", args.reader_port), Reader) as reader:
        gemini.socket = context.wrap_socket(gemini.socket, server_side=True)
        print(f"TURNSTONE_A11Y_GEMINI_PORT={gemini.server_address[1]}", flush=True)
        print(f"TURNSTONE_A11Y_GOPHER_PORT={gopher.server_address[1]}", flush=True)
        print(f"TURNSTONE_A11Y_MICRON_URI={(ROOT / 'page.mu').as_uri()}", flush=True)
        print(f"TURNSTONE_A11Y_READER_PORT={reader.server_address[1]}", flush=True)
        threading.Thread(target=gemini.serve_forever, daemon=True).start()
        threading.Thread(target=reader.serve_forever, daemon=True).start()
        try:
            gopher.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            gemini.shutdown()
            reader.shutdown()


if __name__ == "__main__":
    main()
