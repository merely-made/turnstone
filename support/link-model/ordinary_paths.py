#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Fail-closed verifier for ordinary responsive-link qualification receipts.

Expected layout beneath QUALIFICATION_ROOT:
  build.json (source revision, package artifact hashes, and explicit process_records)
  process JSON/log files named by build.json.process_records
  browser-qualified/p4_tree_link_paths/{result.json, ordinary_link_paths_*.png}
  browser-qualified/p4_tree_link_paths_reopen/{result.json, ordinary_link_paths_*.png}

The JSON records are evidence inputs, not proof by their existence: process exits,
Cargo test suite summaries, log hashes, artifact hashes, browser scenario gates,
and actual PNG bytes are checked here.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Any

SEED_SCENARIO = "p4_tree_link_paths"
REOPEN_SCENARIO = "p4_tree_link_paths_reopen"
SEED_CAPTURES = {
    "ordinary_link_paths_baseline",
    "ordinary_link_paths_field_only",
    "ordinary_link_paths_attractor_field_only",
    "ordinary_link_paths_vortex_field_only",
    "ordinary_link_paths_compatible_only",
    "ordinary_link_paths_field_and_compatible",
    "ordinary_link_paths_both_disabled",
    "ordinary_link_paths_hidden",
}
REOPEN_CAPTURES = {
    "ordinary_link_paths_reopened_hidden",
    "ordinary_link_paths_reopened_shown",
}
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")


class VerificationError(Exception):
    pass


def fail(message: str) -> None:
    raise VerificationError(message)


