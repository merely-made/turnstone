#!/usr/bin/env python3
"""Verify a Turnstone diagnostics.json reactive causal chain without mutation.

Produce the export with a headed run of scenarios/settings_reactive.scn, using a fresh
TURNSTONE_ROOT, a TURNSTONE_CAPTURE_DIR, and TURNSTONE_DIAGNOSTIC_RECORDS=65536 and
TURNSTONE_DIAGNOSTIC_BYTES=33554432. Then pass <capture dir>/diagnostics.json here.
The coverage counters stay clean when renderer work escapes its scope; this chain does not.
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict, deque
from pathlib import Path
from typing import Any


def fail(message: str) -> None:
    raise ValueError(message)


def producer_ref(value: Any, label: str) -> tuple[int, int]:
    if not isinstance(value, dict):
        fail(f"{label}: expected ProducerRef object")
    try:
        producer_id = value["producer_id"]
        sequence = value["sequence"]
    except KeyError as exc:
        fail(f"{label}: missing ProducerRef field {exc.args[0]}")
    if type(producer_id) is not int or type(sequence) is not int:
        fail(f"{label}: ProducerRef fields must be integers")
    if producer_id < 0 or sequence <= 0:
        fail(f"{label}: invalid ProducerRef ({producer_id}, {sequence})")
    return producer_id, sequence


def record_ref(value: Any, label: str) -> tuple[str, str, int]:
    if not isinstance(value, dict):
        fail(f"{label}: expected RecordRef object")
    try:
        run, source, sequence = value["run"], value["source"], value["sequence"]
    except KeyError as exc:
        fail(f"{label}: missing RecordRef field {exc.args[0]}")
    if not isinstance(run, str) or not run or not isinstance(source, str) or not source:
        fail(f"{label}: run and source must be nonempty strings")
    if type(sequence) is not int or sequence <= 0:
        fail(f"{label}: sequence must be a positive integer")
    return run, source, sequence


def event_of(record: dict[str, Any]) -> dict[str, Any] | None:
    payload = record.get("payload")
    reactive = payload.get("reactive") if isinstance(payload, dict) else None
    if not isinstance(reactive, dict):
        return None
    union = reactive.get("event")
    if not isinstance(union, dict):
        return None
    source = union.get("source")
    event = union.get("event")
    if source == "cambium" and isinstance(event, dict) and event.get("record") == "decision":
        return event
    return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("diagnostics", type=Path, help="exported diagnostics.json")
    parser.add_argument("--receipt", type=Path, help="optional path for the JSON verification receipt")
    parser.add_argument("--dependency-id", type=int, help="require a specific fixed owner domain id")
    args = parser.parse_args()

    receipt: dict[str, Any] = {
        "schema_version": 1,
        "verifier": "turnstone-native-reactive-chain",
        "qualified": False,
        "input": str(args.diagnostics),
        "chain": [],
        "coverage": {},
        "limitations": [
            "Cause omissions and prior join-reference evictions are reported, not required to be zero.",
            "HostFrameSubmitted means queue submission returned; it does not prove physical presentation or captured pixels.",
            "The causal chain does not establish exact-frame identity across renderer stages.",
            "Renderer event coverage is limited by the exported renderer_scope value.",
        ],
        "errors": [],
    }
    try:
        batch = json.loads(args.diagnostics.read_text(encoding="utf-8"))
        if not isinstance(batch, dict):
            fail("export root must be an ObservationStore batch object")
        records = batch.get("records")
        stats = batch.get("stats")
        if not isinstance(records, list) or not isinstance(stats, dict):
            fail("export must contain records and stats")
        run = stats.get("run")
        source = stats.get("source")
        if not isinstance(run, str) or not run or not isinstance(source, str) or not source:
            fail("batch stats must identify a nonempty run and source")

        records_by_id: dict[tuple[str, str, int], dict[str, Any]] = {}
        decisions: dict[tuple[int, int], dict[str, Any]] = {}
        producer_records: dict[tuple[int, int], tuple[str, str, int]] = {}
        decisions_by_kind: dict[str, list[tuple[int, int]]] = defaultdict(list)
        coverage: list[dict[str, Any]] = []
        resolutions: list[dict[str, Any]] = []
        producer_coverage_failures: list[dict[str, Any]] = []
        for index, record in enumerate(records):
            if not isinstance(record, dict) or not isinstance(record.get("envelope"), dict):
                fail(f"record[{index}] has no envelope")
            rid = record_ref(record["envelope"].get("reference"), f"record[{index}].reference")
            if rid[:2] != (run, source):
                fail(f"record[{index}] belongs to a different run/source")
            if rid in records_by_id:
                fail(f"duplicate retained RecordRef {rid}")
            records_by_id[rid] = record
            reactive = record.get("payload", {}).get("reactive")
            if not isinstance(reactive, dict):
                continue
            union = reactive.get("event")
            if not isinstance(union, dict):
                continue
            payload_source = union.get("source")
            event = union.get("event")
            if payload_source == "coverage" and isinstance(event, dict):
                coverage.append(event)
            elif payload_source == "cause_resolution" and isinstance(event, dict):
                resolutions.append(event)
            event_ref = None
            if payload_source == "cambium" and isinstance(event, dict) and event.get("record") in (
                "decision", "cause_edge", "causes_omitted"
            ):
                event_ref = producer_ref(event.get("reference"), f"record[{index}] producer reference")
            elif payload_source == "meristem" and isinstance(event, dict) and event.get("record") == "decision":
                event_ref = producer_ref(
                    {"producer_id": event.get("producer_id"), "sequence": event.get("sequence")},
                    f"record[{index}] Meristem producer reference",
                )
            if event_ref is not None:
                if event_ref in producer_records:
                    fail(f"duplicate retained producer reference {event_ref}")
                producer_records[event_ref] = rid
            if isinstance(event, dict) and (
                (payload_source == "meristem" and event.get("record") == "coverage_gap")
                or (
                    payload_source == "cambium"
                    and event.get("record") in ("sequence_exhausted", "producer_id_unavailable")
                )
                or (
                    payload_source == "meristem"
                    and event.get("record") == "decision"
                    and event.get("kind") == "coverage_unavailable"
                )
            ):
                producer_coverage_failures.append({"source": payload_source, "event": event})
            decision = event_of(record)
            if decision is not None:
                ref = producer_ref(decision.get("reference"), f"record[{index}] decision reference")
                decisions[ref] = decision
                kind = decision.get("kind")
                if not isinstance(kind, str):
                    fail(f"record[{index}] decision has no kind")
                decisions_by_kind[kind].append(ref)

        if len(coverage) != 1:
            fail(f"expected exactly one reactive coverage record, found {len(coverage)}")
        counters = coverage[0].get("counters")
        if not isinstance(counters, dict):
            fail("coverage event has no counters object")
        receipt["coverage"] = {
            "attached": counters.get("attached"),
            "renderer_scope": counters.get("renderer_scope"),
            "renderer_coverage_gaps": counters.get("renderer_coverage_gaps"),
            "malformed_rejected": counters.get("malformed_rejected"),
            "ingress_full_loss": counters.get("ingress_full_loss"),
            "ingress_disabled_loss": counters.get("ingress_disabled_loss"),
            "ingress_other_loss": counters.get("ingress_other_loss"),
            "store_rejected": counters.get("store_rejected"),
            "causes_omitted": counters.get("causes_omitted"),
            "join_evicted": counters.get("join_evicted"),
            "pending_edge_dropped": counters.get("pending_edge_dropped"),
            "producer_gap_candidates": counters.get("producer_gap_candidates"),
            "producer_gap_late_fills": counters.get("producer_gap_late_fills"),
            "producer_tracker_evicted": counters.get("producer_tracker_evicted"),
            "store_loss": stats.get("loss", {}),
            "store_gaps": batch.get("gaps", []),
            "producer_coverage_failure_records": len(producer_coverage_failures),
            "producer_coverage_failures": producer_coverage_failures,
        }
        required_zero = (
            "malformed_rejected",
            "ingress_full_loss",
            "ingress_disabled_loss",
            "ingress_other_loss",
            "store_rejected",
        )
        for field in required_zero:
            if type(counters.get(field)) is not int or counters[field] != 0:
                fail(f"coverage {field} must be exactly zero")
        if counters.get("attached") is not True:
            fail("reactive receiver was not attached")
        if producer_coverage_failures:
            fail(f"producer reported {len(producer_coverage_failures)} coverage failure record(s)")
        scope = counters.get("renderer_scope")
        if scope != "settings_pane_and_outer_frame":
            fail(f"unexpected renderer coverage scope: {scope!r}")
        store_loss = stats.get("loss")
        if not isinstance(store_loss, dict):
            fail("batch stats have no store loss object")
        for field in ("rejected_disabled", "rejected_oversized", "rejected_accounting", "dropped"):
            if type(store_loss.get(field)) is not int or store_loss[field] != 0:
                fail(f"store loss {field} must be exactly zero")

        # Build causal edges only from collector-resolved edges, whose exact
        # child/parent record references are checked against retained records.
        parents: dict[tuple[int, int], list[tuple[str, tuple[int, int]]]] = defaultdict(list)
        for index, resolution in enumerate(resolutions):
            child = producer_ref(resolution.get("child"), f"resolution[{index}].child")
            parent = producer_ref(resolution.get("parent"), f"resolution[{index}].parent")
            child_record = record_ref(resolution.get("child_record"), f"resolution[{index}].child_record")
            parent_record = record_ref(resolution.get("parent_record"), f"resolution[{index}].parent_record")
            if child_record[:2] != (run, source) or parent_record[:2] != (run, source):
                fail(f"resolution[{index}] references a different run/source")
            if child not in producer_records or parent not in producer_records:
                fail(f"resolution[{index}] producer references are not both retained")
            actual_child = record_ref(
                records_by_id[child_record].get("envelope", {}).get("reference"),
                f"resolution[{index}] retained child reference",
            ) if child_record in records_by_id else None
            actual_parent = record_ref(
                records_by_id[parent_record].get("envelope", {}).get("reference"),
                f"resolution[{index}] retained parent reference",
            ) if parent_record in records_by_id else None
            if actual_child != child_record or actual_parent != parent_record:
                fail(f"resolution[{index}] points to a missing or mismatched retained RecordRef")
            if producer_records.get(child) != child_record or producer_records.get(parent) != parent_record:
                fail(f"resolution[{index}] RecordRefs do not match the exact producer records")
            relation = resolution.get("relation")
            if not isinstance(relation, str):
                fail(f"resolution[{index}] has no typed relation")
            parents[child].append((relation, parent))

        def linked_children(parent: tuple[int, int], relation: str, kind: str) -> list[tuple[int, int]]:
            return [
                child
                for child, links in parents.items()
                if any(link_relation == relation and link_parent == parent for link_relation, link_parent in links)
                and decisions.get(child, {}).get("kind") == kind
            ]

        def path_to_capture(invalidated: tuple[int, int]) -> tuple[tuple[int, int], list[tuple[int, int]]] | None:
            # A capture points to the most recent invalidation or to one or
            # more merged-input records which point back with coalesced_from.
            for capture in decisions_by_kind.get("composition_input_captured", []):
                for relation, parent in parents.get(capture, []):
                    if relation != "captured_from":
                        continue
                    chain = [capture]
                    cursor = parent
                    seen: set[tuple[int, int]] = set()
                    while cursor not in seen:
                        seen.add(cursor)
                        kind = decisions.get(cursor, {}).get("kind")
                        chain.append(cursor)
                        if cursor == invalidated:
                            return capture, chain
                        if kind != "composition_input_merged":
                            break
                        prior = [p for rel, p in parents.get(cursor, []) if rel == "coalesced_from"]
                        if len(prior) != 1:
                            break
                        cursor = prior[0]
            return None

        selected: list[tuple[int, int]] | None = None
        for requested in decisions_by_kind.get("settings_requested", []):
            for committed in linked_children(requested, "committed_by", "settings_committed"):
                for advanced in linked_children(committed, "committed_by", "revision_advanced"):
                    if args.dependency_id is not None and decisions[advanced].get("details", {}).get("dependency_id") != args.dependency_id:
                        continue
                    for invalidated in linked_children(advanced, "triggered_by", "composition_input_invalidated"):
                        captured_path = path_to_capture(invalidated)
                        if captured_path is None:
                            continue
                        captured, capture_nodes = captured_path
                        for evaluated in linked_children(captured, "captured_from", "component_input_evaluated"):
                            for flush in linked_children(evaluated, "evaluated_from", "composition_flush_completed"):
                                for mutation in linked_children(flush, "produced_by", "mutation_batch"):
                                    for styled in linked_children(mutation, "rendered_from", "renderer_style_completed"):
                                        for laid_out in linked_children(styled, "produced_by", "renderer_layout_completed"):
                                            for painted in linked_children(laid_out, "produced_by", "renderer_paint_completed"):
                                                for submitted in linked_children(painted, "scope_contains", "host_frame_submitted"):
                                                    selected = [requested, committed, advanced, invalidated, *reversed(capture_nodes[:-1]), captured, evaluated, flush, mutation, styled, laid_out, painted, submitted]
                                                    break
                                                if selected: break
                                            if selected: break
                                        if selected: break
                                    if selected: break
                                if selected: break
                            if selected: break
                        if selected: break
                    if selected: break
                if selected: break
            if selected: break

        if selected is None:
            fail("no complete resolved causal chain matched the required typed stages")
        chain_rows: list[dict[str, Any]] = []
        required_kinds = [
            "settings_requested", "settings_committed", "revision_advanced",
            "composition_input_invalidated", "composition_input_captured",
            "component_input_evaluated", "composition_flush_completed", "mutation_batch",
            "renderer_style_completed", "renderer_layout_completed", "renderer_paint_completed",
            "host_frame_submitted",
        ]
        # Capture may have intermediate merged nodes. Keep them in output but
        # check each selected node's actual type against its known reference.
        selected = list(dict.fromkeys(selected))
        terminal_by_kind = {decisions[ref].get("kind"): ref for ref in selected}
        for kind in required_kinds:
            ref = terminal_by_kind.get(kind)
            if ref is None:
                fail(f"selected chain lacks required stage {kind}")
            decision = decisions[ref]
            details = decision.get("details") or {}
            rid = next(
                key for key, record in records_by_id.items()
                if event_of(record) is not None
                and producer_ref(event_of(record).get("reference"), "chain reference") == ref
            )
            chain_rows.append({
                "kind": kind,
                "producer_ref": {"producer_id": ref[0], "sequence": ref[1]},
                "record_ref": {"run": rid[0], "source": rid[1], "sequence": rid[2]},
            })
        mutation_ref = terminal_by_kind["mutation_batch"]
        flush_ref = terminal_by_kind["composition_flush_completed"]
        mutation_details = decisions[mutation_ref].get("details") or {}
        flush_details = decisions[flush_ref].get("details") or {}
        start, end = mutation_details.get("mutation_cursor_start"), mutation_details.get("mutation_cursor_end")
        if type(start) is not int or type(end) is not int or end <= start:
            fail("MutationBatch must carry a nonempty cursor range")
        if flush_details.get("mutation_cursor_start") != start or flush_details.get("mutation_cursor_end") != end:
            fail("MutationBatch cursor range does not exactly match its flush")
        submitted_ref = terminal_by_kind["host_frame_submitted"]
        submitted_details = decisions[submitted_ref].get("details") or {}
        for field in ("window_id", "frame_id"):
            value = submitted_details.get(field)
            if type(value) is not int or value <= 0:
                fail(f"HostFrameSubmitted must have nonzero {field}")
        surface_generation = submitted_details.get("surface_generation")
        if type(surface_generation) is not int or surface_generation < 0:
            fail("HostFrameSubmitted must have a numeric surface_generation")

        evaluated_ref = terminal_by_kind["component_input_evaluated"]
        evaluated_subject = decisions[evaluated_ref].get("subject")
        operation_refs = {
            parent for relation, parent in parents.get(evaluated_ref, [])
            if relation == "produced_by"
            and decisions.get(parent, {}).get("kind") == "component_operation_started"
            and decisions[parent].get("subject") == evaluated_subject
        }
        scoped_core = []
        for rid, record in records_by_id.items():
            trace = record.get("payload", {}).get("reactive", {})
            event = trace.get("event", {})
            core = event.get("event", {})
            span = trace.get("span_context") or {}
            if event.get("source") != "meristem" or core.get("record") != "decision":
                continue
            ref = producer_ref(core, "scoped core event")
            operation = span.get("operation")
            operation_ref = producer_ref(operation, "core operation") if operation else None
            if (span.get("subject") == evaluated_subject
                and operation_ref in operation_refs
                and trace.get("scope_cause") == operation
                and ("scope_contains", operation_ref) in parents.get(ref, [])):
                scoped_core.append({"run": rid[0], "source": rid[1], "sequence": rid[2]})
        if not scoped_core:
            fail("no actual Meristem decision resolves to the evaluated component operation")

        receipt["chain"] = chain_rows
        receipt["core_decisions_linked_to_evaluation"] = scoped_core
        receipt["mutation_cursor_range"] = {"start": start, "end": end}
        receipt["host_submission"] = {
            "window_id": submitted_details["window_id"],
            "frame_id": submitted_details["frame_id"],
            "surface_generation": submitted_details["surface_generation"],
            "meaning": "queue submission returned; physical presentation is not asserted",
        }
        receipt["resolution_records_checked"] = len(resolutions)
        receipt["qualified"] = True
    except Exception as exc:  # a failed verifier still writes a useful receipt
        receipt["errors"].append(str(exc))

    rendered = json.dumps(receipt, indent=2, sort_keys=True)
    print(rendered)
    if args.receipt:
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        args.receipt.write_text(rendered + "\n", encoding="utf-8")
    return 0 if receipt["qualified"] else 1


if __name__ == "__main__":
    sys.exit(main())
