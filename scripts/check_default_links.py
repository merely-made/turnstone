#!/usr/bin/env python3
"""Check which engines a Turnstone feature selection links.

Reads `cargo metadata` for the turnstone package, walks its normal (non-dev,
non-build) dependency edges and reports the crates found against a named set.
The Reticulum-licensed engines are never part of a default build.

    scripts/check_default_links.py
    scripts/check_default_links.py --features nomadnet --expect retinue

`--expect NAME` is the positive control: it checks only that NAME is reachable,
so CI knows the walker sees an optional engine when its feature is on.
"""

import argparse
import collections
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Reticulum-licensed terms bar uses contributing to model training, so a build
# names these on purpose (`nomadnet`) or not at all.
FORBIDDEN_BY_DEFAULT = [  # a renamed crate needs both names listed
    "retinue",
    "tulle",
    "radio-hand",
    "outrider",
    "sennet",
    "tucket",
    "murm-i2p",
    "emissary",
    "arti-client",
    "arti",
]

# Linked by the default build today and tracked as gaps, not failures: the peer
# transport and the smolweb clients. Printed so a reader sees what a default
# Turnstone still carries.
CARRIED = [
    "mere-transport",
    "mere-p2panda-net",
    "iroh",
    "quinn",
    "mere-webrtc-carrier",
    "murm-webrtc",
    "netfetcher",
    "errand",
    "gemini-protocol",
]


def load_metadata(features):
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
    ]
    if features:
        command += ["--features", features]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"cargo metadata failed:\n{result.stderr}")
    return json.loads(result.stdout)


def reachable(metadata, root_name):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    roots = [i for i, p in packages.items() if p["name"] == root_name and p["source"] is None]
    if not roots:
        sys.exit(f"package {root_name} is not a workspace member")
    parent = {roots[0]: None}
    queue = collections.deque(roots)
    while queue:
        current = queue.popleft()
        for dep in nodes[current]["deps"]:
            if not any(kind["kind"] is None for kind in dep["dep_kinds"]):
                continue
            if dep["pkg"] not in parent:
                parent[dep["pkg"]] = current
                queue.append(dep["pkg"])
    by_name = collections.defaultdict(list)
    for identifier in parent:
        by_name[packages[identifier]["name"]].append(identifier)

    def path(name):
        chain, identifier = [], by_name[name][0]
        while identifier is not None:
            chain.append(packages[identifier]["name"])
            identifier = parent[identifier]
        return " <- ".join(chain)

    return by_name, path


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--features", help="features to enable on turnstone")
    parser.add_argument("--expect", action="append", default=[], help="crate that must be linked")
    args = parser.parse_args()

    by_name, path = reachable(load_metadata(args.features), "turnstone")
    print(f"turnstone{' --features ' + args.features if args.features else ''}: "
          f"{sum(len(v) for v in by_name.values())} packages linked")

    failed = False
    for name in args.expect:
        if name in by_name:
            print(f"  expected {name}: linked ({path(name)})")
        else:
            print(f"  expected {name}: NOT linked")
            failed = True
    if not args.expect:
        for name in FORBIDDEN_BY_DEFAULT:
            if name in by_name:
                print(f"  FORBIDDEN {name}: {path(name)}")
                failed = True
    carried = [name for name in CARRIED if name in by_name]
    if carried:
        print("  still carried: " + ", ".join(carried))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
