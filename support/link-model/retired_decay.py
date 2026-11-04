#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Check retired-decay qualification with the existing receipt validators."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib

import ordinary_paths as CHECK
REPOSITORY = Path(__file__).resolve().parents[2]


def git_blob(revision, path):
    result = subprocess.run(["git", "-C", str(REPOSITORY), "show", f"{revision}:{path}"],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        CHECK.fail(f"Git could not read {revision}:{path}")
    return result.stdout


def main():
    global REPOSITORY
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("qualification_root", type=Path)
    parser.add_argument("--repo", type=Path, default=REPOSITORY)
    args = parser.parse_args()
    REPOSITORY = args.repo.resolve()
    root = args.qualification_root.resolve()
    manifest = CHECK.read_json(root / "qualification.json")
    revision = manifest["source_revision"]
    if not isinstance(revision, str) or not CHECK.HEX40.fullmatch(revision):
        CHECK.fail("qualification source must be a full commit SHA")
    expected_tests = {
        "kernel-intake.json": {"mere-kernel"},
        "pandect-intake.json": {"pandect"},
        "consumers-intake.json": {"mere", "mere-linked-data", "mere-cartography", "mere-view", "pictograph"},
        "graphshell-intake.json": {"graphshell"},
    }
    if len(manifest["tests"]) != len(expected_tests) or {
            row["record"]: set(row["packages"]) for row in manifest["tests"]} != expected_tests:
        CHECK.fail("required test packages or process records are missing")
    expected_checks = {
        "djinn-intake.json": ("cargo", "check", "djinn", "--all-targets"),
        "wasm-intake.json": ("cargo", "check", "wasm32-unknown-unknown", "product,presentation-webgpu"),
        "boundaries-final.json": ("check_port_boundaries.py",),
    }
    if len(manifest["checks"]) != len(expected_checks) or {
            row["record"] for row in manifest["checks"]} != set(expected_checks):
        CHECK.fail("required compile or boundary checks are missing")
    for row in manifest["tests"]:
        record = root / row["record"]
        for package in row["packages"]:
            total = CHECK.verify_cargo_qualification(root, record, revision, package)
            if total != row["passed"]:
                CHECK.fail(f"unexpected suite total in {record.name}")
        suites = CHECK.read_json(record)["suites"]
        if sum(suite["ignored"] for suite in suites) != row["ignored"]:
            CHECK.fail(f"unexpected ignored count in {record.name}")
    for row in manifest["checks"]:
        CHECK.verify_command_record(root, root / row["record"], revision, True,
                                    expected_checks[row["record"]])
    legacy = CHECK.read_json(root / "legacy-fixtures.json")
    CHECK.verify_command_record(root, root / "legacy-fixture-writer.json",
                                legacy["source_revision"], True, ("cargo", "run"))
    writer = root / legacy["writer"]
    if CHECK.sha256_file(writer) != legacy["writer_sha256"]:
        CHECK.fail("baseline fixture writer hash mismatch")
    for filename, key in (("legacy-writer.Cargo.toml.txt", "writer_manifest_sha256"),
                          ("legacy-writer.Cargo.lock.txt", "writer_lock_sha256")):
        if CHECK.sha256_file(root / filename) != legacy[key]:
            CHECK.fail("baseline fixture writer dependency identity mismatch")
    if {row["path"] for row in legacy["fixtures"]} != {
            "captured-with-decay.json", "captured-with-decay.postcard", "captured-with-decay.rkyv"}:
        CHECK.fail("all three baseline codec fixtures are required")
    for row in legacy["fixtures"]:
        path = "crates/graph/graph-kernel/src/graph/capture/fixtures/" + row["path"]
        data = git_blob(revision, path)
        if len(data) != row["bytes"] or hashlib.sha256(data).hexdigest() != row["sha256"]:
            CHECK.fail("committed historical fixture differs from recorded baseline output")
    for row in json.loads((root / "preserved-readers.json").read_text()):
        before = git_blob(legacy["source_revision"], row["path"])
        after = git_blob(revision, row["path"])
        if before != after or hashlib.sha256(after).hexdigest() != row["sha256"]:
            CHECK.fail("snapshot readers/schema, manifest or web lock changed")
    caller = CHECK.read_json(root / "caller-migration.json")
    total = CHECK.verify_cargo_qualification(
        root, root / caller["record"], caller["source_revision"], caller["package"])
    if total != 5 or total != caller["passed"]:
        CHECK.fail("all five owning-repo caller regressions are required")
    command = CHECK.read_json(root / caller["record"])
    argv = command["command"]
    if ("--locked" not in argv or "--manifest-path" not in argv or
            argv[argv.index("--manifest-path") + 1] != caller["harness_manifest"] or
            command["cwd"] != caller["harness_cwd"] or
            Path(caller["harness_manifest"]) != Path(caller["harness_cwd"]) / "Cargo.toml"):
        CHECK.fail("caller test must identify the locked, recorded harness")
    for name, digest in caller["files"].items():
        if CHECK.sha256_file(root / name) != digest:
            CHECK.fail("caller harness manifest or lock hash mismatch")
    caller_lock = tomllib.loads((root / "caller-regressions.Cargo.lock.txt").read_text())
    sdk_sources = {row["source"] for row in caller_lock["package"]
                   if "github.com/merely-made/mere.git" in row.get("source", "")}
    if sdk_sources != {"git+https://github.com/merely-made/mere.git?branch=main#" + caller["sdk_revision"]}:
        CHECK.fail("caller regression SDK identity changed")
    CHECK.verify_command_record(root, root / caller["publication_record"],
                                caller["source_revision"], True,
                                ("git", "ls-remote", caller["source_repository"], "refs/heads/main"))
    published = (root / caller["publication_record"]).with_suffix(".log").read_text().splitlines()[1:]
    if published != [caller["source_revision"] + "\trefs/heads/main"]:
        CHECK.fail("recorded caller repository main does not contain the migration")
    intake = CHECK.read_json(root / "dependency-intake.json")
    if intake["qualified_revision"] != revision or intake["caller_revision"] != caller["source_revision"]:
        CHECK.fail("dependency intake source identities disagree")
    locks = []
    for key, hash_key in (("baseline_revision", "baseline_lock_sha256"),
                          ("qualified_revision", "qualified_lock_sha256")):
        data = git_blob(intake[key], "Cargo.lock")
        if hashlib.sha256(data).hexdigest() != intake[hash_key]:
            CHECK.fail("dependency intake lock hash mismatch")
        packages = tomllib.loads(data.decode())["package"]
        # Separate Git revisions can share name/version (for example Turquet).
        # Only the one intended Cleromancy source replacement is paired across revisions.
        def package_key(row):
            source = "" if row["name"] == "cleromancy-core-acceptance" else row.get("source", "")
            return (row["name"], row["version"], source)
        lock = {package_key(row): row for row in packages}
        if len(lock) != len(packages) or len(packages) != intake["package_count"]:
            CHECK.fail("dependency inventory collapsed duplicate source identities")
        locks.append(lock)
    before, after = locks
    if set(before) != set(after) or len(after) != intake["package_count"]:
        CHECK.fail("dependency package inventory changed")
    identity = lambda row: (row["name"], row["version"], row.get("source"), row.get("checksum"))
    changed = [key for key in before if identity(before[key]) != identity(after[key])]
    changed_names = [key[0] + "@" + key[1] for key in changed]
    if changed_names != ["cleromancy-core-acceptance@0.1.0"] or changed_names != intake["package_identity_changes"]:
        CHECK.fail("unrelated package identity changed during caller intake")
    if not after[changed[0]]["source"].endswith("#" + caller["source_revision"]):
        CHECK.fail("qualified lock does not pin the owning-repo caller migration")
    actual_changes = [{"package": key[0] + "@" + key[1], "before": before[key], "after": after[key]}
                      for key in before if before[key] != after[key]]
    if actual_changes != intake["changes"]:
        CHECK.fail("dependency edge changes differ from the recorded Cargo output")
    comparison = CHECK.read_json(root / "doc-audit-comparison.json")
    if len(comparison["audits"]) != 2 or {row["audit"] for row in comparison["audits"]} != {"doc-audit", "judgment"}:
        CHECK.fail("both documentation audit comparisons are required")
    for row in comparison["audits"]:
        outputs = []
        for record_key, source, digest_key in (
                ("baseline_record", legacy["source_revision"], "baseline_log_sha256"),
                ("final_record", revision, "final_log_sha256")):
            path = root / row[record_key]
            record = CHECK.read_json(path)
            CHECK.verify_log(root, record, path)
            if (record["revision"] != source or record["exit_code"] != 1 or
                    record["log_sha256"] != row[digest_key]):
                CHECK.fail("documentation audit source, result or log differs")
            outputs.append(json.loads(path.with_suffix(".log").read_text().split("\n", 1)[1]))
        if outputs[0] != outputs[1] or not row["json_equal"] or row["changed_fields"]:
            CHECK.fail("documentation audit findings changed")
        if row["audit"] == "doc-audit" and outputs[1]["counts"] != row["finding_counts"]:
            CHECK.fail("documentation finding counts differ")
        if row["audit"] == "judgment" and len(outputs[1]["errors"]) != row["errors"]:
            CHECK.fail("judgment error count differs")
    print(f"Retired-decay qualification verified at {revision}")
    print("All recorded tests/checks exited successfully from clean source; fixture and preserved-reader hashes match.")


if __name__ == "__main__":
    try:
        main()
    except CHECK.VerificationError as error:
        print(f"verification failed: {error}", file=sys.stderr)
        raise SystemExit(1)
