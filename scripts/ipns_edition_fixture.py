"""Loopback trustless gateway for the Turnstone retained-edition regression."""
import argparse
import base64
import hashlib
import json
import struct
import subprocess
import zlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit


def varint(n):
    if n < 128:
        return bytes([n])
    return bytes([128 | (n & 127)]) + varint(n >> 7)


def field(tag, value):
    if isinstance(value, int):
        return varint(tag << 3) + varint(value)
    return varint(tag << 3 | 2) + varint(len(value)) + value


def base32(data):
    return "b" + base64.b32encode(data).decode().lower().rstrip("=")


def cbor(value):
    if isinstance(value, dict):
        entries = sorted((cbor(k), cbor(v)) for k, v in value.items())
        return bytes([160 + len(entries)]) + b"".join(k + v for k, v in entries)
    if isinstance(value, int):
        return bytes([value])
    major = 0x60 if isinstance(value, str) else 0x40
    data = value.encode() if isinstance(value, str) else value
    n = len(data)
    return (bytes([major + n]) if n < 24 else bytes([major + 24, n])) + data


def png(color):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    header = struct.pack(">IIBBBBB", 32, 32, 8, 2, 0, 0, 0)
    pixels = (b"\0" + bytes(color) * 32) * 32
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b"")


def serve(directory, openssl):
    directory.mkdir(parents=True, exist_ok=True)
    blocks = {}
    def block(codec, payload):
        cid = bytes([1, codec, 0x12, 32]) + hashlib.sha256(payload).digest()
        blocks[base32(cid)] = payload
        return cid
    def folder(entries):
        # UnixFS directory Data.Type = Directory. UnixFS links are ordered by name.
        links = [field(2, field(1, cid) + field(2, name.encode())) for name, cid in sorted(entries.items())]
        return block(0x70, b"".join(links) + field(1, field(1, 1)))

    key = directory / "fixture-key.pem"
    if not key.exists():
        subprocess.run([openssl, "genpkey", "-algorithm", "ED25519", "-out", key], check=True)
    pub = subprocess.check_output([openssl, "pkey", "-in", key, "-pubout", "-outform", "der"])[-32:]
    public_key = field(1, 1) + field(2, pub)
    name = base32(bytes([1, 0x72, 0, len(public_key)]) + public_key)
    records, editions = {}, {}
    for sequence, label in enumerate("AB"):
        marker = (29, 203, 173) if label == "A" else (231, 74, 119)
        ink = "#7652d1" if label == "A" else "#e68520"
        image = png(marker)
        css = ("h1 {font-size:16px;line-height:20px;margin:2px}"
               f".sheet {{background-color:{ink};width:112px;height:24px}}"
               ".emblem {width:48px;height:48px;background-image:url(../art/emblem.png)}").encode()
        html = (f'<!doctype html><html><head><title>Linnet {label}</title><link rel="stylesheet" href="styles/page.css">'
                f'</head><body><h1>Linnet edition {label}</h1><div class="sheet"></div><div class="emblem"></div></body></html>').encode()
        docs = folder({"index.html": block(0x55, html), "styles": folder({"page.css": block(0x55, css)}),
                       "art": folder({"emblem.png": block(0x55, image)})})
        root = base32(folder({"docs": docs}))
        value = f"/ipfs/{root}/docs".encode()
        validity = b"2099-01-01T00:00:00Z"
        data = cbor(dict(Value=value, Validity=validity, ValidityType=0, Sequence=sequence, TTL=0))
        message = directory / "sign-input"
        message.write_bytes(b"ipns-signature:" + data)
        sig = subprocess.check_output([openssl, "pkeyutl", "-sign", "-rawin", "-inkey", key, "-in", message])
        records[label] = (field(1, value) + field(3, 0) + field(4, validity) + field(5, sequence)
                          + field(6, 0) + field(8, sig) + field(9, data))
        editions[label] = {"root": root, "html_sha256": hashlib.sha256(html).hexdigest(),
            "css_sha256": hashlib.sha256(css).hexdigest(), "png_sha256": hashlib.sha256(image).hexdigest()}
    if not (directory / "head").exists():
        (directory / "head").write_text("A")
    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            path = urlsplit(self.path).path
            head = (directory / "head").read_text().strip()
            with (directory / "requests.jsonl").open("a") as log:
                print(json.dumps({"head": head, "path": self.path}), file=log, flush=True)
            if path == "/ipns/" + name:
                data = records[head]
            else:
                data = blocks.get(path.removeprefix("/ipfs/"))
            if data is None:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    (directory / "ready.json").write_text(json.dumps({"url": f"ipns://{name}/index.html",
        "gateway": f"http://127.0.0.1:{server.server_port}", "editions": editions}, indent=2))
    server.serve_forever()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--openssl", default="openssl")
    args = parser.parse_args()
    serve(args.directory, args.openssl)
