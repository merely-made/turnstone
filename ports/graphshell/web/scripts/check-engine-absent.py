#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""List the force engine's symbols in a wasm built with its name section kept.

`cargo tree` shows what a crate may link; this shows what the module does. Build
the viewer with `CARGO_PROFILE_VIEWER_STRIP=none` (so the raw module keeps its
name section), then:

    check-engine-absent.py graphshell_web.wasm            # exits 1 if the engine is in it
    check-engine-absent.py --expect-present named-engine-on.wasm   # the positive control

A symbol is the engine's when its path names rapier2d, parry2d, nalgebra, simba
or glamx, or one of the `seiche` modules that only exist with its `engine`
feature and are not force structs (the simulation, the settle drivers, the
lagged lane, the Worker's host and the actor). The force structs, laws,
overlays and combinators that option (b) keeps in the main module are listed
as "kept", not failed on.
"""

import argparse
import collections
import re
import sys

# Names are read as written (Rust v0 `_R...` or legacy `_ZN...`): every path in
# either mangling is length-prefixed identifiers, so `8rapier2d` names the crate
# and `6seiche10simulation` the `seiche::simulation` module, with no demangler.
CRATE_TOKENS = ("rapier2d", "parry2d", "nalgebra", "simba", "glamx")
# `seiche` modules that exist only with its `engine` feature and are not force
# structs: the simulation and everything that steps it.
SEICHE_ENGINE = (
    "simulation", "settle", "lagged", "sift", "propose", "node_body", "obstacles", "scene_sim",
    "fluid_coupling", "field", "emitter", "sync", "query", "tensor_forces",
)
# Option (b) (Graphshell host plan, T7) keeps the force structs, their `Declared`
# terms, the combinators and the catalog builders in the main module; they are
# listed, not failed on.
SEICHE_KEPT = ("laws", "overlays", "forces", "barnes_hut", "compose", "grouped", "weighted")
ROOT = r"(?:_|ZN)"
CRATE = re.compile(ROOT + "(" + "|".join(f"{len(c)}{c}" for c in CRATE_TOKENS) + ")")
SEICHE = re.compile(
    ROOT + "6seiche(" + "|".join(f"{len(m)}{m}" for m in SEICHE_ENGINE)
    + r"|7runtime6worker|7runtime5actor)"
)
KEPT = re.compile(ROOT + "6seiche(" + "|".join(f"{len(m)}{m}" for m in SEICHE_KEPT) + ")")


def leb(data, i):
    result = shift = 0
    while True:
        byte = data[i]
        i += 1
        result |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return result, i


def function_names(data):
    """The name section's function names (subsection 1), by function index."""
    names = {}
    i = 8
    while i < len(data):
        section = data[i]
        i += 1
        size, j = leb(data, i)
        if section == 0:
            length, k = leb(data, j)
            if data[k:k + length] == b"name":
                k += length
                end = j + size
                while k < end:
                    sub = data[k]
                    k += 1
                    sub_size, body = leb(data, k)
                    if sub == 1:
                        count, p = leb(data, body)
                        for _ in range(count):
                            index, p = leb(data, p)
                            length, p = leb(data, p)
                            names[index] = data[p:p + length].decode("utf8", "replace")
                            p += length
                    k = body + sub_size
        i = j + size
    return names


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("wasm")
    parser.add_argument("--expect-present", action="store_true",
                        help="positive control: exit 1 unless engine symbols are found")
    parser.add_argument("--show", type=int, default=12, help="how many symbols to print")
    args = parser.parse_args()
    data = open(args.wasm, "rb").read()
    names = function_names(data)
    if not names:
        sys.exit(f"{args.wasm} has no name section: build with CARGO_PROFILE_VIEWER_STRIP=none")
    found = []
    by_crate = collections.Counter()
    kept = collections.Counter()
    for name in names.values():
        hit = CRATE.search(name) or SEICHE.search(name)
        if hit:
            found.append(name)
            by_crate[hit.group(1)] += 1
        elif hit := KEPT.search(name):
            kept[hit.group(1)] += 1
    print(f"{args.wasm}: {len(names)} named functions, {len(found)} in the force engine")
    for crate, count in by_crate.most_common():
        print(f"  {count:6d}  {crate}")
    for name in found[: args.show]:
        print("    ", name[:160])
    if kept:
        print("  kept by option (b), not the engine: " + ", ".join(
            f"seiche {name} {count}" for name, count in kept.most_common()))
    if args.expect_present:
        if not found:
            sys.exit("positive control failed: no engine symbols in a module that should hold them")
        return
    if found:
        sys.exit("the force engine is in this module")
    print("no rapier, parry, nalgebra, simba, glamx or seiche engine symbol")


if __name__ == "__main__":
    main()
