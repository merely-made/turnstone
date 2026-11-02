#!/usr/bin/env python3
"""Enforce the selected package's engine/service composition with Cargo's target tree.

Workspace metadata unifies Turnstone features requested by the Windows sandbox
helper. `cargo tree -p turnstone -e normal --target ...` selects the application
and platform being shipped, omitting dev/build edges and unrelated workspace roots.
"""
import argparse
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Consumers opt in to these implementations; keeping a package in Cargo.lock
# (which resolves all optional features) does not mean it is compiled or linked.
OPTIONAL = {
    "sky": {"turquet"},
    "audio": {"firewheel", "cpal", "symphonia"},
    "authoring": {"knot-editor"},
    "inference": {"distillery"},
    "servo": {"servo", "graft-engine", "grafting-servo"},
    "weld": {"welding", "weld-engine", "cef"},
    "scry": {"scrying-engine", "webview2-com"},
    "nomadnet": {"retinue", "tulle", "radio-hand", "outrider", "sennet", "tucket"},
    "wasm": {"wasmtime", "app-host"},
    "piccolo": {"genet-didge"},
}
REQUIRED = {
    "audio": {"firewheel", "redshank-playback/native"},
    "authoring": {"knot-editor"},
    "peers": {"mere-transport", "graphshell-host/native"},
    "inference": {"distillery"},
    "workshop": {"mere-athanor/workshop-worker", "mere-alembic/workshop-native"},
    "endpoint-process": {"graphshell-host/native"},
    "sky": {"turquet"},
    "nomadnet": {"retinue"},
    "piccolo": {"genet-didge"},
    "wasm": {"wasmtime"},
    "servo": {"servo", "graft-engine"},
    "weld": {"weld-engine"},
}
FORBIDDEN = {"murm-i2p", "emissary", "arti-client", "arti"}
PEER_TRANSPORT = {"mere-transport", "mere-p2panda-net", "iroh", "mere-webrtc-carrier", "murm-webrtc"}


def parse_tree(text):
    packages = {}
    for line in text.splitlines():
        match = re.match(r"^(\S+) v\S+.*? \[([^]]*)\](?: \(\*\))?$", line)
        if match:
            packages.setdefault(match[1], set()).update(filter(None, match[2].split(",")))
    if "turnstone" not in packages:
        raise ValueError("Cargo tree did not contain the selected Turnstone package")
    return packages


def violations(packages):
    selected = packages["turnstone"]
    failures = []
    if "servo-media-player" in packages and not selected & {"audio", "servo"}:
        failures.append("servo-media-player requires audio or servo")
    for feature, required in REQUIRED.items():
        if feature not in selected:
            continue
        for expected in sorted(required):
            package, _, supplier_feature = expected.partition("/")
            if package not in packages or (supplier_feature and supplier_feature not in packages[package]):
                failures.append(f"{feature} must admit {expected}")
    for feature, names in OPTIONAL.items():
        if feature not in selected:
            failures.extend(f"{name} requires {feature}" for name in sorted(names & packages.keys()))
    # Knot bundles the local blob evidence store with its peer assembly;
    # Distillery bundles the installed mesh view with its mesh assembly.
    # Selecting those services admits their supplier assemblies as documented.
    if not selected & {"authoring", "peers", "endpoint-process", "inference"}:
        failures.extend(f"{name} requires a peer-capable service" for name in sorted(PEER_TRANSPORT & packages.keys()))
        for feature in {"native", "webrtc-join", "webrtc-session"} & packages.get("graphshell-host", set()):
            failures.append(f"graphshell-host/{feature} requires a peer-capable service")
    for feature in {"workshop-worker"} & packages.get("mere-athanor", set()):
        if "workshop" not in selected:
            failures.append(f"mere-athanor/{feature} requires workshop")
    if "workshop-native" in packages.get("mere-alembic", set()) and "workshop" not in selected:
        failures.append("mere-alembic/workshop-native requires workshop")
    failures.extend(f"{name} is not admitted" for name in sorted(FORBIDDEN & packages.keys()))
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--features")
    parser.add_argument("--online", action="store_true", help="allow source fetching in CI")
    parser.add_argument("--target", help="target triple; defaults to the active toolchain host")
    parser.add_argument("--expect", action="append", default=[])
    parser.add_argument("--expect-feature", action="append", default=[], metavar="PACKAGE/FEATURE")
    args = parser.parse_args()
    target = args.target
    if not target:
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        target = next(line.split(": ", 1)[1] for line in version.splitlines() if line.startswith("host: "))
    command = ["cargo", "tree", "--manifest-path", str(ROOT / "Cargo.toml"), "--locked", "-p", "turnstone", "-e", "normal", "--target", target, "--prefix", "none", "--format", "{p} [{f}]"]
    if not args.online:
        command.append("--offline")
    if args.features:
        command += ["--features", args.features]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(result.stderr)
    packages = parse_tree(result.stdout)
    failures = violations(packages)
    failures.extend(f"expected {name} is absent" for name in args.expect if name not in packages)
    for expected in args.expect_feature:
        package, feature = expected.split("/", 1)
        if feature not in packages.get(package, set()):
            failures.append(f"expected {expected} is absent")
    print(f"{target}: {len(packages)} package names; Turnstone features: {','.join(sorted(packages['turnstone']))}")
    for failure in failures:
        print("FAIL: " + failure)
    if not failures:
        print("PASS: optional implementation boundaries")
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
