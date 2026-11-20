#!/usr/bin/env python3
"""Check that a hidden Settings row publishes no accessibility nodes.

Inputs are snapshots written by scenarios/settings_hidden_a11y.scn (run with
TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT=1 so labels are present):
  visible.uxtree.json   Theme row shown, with an unapplied draft
  hidden.uxtree.json    the same row hidden through the arrangement editor
  restored.uxtree.json  the row shown again
  h00..h14.uxtree.json  a Tab walk while the row is hidden

The check is not vacuous: the visible snapshot must contain the row's radio buttons as
focusable nodes and the draft marker, or it fails before looking at the hidden one.
"""
import argparse
import json
import sys
from pathlib import Path

ROW_LABELS = ["Turnstone default", "Merely", "Graphshell night"]
DRAFT_MARKER = "Discard edit"


def load(path):
    tree = json.loads(Path(path).read_text())["trees"][0]
    return tree, {n["node_id"]: n for n in tree["nodes"]}


def texts(node):
    t = node["text"] or {}
    return [v for v in (t.get("label"), t.get("value"), t.get("description")) if v]


def mentions(nodes, needle):
    return [n for n in nodes.values() if any(needle in s for s in texts(n))]


def row_present(nodes):
    problems = []
    for label in ROW_LABELS:
        hits = [n for n in nodes.values()
                if n["role"] == "RadioButton" and (n["text"] or {}).get("label") == label
                and "Focus" in n["actions"]]
        if not hits:
            problems.append(f"no focusable RadioButton {label!r}")
    if not [n for n in nodes.values() if n["role"] == "Button" and (n["text"] or {}).get("label") == DRAFT_MARKER]:
        problems.append(f"no {DRAFT_MARKER!r} button")
    return problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture_dir")
    ap.add_argument("--visible", default="visible")
    ap.add_argument("--hidden", default="hidden")
    ap.add_argument("--restored", default="restored")
    ap.add_argument("--sweep-prefix", default="h")
    ap.add_argument("--sweep-count", type=int, default=15)
    args = ap.parse_args()
    cap = Path(args.capture_dir)
    f = lambda name: cap / f"{name}.uxtree.json"
    failures = []

    _, vis = load(f(args.visible))
    problems = row_present(vis)
    if problems:
        sys.exit("FAIL visible tree is not a valid baseline (check would be vacuous): " + "; ".join(problems))
    _, hid = load(f(args.hidden))
    for needle in ROW_LABELS + [DRAFT_MARKER]:
        for n in mentions(hid, needle):
            if needle == DRAFT_MARKER and (n["text"] or {}).get("label") != DRAFT_MARKER:
                continue
            failures.append(f"hidden tree still publishes {n['role']} {texts(n)[0][:60]!r}")
    if len(hid) >= len(vis):
        failures.append(f"hidden tree has {len(hid)} nodes, visible {len(vis)}")

    _, res = load(f(args.restored))
    for p in row_present(res):
        failures.append("restored tree: " + p)

    seen = set()
    for i in range(args.sweep_count):
        tree, nodes = load(f(f"{args.sweep_prefix}{i:02d}"))
        node = nodes.get(tree["focus_node_id"])
        if node is None:
            failures.append(f"{args.sweep_prefix}{i:02d}: focus is not a published node")
            continue
        seen.add(node["node_id"])
        label = (node["text"] or {}).get("label") or ""
        if any(needle in label for needle in ROW_LABELS):
            failures.append(f"{args.sweep_prefix}{i:02d}: focus on hidden control {label!r}")
    if len(seen) < 5:
        failures.append(f"focus sweep visited only {len(seen)} distinct nodes (vacuous)")

    for m in failures:
        print("FAIL", m)
    if failures:
        sys.exit(1)
    print(f"OK visible {len(vis)} nodes -> hidden {len(hid)} nodes, row absent, restored {len(res)} nodes; "
          f"focus sweep {len(seen)} distinct stops, none hidden")


if __name__ == "__main__":
    main()