def read_json(path: Path) -> dict[str, Any]:
    if not path.is_file() or path.stat().st_size == 0:
        fail(f"missing or empty JSON evidence: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail(f"invalid JSON evidence {path}: {exc}")
    if not isinstance(value, dict):
        fail(f"expected a JSON object: {path}")
    return value


def require_string(obj: dict[str, Any], key: str, where: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value.strip():
        fail(f"{where}.{key} must be a nonempty string")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def absolute_evidence_path(value: str) -> bool:
    """Recorded command paths belong to their source host, not this verifier OS."""
    return PurePosixPath(value).is_absolute() or PureWindowsPath(value).is_absolute()


def rooted_evidence_path(value: str) -> bool:
    return bool(PurePosixPath(value).root or PureWindowsPath(value).drive
                or PureWindowsPath(value).root)


def resolve_relative(root: Path, value: str, where: str, suffix: str) -> Path:
    relative = Path(value)
    if rooted_evidence_path(value) or ".." in relative.parts or relative.suffix.lower() != suffix:
        fail(f"{where} must be a safe relative {suffix} path")
    resolved = (root / relative).resolve()
    try:
        resolved.relative_to(root.resolve())
    except ValueError:
        fail(f"{where} resolves outside the qualification root")
    return resolved


def verify_log(root: Path, record: dict[str, Any], record_path: Path) -> None:
    stem = record_path.stem
    claimed = require_string(record, "log_sha256", str(record_path.relative_to(root)))
    if not HEX64.fullmatch(claimed):
        fail(f"{stem}.log_sha256 is not a lowercase SHA-256 digest")
    log = record_path.with_suffix(".log")
    if not log.is_file() or log.stat().st_size == 0:
        fail(f"missing or empty process log: {log}")
    actual = sha256_file(log)
    if actual != claimed:
        fail(f"{stem} log hash mismatch: recorded {claimed}, actual {actual}")


def verify_command_record(root: Path, record_path: Path, source_revision: str,
                          require_clean: bool, required_tokens: tuple[str, ...]) -> None:
    path = record_path
    record = read_json(path)
    where = path.name
    command = record.get("command")
    if not isinstance(command, list) or not command or not all(
        isinstance(arg, str) and arg for arg in command
    ):
        fail(f"{where}.command must be a nonempty argv array")
    lower_command = [arg.lower() for arg in command]
    for token in required_tokens:
        if not any(token in arg for arg in lower_command):
            fail(f"{where}.command does not identify required operation {token!r}")
    if record.get("exit_code") != 0:
        fail(f"{where} process exit was not zero: {record.get('exit_code')!r}")
    if record.get("revision") != source_revision:
        fail(f"{where} source revision differs from build.json")
    cwd = require_string(record, "cwd", where)
    if not absolute_evidence_path(cwd):
        fail(f"{where}.cwd must be absolute")
    require_string(record, "started", where)
    require_string(record, "finished", where)
    if require_clean:
        if "status_before" not in record:
            fail(f"{where}.status_before is required to establish the source worktree state")
        status = record["status_before"]
        if status != "":
            fail(f"{where} did not start from a clean source worktree: {status!r}")
    verify_log(root, record, path)


def verify_cargo_qualification(root: Path, record_path: Path, source_revision: str,
                               package: str) -> int:
    path = record_path
    record = read_json(path)
    where = path.name
    command = record.get("command")
    if not isinstance(command, list) or not command or not all(
        isinstance(arg, str) and arg for arg in command
    ):
        fail(f"{where}.command must be a nonempty argv array")
    lower_command = [arg.lower() for arg in command]
    if not any(arg == "cargo" or arg.endswith("/cargo") for arg in lower_command):
        fail(f"{where}.command does not identify Cargo")
    if "test" not in lower_command:
        fail(f"{where}.command is not a Cargo test invocation")
    if "-p" not in lower_command or package not in lower_command:
        fail(f"{where}.command does not test package {package!r}")
    if record.get("exit_code") != 0:
        fail(f"{where} command exit was not zero: {record.get('exit_code')!r}")
    if record.get("revision") != source_revision:
        fail(f"{where} source revision differs from build.json")
    require_string(record, "cwd", where)
    require_string(record, "started", where)
    require_string(record, "finished", where)
    suites = record.get("suites")
    if not isinstance(suites, list) or not suites:
        fail(f"{where}.suites must be a nonempty list")
    total_passed = 0
    for index, suite in enumerate(suites):
        label = f"{where}.suites[{index}]"
        if not isinstance(suite, dict):
            fail(f"{label} must be an object")
        if suite.get("result") != "ok":
            fail(f"{label} did not report result=ok")
        passed = suite.get("passed")
        failed = suite.get("failed")
        ignored = suite.get("ignored")
        measured = suite.get("measured")
        filtered = suite.get("filtered_out")
        if not all(isinstance(n, int) and not isinstance(n, bool) and n >= 0
                   for n in (passed, failed, ignored, measured, filtered)):
            fail(f"{label} has missing or invalid test counts")
        if failed != 0:
            fail(f"{label} reports {failed} failed tests")
        total_passed += passed
    if total_passed <= 0:
        fail(f"{where} contains no passing tests; refusing a dummy/empty qualification")
    if record.get("status_before") != "":
        fail(f"{where} must begin with a recorded clean source worktree")
    verify_log(root, record, path)
    pattern = r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out"
    parsed = [dict(zip(
        ("result", "passed", "failed", "ignored", "measured", "filtered_out"),
        (match[0], *map(int, match[1:])),
    )) for match in re.findall(pattern, path.with_suffix(".log").read_text(encoding="utf-8"))]
    if parsed != suites:
        fail(f"{where} suite summaries differ from the actual process log")
    return total_passed


def verify_build_manifest(root: Path) -> tuple[str, dict[str, Path]]:
    path = root / "build.json"
    manifest = read_json(path)
    if manifest.get("schema") != "ordinary-responsive-links-build/v1":
        fail("build.json schema must be ordinary-responsive-links-build/v1")
    revision = require_string(manifest, "source_revision", "build.json")
    if not HEX40.fullmatch(revision):
        fail("build.json.source_revision must be a 40-character lowercase Git SHA")
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list) or len(artifacts) < 2:
        fail("build.json.artifacts must list at least the generated JS and WASM package files")
    seen: set[str] = set()
    suffixes: set[str] = set()
    for index, artifact in enumerate(artifacts):
        label = f"build.json.artifacts[{index}]"
        if not isinstance(artifact, dict):
            fail(f"{label} must be an object")
        rel = require_string(artifact, "path", label)
        expected = require_string(artifact, "sha256", label)
        if not HEX64.fullmatch(expected):
            fail(f"{label}.sha256 is not a lowercase SHA-256 digest")
        rel_path = Path(rel)
        if rooted_evidence_path(rel) or ".." in rel_path.parts:
            fail(f"{label}.path must stay beneath the qualification root")
        resolved = (root / rel_path).resolve()
        try:
            resolved.relative_to(root.resolve())
        except ValueError:
            fail(f"{label}.path resolves outside the qualification root")
        if rel in seen:
            fail(f"duplicate artifact path in build.json: {rel}")
        seen.add(rel)
        suffixes.add(resolved.suffix.lower())
        if not resolved.is_file() or resolved.stat().st_size == 0:
            fail(f"missing or empty build artifact: {rel}")
        actual = sha256_file(resolved)
        if actual != expected:
            fail(f"artifact hash mismatch for {rel}: recorded {expected}, actual {actual}")
    if not {".js", ".wasm"}.issubset(suffixes):
        fail("build.json must hash at least one .js and one .wasm package artifact")
    process_records = manifest.get("process_records")
    required_processes = {"canvas", "graphshell", "build", "bindgen"}
    if not isinstance(process_records, dict) or not required_processes.issubset(process_records):
        fail("build.json.process_records must map canvas, graphshell, build, and bindgen to JSON records")
    resolved_records: dict[str, Path] = {}
    for name in sorted(required_processes):
        record_name = process_records.get(name)
        if not isinstance(record_name, str) or not record_name.strip():
            fail(f"build.json.process_records.{name} must be a nonempty relative JSON path")
        resolved_records[name] = resolve_relative(
            root, record_name, f"build.json.process_records.{name}", ".json"
        )
    if len(set(resolved_records.values())) != len(required_processes):
        fail("build.json.process_records must name four distinct process records")
    return revision, resolved_records


def png_dimensions(path: Path) -> tuple[int, int]:
    with path.open("rb") as stream:
        header = stream.read(24)
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n" or header[12:16] != b"IHDR":
        fail(f"capture is not a valid PNG with IHDR: {path}")
    width, height = struct.unpack(">II", header[16:24])
    if width <= 0 or height <= 0:
        fail(f"capture has invalid dimensions: {path}")
    return width, height


def verify_browser_scenario(root: Path, scenario_name: str,
                            expected_captures: set[str]) -> dict[str, Any]:
    folder = root / "browser-qualified" / scenario_name
    result_path = folder / "result.json"
    document = read_json(result_path)
    scenario = document.get("scenario")
    if not isinstance(scenario, dict):
        fail(f"{result_path} lacks the nested scenario object")
    if scenario.get("state") != "ok":
        fail(f"{scenario_name} scenario state is not ok")
    errors = scenario.get("errors")
    gates = scenario.get("gate_failures")
    if errors != []:
        fail(f"{scenario_name} scenario errors must be an empty list, got {errors!r}")
    if gates != []:
        fail(f"{scenario_name} scenario gate_failures must be an empty list, got {gates!r}")
    result = scenario.get("result")
    if not isinstance(result, dict) or result.get("result") != "ok":
        fail(f"{scenario_name} scenario result must report result=ok")
    log = result.get("log")
    if not isinstance(log, list) or not log or not all(isinstance(item, str) for item in log):
        fail(f"{scenario_name} scenario result log must be a nonempty string list")
    captures = scenario.get("captures")
    if not isinstance(captures, list) or not captures:
        fail(f"{scenario_name} captures must be a nonempty list")
    by_name: dict[str, dict[str, Any]] = {}
    for index, capture in enumerate(captures):
        label = f"{scenario_name}.captures[{index}]"
        if not isinstance(capture, dict):
            fail(f"{label} must be an object")
        name = require_string(capture, "name", label)
        if name in by_name:
            fail(f"{scenario_name} lists duplicate capture {name!r}")
        by_name[name] = capture
    actual_names = set(by_name)
    if actual_names != expected_captures:
        missing = sorted(expected_captures - actual_names)
        unexpected = sorted(actual_names - expected_captures)
        fail(f"{scenario_name} capture names mismatch; missing={missing}, unexpected={unexpected}")

    for name in sorted(expected_captures):
        capture = by_name[name]
        if f"captured {name}" not in log:
            fail(f"{scenario_name} result log does not confirm capture {name!r}")
        path = folder / f"{name}.png"
        if not path.is_file() or path.stat().st_size == 0:
            fail(f"missing or empty expected PNG capture: {path}")
        # loader.js records img.src.length: the encoded PNG data URL length.
        data_url_length = len("data:image/png;base64,") + 4 * ((path.stat().st_size + 2) // 3)
        if capture.get("bytes") != data_url_length:
            fail(f"{scenario_name}/{name}.png encoded data URL length disagrees with result.json")
        dimensions = png_dimensions(path)
        if capture.get("width") != dimensions[0] or capture.get("height") != dimensions[1]:
            fail(f"{scenario_name}/{name}.png dimensions disagree with result.json")
    return document


def verify_source_comparison(root: Path,
                             seed_result: dict[str, Any],
                             reopen_result: dict[str, Any]) -> None:
    """Compare source-owned observations captured by each scenario completion receipt."""
    def observations(document: dict[str, Any], label: str) -> dict[str, Any]:
        scenario = document.get("scenario")
        if not isinstance(scenario, dict):
            fail(f"{label} browser result lacks scenario")
        result = scenario.get("result")
        if not isinstance(result, dict):
            fail(f"{label} browser result lacks scenario.result")
        saved_graph = result.get("saved_graph")
        if not isinstance(saved_graph, dict):
            fail(f"{label} scenario.result lacks saved_graph")
        source = saved_graph.get("link_path_source")
        if not isinstance(source, dict):
            fail(f"{label} saved_graph lacks link_path_source observations")
        return source

    seed_snaps = observations(seed_result, "seed")
    reopen_snaps = observations(reopen_result, "reopen")
    fields = ("link-rest-source-facts", "link-rest-graph-revision")
    for field in fields:
        a = seed_snaps.get(field)
        b = reopen_snaps.get(field)
        if not isinstance(a, str) or not a.strip() or not isinstance(b, str) or not b.strip():
            fail(f"source comparison lacks nonempty {field} values for both runs")
        if a.lower() in {"missing", "none", "null", "0"} or b.lower() in {"missing", "none", "null", "0"}:
            fail(f"source comparison {field} contains a placeholder value")
        if a != b:
            fail(f"seed/reopen source mismatch for {field}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("qualification_root", type=Path)
    args = parser.parse_args()
    root = args.qualification_root.resolve()
    if not root.is_dir():
        fail(f"qualification root is not a directory: {root}")

    source_revision, records = verify_build_manifest(root)
    verify_command_record(root, records["build"], source_revision,
                          require_clean=True, required_tokens=("cargo", "build"))
    verify_command_record(root, records["bindgen"], source_revision,
                          require_clean=True, required_tokens=("bindgen",))
    canvas_passed = verify_cargo_qualification(root, records["canvas"],
                                               source_revision, "pictograph")
    graphshell_passed = verify_cargo_qualification(root, records["graphshell"],
                                                   source_revision, "graphshell")
    seed = verify_browser_scenario(root, SEED_SCENARIO, SEED_CAPTURES)
    reopened = verify_browser_scenario(root, REOPEN_SCENARIO, REOPEN_CAPTURES)
    verify_source_comparison(root, seed, reopened)

    print("ordinary responsive-link evidence verified")
    print(f"source revision: {source_revision}")
    print(f"canvas passing tests: {canvas_passed}")
    print(f"Graphshell passing tests: {graphshell_passed}")
    print(f"seed PNG captures: {len(SEED_CAPTURES)}; reopen PNG captures: {len(REOPEN_CAPTURES)}")
    print("browser scenario errors/gate failures: zero")
    print("seed/reopen source facts and graph revision: equal")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except VerificationError as exc:
        print(f"verification failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
