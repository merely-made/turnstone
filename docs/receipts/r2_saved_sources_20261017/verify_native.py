#!/usr/bin/env python3
"""Verify the R2 private saved-source headed receipt without launching Turnstone."""

from __future__ import annotations

import hashlib
import errno
import json
from pathlib import Path
import shutil
import subprocess
import sys


def load(path: Path):
    return json.loads(path.read_text())


def blake3_hex(value: str) -> str:
    """Use an installed BLAKE3 implementation; never substitute another hash."""
    try:
        import blake3  # type: ignore[import-not-found]
    except ImportError:
        binary = shutil.which("b3sum")
        if not binary:
            raise RuntimeError("verifying canonical text hashes requires Python blake3 or b3sum")
        return subprocess.check_output([binary], input=value.encode("utf-8")).decode().split()[0]
    return blake3.blake3(value.encode("utf-8")).hexdigest()


def hash_digest(value: str) -> str:
    """Normalize the supported Eidetic `blake3:<hex>` spelling."""
    if ":" in value:
        algorithm, digest = value.split(":", 1)
        assert algorithm.lower() == "blake3", f"unexpected text-hash algorithm {algorithm}"
        return digest.lower()
    return value.lower()


def png_size(path: Path) -> tuple[int, int]:
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"{path}: not PNG"
    assert len(data) > 128, f"{path}: empty or truncated capture"
    width, height = int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
    assert min(width, height) >= 400, f"{path}: too small for visual review"
    return width, height


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: verify_native.py <receipt-directory>")
    base = Path(sys.argv[1]).resolve(strict=True)
    inputs = load(base / "inputs.json")
    runs = load(base / "runs.json")
    assert [entry["phase"] for entry in runs] == ["seed", "reopen"]
    assert all(entry["exit_code"] == 0 and entry["process"] and entry["process"]["success"] and not entry["process"]["timeout"] for entry in runs)
    assert {entry["process"]["binary_sha256"] for entry in runs} == {inputs["binary_sha256"]}
    for run in runs:
        done = (base / run["phase"] / "scenario.done").read_text()
        assert done.startswith("RESULT ok") and "FAIL:" not in done
        assert "blank=0" in done and "presentation frames=" in done

    stop = load(base / "origin-stop.json")
    assert stop["exited"] and stop["tcp_refused"] and stop["connect_errno"] == errno.ECONNREFUSED, "origin must be dead and TCP connections refused before offline actions"
    stopped = load(base / "fixture-process.json")
    assert stopped["exited"] and stopped["exit_code"] != 0
    seed_requests = load(base / "fixture-requests-after-seed.json")["requests"]
    final_requests = load(base / "fixture-requests-final.json")["requests"]
    paths = {item["path"] for item in seed_requests}
    assert {"/A", "/B", "/X", "/Foreign", "/swap-a"}.issubset(paths)
    assert seed_requests == final_requests, "no origin requests may occur after shutdown, including fresh-process reopen"

    custody = load(base / "custody-process.json")
    assert custody["exited"] and custody["exit_code"] == 0 and custody["original_files_unchanged"]
    assert custody["awake_assertion_process_exited"]

    seed = load(base / "seed/08_offline_A1_reader.saved-sources.json")
    reopened = load(base / "reopen/10_reopen_A1_reader.saved-sources.json")
    foreign = load(base / "seed/04_foreign_session_source.saved-sources.json")
    session = seed["session"]
    assert seed["status"] == "Ready" and reopened["status"] == "Ready"
    assert seed["session"] == reopened["session"]
    assert foreign["session"] != seed["session"]
    assert len(foreign["admissions"]) == 1
    assert len(foreign["entries"]) == 1
    assert foreign["entries"][0]["admission"]["url"].endswith("/Foreign")
    assert "copperfield" in foreign["entries"][0]["content"]["text"]
    assert seed["generation"] != reopened["generation"], "reopen must be a fresh process/runtime generation"

    def exact_a1(reader_record):
        readers = reader_record["readers"]
        assert len(readers) == 1
        reader = readers[0]
        assert reader["phase"] == "Ready" and reader["error"] is None
        assert reader["engine"] == "turnstone.saved-source"
        content = reader["content"]
        assert content["title"].startswith("Source A")
        assert content["origin"].endswith("/A")
        assert "Archive A1 opening." in content["text"]
        quote = "naïve — 東京 🐦"
        expected_passage = "naïve — 東京 🐦 passage records cedar mathematics"
        assert reader["target"]["selector"] is not None
        start, end = reader["target"]["selector"]["start"], reader["target"]["selector"]["end"]
        assert 0 <= start <= end <= len(content["text"])
        assert end - start == len(quote), "selector is measured in Unicode scalar values"
        assert content["text"][start:end] == quote, "selector must index Unicode scalar values, not UTF-8 bytes"
        assert expected_passage in content["text"]
        assert hash_digest(content["text_hash"]) == blake3_hex(content["text"])
        target = reader["target"]
        assert hash_digest(target["text_hash"]) == blake3_hex(content["text"])
        assert target["key"]["session"] == reader_record["session"]
        assert target["key"]["annotation_manifest"]
        assert reader["url"].startswith("mere://saved-source/")
        document = reader.get("rendered_document")
        assert isinstance(document, dict), "reader must resolve into the retained document session"
        assert document["address"] == reader["url"]
        assert document["content_type"] == "text/plain; profile=saved-source"
        assert document["provenance"]["source_kind"] == "turnstone.saved-source"
        assert document["provenance"]["canonical_uri"] == reader["url"]

        def strings(value):
            if isinstance(value, str):
                yield value
            elif isinstance(value, list):
                for child in value:
                    yield from strings(child)
            elif isinstance(value, dict):
                for child in value.values():
                    yield from strings(child)

        rendered = list(strings(document["blocks"]))
        assert content["text"] in rendered, "actual retained document must contain the exact full canonical text"
        assert quote in rendered, "actual retained document must separately reveal the exact selected passage"
        assert "Open original address" in rendered and content["origin"] in rendered
        assert any("Saved version: " + hash_digest(target["key"]["annotation_manifest"])[:12] in value for value in rendered)
        assert hash_digest(content["text_hash"]) in reader["url"], "full canonical-text version remains in exact persisted intent"
        return reader

    seed_reader = exact_a1(seed)
    reopen_reader = exact_a1(reopened)
    assert seed_reader["target"] == reopen_reader["target"], "offline reopen must preserve exact capture identity and selector"
    assert seed_reader["content"] == reopen_reader["content"], "offline reopen must render the same canonical A1 record"
    annotation_hex = seed_reader["target"]["key"]["annotation_manifest"].split(":")[-1]
    assert annotation_hex in seed_reader["url"], "reader route must include the annotation hex identity"

    admissions = seed["admissions"]
    assert len(admissions) == 3, "private session has exactly A1, B1, and A2; visited-only X is absent"
    assert all(entry["title"] and entry["error"] is None for entry in admissions)
    assert all(entry["key"]["session"] == session for entry in admissions)
    assert "orchid-only" not in json.dumps(seed, ensure_ascii=False)
    assert "copperfield" not in json.dumps(seed, ensure_ascii=False), "foreign-session source must not leak into private corpus"

    # Cross-check the normal capture lane to identify A1/B1/A2 without assuming
    # a global ordering among graph members.
    a1_capture = sorted(load(base / "seed/01_A1_saved.source-captures.json"), key=lambda item: item["ordinal"])
    b_capture = sorted(load(base / "seed/02_B_saved.source-captures.json"), key=lambda item: item["ordinal"])
    a2_capture = sorted(load(base / "seed/03_A2_saved.source-captures.json"), key=lambda item: item["ordinal"])
    assert [len(a1_capture), len(b_capture), len(a2_capture)] == [1, 2, 3]
    assert all(item["phase"] == "saved" for group in (a1_capture, b_capture, a2_capture) for item in group)
    by_manifest = {entry["key"]["annotation_manifest"]: entry for entry in admissions}
    a1 = by_manifest[a1_capture[0]["stored"]["annotation_manifest"]]
    b1 = by_manifest[b_capture[1]["stored"]["annotation_manifest"]]
    a2 = by_manifest[a2_capture[2]["stored"]["annotation_manifest"]]
    assert a1["url"].endswith("/A") and b1["url"].endswith("/B") and a2["url"].endswith("/A")
    assert a1["key"]["annotation_manifest"] != a2["key"]["annotation_manifest"]
    assert a1["key"]["annotation_manifest"] != b1["key"]["annotation_manifest"]
    assert len({entry["key"]["annotation_manifest"] for entry in admissions}) == 3
    assert a1["key"]["node"] != b1["key"]["node"]
    assert a1["key"]["node"] == a2["key"]["node"], "same-URL revisit must retain the source member"
    assert a1["occurrence"] == 0 and a2["occurrence"] == 1, "A1/A2 occurrence order within the reused member must be stable"
    assert len({item["request"] for group in (a1_capture, b_capture, a2_capture) for item in group}) == 3
    assert [item["ordinal"] for item in (a1_capture[0], b_capture[1], a2_capture[2])] == [1, 2, 3]
    a1_entries = [entry for entry in seed["entries"] if entry["admission"]["key"] == a1["key"]]
    assert len(a1_entries) == 1 and a1_entries[0]["error"] is None and a1_entries[0]["content"]
    assert a1_entries[0]["content"]["text"] == seed_reader["content"]["text"]
    assert hash_digest(a1_entries[0]["content"]["text_hash"]) == hash_digest(seed_reader["content"]["text_hash"])
    assert a1_entries[0]["admission"]["key"] == seed_reader["target"]["key"]
    assert a1_capture[0]["stored"]["annotation_manifest"] == a1["key"]["annotation_manifest"]
    assert b_capture[1]["stored"]["annotation_manifest"] == b1["key"]["annotation_manifest"]
    assert a2_capture[2]["stored"]["annotation_manifest"] == a2["key"]["annotation_manifest"]
    assert a2_capture[0]["source"]["body_hash"] != a2_capture[2]["source"]["body_hash"]
    assert a1_entries[0]["content"]["raw_manifest"] == a1_capture[0]["stored"]["raw_manifest"]
    facets_path = base / "profile" / "sessions" / session / "facets.json"
    facets = load(facets_path)
    graph = load(facets_path.with_name("graph.json"))
    graph_nodes = {entry["node_id"]: entry for entry in graph["nodes"]}
    expected_by_node = {}
    for capture in (a1_capture[0], b_capture[1], a2_capture[2]):
        expected_by_node.setdefault(capture["node"], []).append(capture["stored"]["annotation_manifest"])
    for owner_node, refs in expected_by_node.items():
        assert graph_nodes[owner_node]["url"].endswith("/A" if owner_node in {a1_capture[0]["node"], a2_capture[2]["node"]} else "/B")
        assert facets["nodes"][owner_node]["capture.source-annotations/v1"] == refs
    assert expected_by_node[a1["key"]["node"]] == [a1["key"]["annotation_manifest"], a2["key"]["annotation_manifest"]]
    x_nodes = [item for item in graph["nodes"] if item["url"].endswith("/X")]
    assert len(x_nodes) == 1
    x_facts = facets["nodes"].get(x_nodes[0]["node_id"], {})
    assert not x_facts.get("capture.source-annotations/v1") and not x_facts.get("capture.source-requests/v1")

    expected_pngs = {
        "seed/04_foreign_session_source.png", "seed/05_A1_search.png", "seed/06_A1_reader.png",
        "seed/07_offline_A1_search.png", "seed/08_offline_A1_reader.png",
        "reopen/09_reopen_A1_search.png", "reopen/10_reopen_A1_reader.png",
    }
    actual_pngs = {str(path.relative_to(base)) for path in base.glob("seed/*.png")} | {str(path.relative_to(base)) for path in base.glob("reopen/*.png")}
    assert actual_pngs == expected_pngs, f"unexpected visual review captures: {sorted(actual_pngs)}"
    visuals = [{"path": name, "size": png_size(base / name), "sha256": hashlib.sha256((base / name).read_bytes()).hexdigest()} for name in sorted(actual_pngs)]
    result = {
        "result": "pass",
        "verifier_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "headed_processes": 2,
        "binary_sha256": inputs["binary_sha256"],
        "origin_stopped_before_offline_search_and_reader": True,
        "origin_tcp_refused": True,
        "post_stop_requests": 0,
        "private_admissions": [a1["key"]["annotation_manifest"], b1["key"]["annotation_manifest"], a2["key"]["annotation_manifest"]],
        "a1_reader_target": seed_reader["target"],
        "foreign_session_excluded": True,
        "visited_only_x_excluded": True,
        "offline_fresh_process_reader": True,
        "visual_review_captures": visuals,
    }
    (base / "verification.json").write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(json.dumps(result, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
