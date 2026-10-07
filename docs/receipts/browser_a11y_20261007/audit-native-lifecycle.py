"""Check the retained native functional witnesses, without claiming OS AT."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
NATIVE = ROOT / "docs/receipts/browser_supplier_integration_20261006"


def main():
    out = HERE / "native-lifecycle-audit.json"
    if out.exists():
        raise FileExistsError("Preserving an existing native audit")
    results = {}
    for phase in ("a11y", "a11y-admission"):
        directory = NATIVE / f"native-{phase}-servo"
        inputs = {}
        def read(name):
            path = directory / f"{name}.foreign-a11y.json"
            data = path.read_bytes()
            inputs[str(path.relative_to(ROOT)).replace("\\", "/")] = hashlib.sha256(data).hexdigest()
            return json.loads(data)
        checkpoints = {n: read(n) for n in (
            "a11y-two-pages", "semantics-idle-dom", "semantics-a-navigation",
            "semantics-b-navigation", "semantics-a-closed", "semantics-all-closed",
            "semantics-reopened", "semantics-final-retired")}
        def roots(snapshot):
            return {s["surface"]: s["root_tree_id"] for s in snapshot["surfaces"]}
        initial = roots(checkpoints["a11y-two-pages"])
        nav_a = roots(checkpoints["semantics-a-navigation"])
        nav_b = roots(checkpoints["semantics-b-navigation"])
        assert len(initial) == 2 and set(initial) == set(nav_a) == set(nav_b)
        assert sum(initial[s] != nav_a[s] for s in initial) == 1
        assert sum(nav_a[s] != nav_b[s] for s in initial) == 1
        assert roots(checkpoints["semantics-idle-dom"]) == initial
        closed_a = roots(checkpoints["semantics-a-closed"])
        assert len(closed_a) == 1 and all(nav_b[s] == r for s, r in closed_a.items())
        reopened = roots(checkpoints["semantics-reopened"])
        assert set(reopened) == set(initial)
        assert not set(reopened.values()) & set(nav_b.values())
        for name in ("semantics-all-closed", "semantics-final-retired"):
            snapshot = checkpoints[name]
            assert (snapshot["surface_count"], snapshot["tree_count"], snapshot["node_count"]) == (0, 0, 0)
        results[phase] = dict(passed=True, inputs=inputs, initial_roots=initial,
                              after_a_navigation=nav_a, after_b_navigation=nav_b,
                              reopened_roots=reopened)
    out.write_text(json.dumps({
        "scope": "Native in-process supplier composition lifecycle only; Windows UIA is separately witnessed",
        "runs": results,
    }, indent=2) + "\n", encoding="utf-8")
    print("Both native functional lifecycle witnesses pass identity and retirement audit")


if __name__ == "__main__":
    main()
