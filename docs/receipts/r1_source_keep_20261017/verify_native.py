#!/usr/bin/env python3
"""Independently verify an R1 source-keep headed receipt directory."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys


def load(path: Path):
    return json.loads(path.read_text())


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"{path}: not PNG"
    assert len(data) > 128, f"{path}: empty or truncated capture"
    width = int.from_bytes(data[16:20], "big")
    height = int.from_bytes(data[20:24], "big")
    assert min(width, height) >= 400, f"{path}: too small for visual review"
    return width, height


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify_native.py <receipt-directory>")
    base = Path(sys.argv[1]).resolve(strict=True)
    inputs = load(base / "inputs.json")
    runs = load(base / "runs.json")
    assert [entry["phase"] for entry in runs] == ["seed", "reopen"]
    binaries = set()
    for entry in runs:
        process = entry["process"]
        assert entry["exit_code"] == 0 and process and process["success"] and not process["timeout"]
        binaries.add(process["binary_sha256"])
    assert binaries == {inputs["binary_sha256"]}, "both native processes must use the frozen binary"
    custody = load(base / "custody-process.json")
    assert custody["exited"] and custody["exit_code"] == 0 and custody["awake_assertion_process_exited"]
    assert custody["original_files_unchanged"], "no-wallet fixture state must remain untouched"
    assert inputs.get("keeper_binary_sha256") and (base / "custody-info.json").is_file()

    by_ordinal = lambda path: sorted(load(path), key=lambda item: item["ordinal"])
    first = by_ordinal(base / "seed/01_pointer_saved_A.source-captures.json")
    second = by_ordinal(base / "seed/02_keyboard_saved_A.source-captures.json")
    third = by_ordinal(base / "seed/03_semantic_saved_A.source-captures.json")
    bound = by_ordinal(base / "seed/04_bound_A_saved_with_cursor_B.source-captures.json")
    ab_a = by_ordinal(base / "seed/05_ab_a_stale_action.source-captures.json")
    replaced = by_ordinal(base / "seed/06_replacement_stale_action.source-captures.json")
    final = by_ordinal(base / "seed/07_replaced_A_explicitly_saved.source-captures.json")
    assert [len(first), len(second), len(third), len(bound), len(ab_a), len(replaced), len(final)] == [1, 2, 3, 4, 5, 6, 7]
    assert [item["phase"] for item in final] == ["saved", "saved", "saved", "saved", "failed", "failed", "saved"]
    assert all(item["keep"] is True for item in final), "every R1 request must carry its Keep promotion intent"
    assert all(item["error"] for item in final[4:6]), "stale actions must explain why attachment was refused"
    assert all(item["stored"] is None for item in final[4:6]), "pre-actor stale refusals must not deposit artifacts"
    captures = [item for item in final if item["phase"] == "saved"]
    assert len(captures) == 5
    assert len({item["request"] for item in captures}) == 5, "each deliberate gesture needs its own request identity"
    assert len({item["session"] for item in captures}) == 1
    assert len({item["graph"] for item in captures}) == 1
    assert len({item["node"] for item in captures}) == 1, "cursor-B action must still belong to bound A"
    assert all(item["url"].endswith("/A") for item in captures)
    assert all(item["source"] and item["source"]["hash"] and item["source"]["body_hash"] for item in captures)
    assert len({item["source"]["body_hash"] for item in captures[:4]}) == 1
    assert captures[4]["source"]["body_hash"] != captures[0]["source"]["body_hash"], "same-URL replacement must be a new admitted response"
    assert all(item["stored"]["raw_manifest"] == item["source"]["hash"] for item in captures), "stored raw identity must match the offered exact source"
    expected_refs = [item["stored"]["annotation_manifest"] for item in captures]
    assert all(expected_refs), "every Saved acknowledgement must carry an annotation manifest"

    session = captures[0]["session"]
    node = captures[0]["node"]
    facets_path = base / "profile" / "sessions" / session / "facets.json"
    facets = load(facets_path)
    graph = load(facets_path.with_name("graph.json"))
    graph_nodes = {entry["node_id"]: entry for entry in graph["nodes"]}
    assert graph_nodes[node]["url"] == captures[0]["url"], "capture owner must resolve to A in persisted graph truth"
    for suffix in ("/B", "/X", "/swap-a"):
        others = [entry for entry in graph["nodes"] if entry["url"].endswith(suffix)]
        assert len(others) == 1, f"expected one uncaptured metadata-only member for {suffix}"
        other = facets["nodes"].get(others[0]["node_id"], {})
        assert other.get("turnstone.controls.v1.keep") is not True, f"capture A must not Keep {suffix}"
        assert not other.get("capture.source-annotations/v1"), f"capture A must not attach to {suffix}"
        assert not other.get("capture.source-requests/v1"), f"capture A must not admit a request on {suffix}"
    stored_node = facets["nodes"][node]
    assert stored_node.get("turnstone.controls.v1.keep") is True, "disk owner must retain canonical Keep=true"
    refs = stored_node.get("capture.source-annotations/v1", [])
    assert refs == expected_refs, "owner facet reference order must match the exact five saved manifests"
    ledger = stored_node.get("capture.source-requests/v1", [])
    ledger_by_id = {record["request"]["id"]: record for record in ledger}
    assert len(ledger_by_id) == 5 and len(ledger) == 5
    assert not ({item["request"] for item in final[4:6]} & set(ledger_by_id)), "stale requests must not be acknowledged in the owner ledger"
    assert [ledger_by_id[item["request"]]["stored"]["annotation_manifest"] for item in captures] == refs
    assert all(ledger_by_id[item["request"]]["request"]["node"] == node for item in captures)
    assert all(ledger_by_id[item["request"]]["request"]["session"] == session for item in captures)
    for item in captures:
        durable_request = ledger_by_id[item["request"]]["request"]
        for field in ("graph", "url", "source", "keep"):
            assert durable_request[field] == item[field], f"durable request changed {field}"

    reopened = load(base / "reopen/10_reopened_A_kept_and_referenced.source-captures.json")
    reopened_by_id = {item["request"]: item for item in reopened}
    saved_by_id = {item["request"]: item for item in captures}
    assert set(reopened_by_id) == set(saved_by_id), "fresh process must reconstruct the exact five saved request identities"
    for request_id, saved in saved_by_id.items():
        fresh = reopened_by_id[request_id]
        assert fresh["phase"] == "saved" and fresh["error"] is None
        for field in ("session", "graph", "node", "url", "source", "keep", "stored"):
            assert fresh[field] == saved[field], f"fresh process changed saved {field} for request {request_id}"
        assert fresh["keep"] is True and fresh["node"] == node

    after_seed = load(base / "fixture-requests-after-seed.json")["requests"]
    final_requests = load(base / "fixture-requests-final.json")["requests"]
    assert {"/A", "/B", "/X", "/swap-a"}.issubset({request["path"] for request in after_seed})
    assert len(final_requests) == len(after_seed), "fresh reopen must not acquire fixture content"

    expected = {
        "seed/00_A_inspector_ready.png",
        "seed/01_pointer_saved_A.png",
        "seed/02_keyboard_saved_A.png",
        "seed/03_semantic_saved_A.png",
        "seed/04_bound_A_saved_with_cursor_B.png",
        "seed/04_same_url_replaced_A_ready.png",
        "seed/05_ab_a_refused.png",
        "seed/06_replacement_refused.png",
        "seed/07_visited_only_X.png",
        "seed/08_visited_X_address_search.png",
        "seed/09_visited_X_body_search_empty.png",
        "reopen/10_reopened_A_kept_and_referenced.png",
    }
    actual = {str(path.relative_to(base)) for path in base.glob("seed/*.png")} | {
        str(path.relative_to(base)) for path in base.glob("reopen/*.png")
    }
    assert actual == expected, f"expected exactly the visual review set, got {sorted(actual)}"
    captures_png = [
        {"path": name, "size": png_size(base / name), "sha256": hashlib.sha256((base / name).read_bytes()).hexdigest()}
        for name in sorted(actual)
    ]
    assert (base / "fixture-process.json").is_file()
    fixture_process = load(base / "fixture-process.json")
    assert fixture_process["exited"] and fixture_process["exit_code"] != 0

    result = {
        "result": "pass",
        "verifier_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "headed_native_processes": 2,
        "frozen_binary_sha256": inputs["binary_sha256"],
        "isolated_custody_fixture_sha256": inputs["keeper_binary_sha256"],
        "pointer_keyboard_semantic_and_cursor_B_requests": [item["request"] for item in captures],
        "request_owner_node": node,
        "same_owner_saved_manifest_references": refs,
        "stale_action_refusals": 2,
        "address_only_history_control": "positive /X address suggestion; no orchid-only body suggestion",
        "fresh_process_reopened_without_network_requests": True,
        "visual_review_captures": captures_png,
    }
    (base / "verification.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
