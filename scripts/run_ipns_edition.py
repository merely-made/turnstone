#!/usr/bin/env python3
"""Exercise Keep, offline reopen, and current-version reopen against an isolated custody endpoint on macOS."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
from urllib.parse import urlsplit

from check_ipns_edition import check

SCRIPTS = Path(__file__).resolve().parent
SCENARIOS = SCRIPTS.parent / "scenarios"


def wait_for(path, process, until):
    while not path.is_file():
        if process.poll() is not None or time.monotonic() > until:
            raise RuntimeError(f"process did not produce {path}")
        time.sleep(.05)


def start_gateway(root):
    ready = root / "gateway/ready.json"
    ready.unlink(missing_ok=True)
    with (root / "gateway.log").open("a") as log:
        server = subprocess.Popen([sys.executable, str(SCRIPTS / "ipns_edition_fixture.py"),
                                   str(root / "gateway")], stdout=log, stderr=log)
    try:
        wait_for(ready, server, time.monotonic() + 15)
        return server, json.loads(ready.read_text())
    except BaseException:
        stop(server)
        raise


def stop(server):
    if server.poll() is None:
        server.terminate()
    server.wait(timeout=10)


def requests(root):
    return [json.loads(line) for line in (root / "gateway/requests.jsonl").read_text().splitlines()]


def scenario(args, phase, gateway):
    root = args.output
    move, moved = root / "seed.move", root / "seed.moved"
    env = dict(PEER_NAME=gateway["url"], MERE_IPFS_GATEWAY=gateway["gateway"],
               MOVE_NAME=str(move), NAME_MOVED=str(moved), GRAPHSHELL_APP_ENDPOINT=args.custody,
               TURNSTONE_ROOT=str(root / "profile"), APPDATA=str(root / "profile"),
               MERE_ROOT=str(root / "mere"), XDG_DATA_HOME=str(root / "data"),
               XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"),
               TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT="1", TURNSTONE_OFFLINE="1" if phase == "offline" else "0",
               RUST_LOG="warn,turnstone::selfdrive=debug")
    if phase != "seed":
        env.update(DYLD_INSERT_LIBRARIES=str(root / "network_observer.dylib"),
                   TURNSTONE_NET_RECEIPT=str(root / (phase + ".network.log")))
    cmd = [sys.executable, str(args.launcher), "--binary", str(args.binary), "--prefix", "TURNSTONE",
           "--scenario", str(SCENARIOS / f"ipns_edition_{phase}.scn"), "--output", str(root / phase), "--timeout", "200"]
    for key, value in env.items():
        cmd.extend(["--env", f"{key}={value}"])
    with (root / f"{phase}.log").open("w") as log:
        with subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT) as child:
            if phase == "seed":
                wait_for(move, child, time.monotonic() + 160)
                head = root / "gateway/head"
                next_head = head.with_suffix(".new")
                next_head.write_text("B")
                os.replace(next_head, head)
                moved.write_text(json.dumps({"head": "B", "request_count": len(requests(root))}))
            status = child.wait()
            if status:
                raise RuntimeError(f"{phase} scenario failed, see {root / phase / 'scenario.done'}")


def run(args):
    root = args.output.resolve()
    args.output = root
    root.mkdir(parents=True, exist_ok=False)
    subprocess.run(["cc", "-dynamiclib", str(SCRIPTS / "ipns_network_observer.c"),
                    "-o", str(root / "network_observer.dylib")], check=True)
    server, a = start_gateway(root)
    try:
        scenario(args, "seed", a)
    finally:
        stop(server)
    seed = requests(root)
    moved = json.loads((root / "seed.moved").read_text())
    assert seed and all(row["head"] == "A" for row in seed)
    assert moved["request_count"] == len(seed)
    assert sum(row["path"].startswith("/ipns/") for row in seed) == 1
    capture, = json.loads((root / "seed/kept-a.source-captures.json").read_text())
    assert capture["phase"] == "saved" and capture["url"] == a["url"]
    assert capture["source"]["effective_url"] == f"ipfs://{a['editions']['A']['root']}/docs/index.html"
    port = urlsplit(a["gateway"]).port
    try:
        with socket.create_connection(("127.0.0.1", port), .5):
            raise RuntimeError("gateway still listening")
    except ConnectionRefusedError:
        pass
    (root / "origin-stop.json").write_text(json.dumps(dict(port=port, refused=True, head="B", requests=len(seed))))
    scenario(args, "offline", a)
    assert requests(root) == seed
    readers = json.loads((root / "offline/offline-a.saved-sources.json").read_text())["readers"]
    full, = [r for r in readers if r["engine"] == "genet.livery" and r["target"]["selector"] is None]
    assert full["error"] is None and full["content"]["text"] == "Linnet edition A"
    assert full["content"]["title"] == "Linnet A"
    assert full["content"]["origin"] == capture["source"]["effective_url"]
    assert full["content"]["raw_manifest"] == capture["stored"]["raw_manifest"]
    server, b = start_gateway(root)
    try:
        assert a["url"] == b["url"] and a["editions"] == b["editions"]
        scenario(args, "current", b)
    finally:
        stop(server)
    verify(root)


def verify(root):
    stop_receipt = json.loads((root / "origin-stop.json").read_text())
    assert stop_receipt["refused"] and stop_receipt["head"] == "B"
    rows = requests(root)
    count = stop_receipt["requests"]
    assert json.loads((root / "seed.moved").read_text())["request_count"] == count
    seed, current = rows[:count], rows[count:]
    assert seed and all(row["head"] == "A" for row in seed)
    b = json.loads((root / "gateway/ready.json").read_text())
    assert current and all(row["head"] == "B" for row in current)
    assert any(row["path"].startswith("/ipns/") for row in current)
    verified = check(root, "seed", "offline", "current")
    verified.update(name=b["url"], editions=b["editions"], requests_before_keep=len(seed),
                    gateway_requests_during_keep=0, gateway_requests_while_offline=0,
                    current_gateway_requests=len(current))
    (root / "verification.json").write_text(json.dumps(verified, indent=2) + "\n")
    print(json.dumps(verified, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--launcher", required=True, type=Path, help="Mere's run_macos_scenario.py")
    parser.add_argument("--custody", required=True, help="isolated, unlocked Djinn Unix socket with Turnstone payload custody")
    parser.add_argument("--output", required=True, type=Path, help="new receipt directory")
    run(parser.parse_args())
