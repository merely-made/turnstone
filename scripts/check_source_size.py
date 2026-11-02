#!/usr/bin/env python3
"""Enforce the 600 physical-line ceiling for owned Rust source, including tests."""

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIMIT = 600
SOURCE_ROOTS = ("src", "tests", "examples", "sandbox_bootstrap_win/src")


def violations(root, limit=LIMIT):
    """Inspect disk contents so newly added, untracked source cannot evade the gate."""
    failures = []
    paths = set(root.glob("*.rs")) | set((root / "sandbox_bootstrap_win").glob("*.rs"))
    for directory in SOURCE_ROOTS:
        paths.update((root / directory).rglob("*.rs"))
    for path in sorted(paths):
        lines = len(path.read_bytes().splitlines())
        if lines > limit:
            failures.append((path.relative_to(root), lines))
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    failures = violations(args.root)
    for path, lines in failures:
        print(f"{path}: {lines} lines exceeds the {LIMIT}-line limit")
    if failures:
        print(f"FAIL: {len(failures)} oversized Rust files; no exemptions or baseline allowances")
        return 1
    print(f"PASS: owned Rust source and tests are within {LIMIT} physical lines")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
