#!/usr/bin/env python3
"""Check the keyboard focus path that scenarios/settings_focus.scn recorded.

The scenario writes one redacted-tree snapshot per key press (f00..f19 for the forward
Tab walk, b01..b03 for the Shift+Tab walk back). It must run with
TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT=1 so labels are present. Pass the capture directory.

Checks (all must hold):
  * every snapshot's focus_node_id resolves to a published node;
  * the forward walk visits exactly FORWARD_ORDER, then wraps to its first stop;
  * Shift+Tab walks the same nodes backwards (by node id);
  * every focus stop is on a Settings control (not the window or the outer chrome);
  * focus stops whose node advertises no Focus action are printed as WARN (Undo, Redo and
    Save arrangement are disabled at rest; the Switch advertises no Click or Focus either,
    so this is not a reliable disabled test). --strict-focus-action makes them failures.
"""
import argparse
import json
import sys
from pathlib import Path

FORWARD_ORDER = [
    ("Button", "Reload saved"),
    ("Button", "Edit themes"),
    ("Button", "Discard edits and reload"),
    ("RadioButton", "Turnstone default"),
    ("Button", "Apply"),
    ("RadioButton", "Theme default / system"),
    ("Button", "Apply"),
    ("Slider", "UI zoom"),
    ("Button", "Apply"),
    ("RadioButton", "Left"),
    ("Button", "Apply"),
    ("Switch", "Toggle"),
    ("Button", "Apply"),
    ("Slider", "Behavior cascade rounds"),
    ("Button", "Apply"),
    ("Button", "Edit arrangement"),
    ("Button", "Undo"),
    ("Button", "Redo"),
    ("Button", "Save arrangement"),
    ("Button", "Reload saved"),
]
BACK_STEPS = 3
CHROME_ROLES = {"Window", "Tab", "TabList"}


def load(path):
    tree = json.loads(Path(path).read_text())["trees"][0]
    nodes = {n["node_id"]: n for n in tree["nodes"]}
    return tree, nodes


def focus_of(path):
    tree, nodes = load(path)
    node = nodes.get(tree["focus_node_id"])
    if node is None:
        raise SystemExit(f"FAIL {path}: focus_node_id {tree['focus_node_id']} is not a published node")
    label = (node["text"] or {}).get("label")
    return node, (node["role"], label)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture_dir")
    ap.add_argument("--strict-focus-action", action="store_true")
    args = ap.parse_args()
    cap = Path(args.capture_dir)
    failures, warnings = [], []

    forward = []
    for i in range(len(FORWARD_ORDER)):
        node, key = focus_of(cap / f"f{i:02d}.uxtree.json")
        forward.append((node, key))
        if key != FORWARD_ORDER[i]:
            failures.append(f"f{i:02d}: focus is {key}, expected {FORWARD_ORDER[i]}")
        if key[0] in CHROME_ROLES:
            failures.append(f"f{i:02d}: focus left the Settings controls for {key}")
        if "Focus" not in node["actions"]:
            msg = f"f{i:02d}: focus on {key}, which advertises no Focus action (disabled, or an a11y gap)"
            (failures if args.strict_focus_action else warnings).append(msg)
    if len({n["node_id"] for n, _ in forward[:-1]}) < len(FORWARD_ORDER) - 3:
        failures.append("forward walk visited too few distinct nodes (vacuous)")
    if forward[0][0]["node_id"] != forward[-1][0]["node_id"]:
        failures.append("forward walk did not wrap back to its first stop")

    for j in range(1, BACK_STEPS + 1):
        node, key = focus_of(cap / f"b{j:02d}.uxtree.json")
        want = forward[len(FORWARD_ORDER) - 1 - j][0]
        if node["node_id"] != want["node_id"]:
            failures.append(f"b{j:02d}: Shift+Tab landed on {key}, expected {FORWARD_ORDER[len(FORWARD_ORDER) - 1 - j]}")

    for w in warnings:
        print("WARN", w)
    for f in failures:
        print("FAIL", f)
    if failures:
        sys.exit(1)
    print(f"OK focus walk: {len(forward)} forward stops in order, wrap, {BACK_STEPS} back steps")


if __name__ == "__main__":
    main()
