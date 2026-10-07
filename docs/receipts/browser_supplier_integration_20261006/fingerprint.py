"""Freeze or verify one labeled exact executable and all browser qualification inputs."""
import argparse
import datetime
import hashlib
import json
import os
import pathlib
import re
import subprocess
import tomllib

receipt = pathlib.Path(__file__).resolve().parent
root = receipt.parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--label", required=True)
parser.add_argument("--features", required=True, help="Exact comma-separated Cargo feature selection")
parser.add_argument("--build-command", help="Actual completed build command; required when freezing")
parser.add_argument("--exe", type=pathlib.Path, default=pathlib.Path("C:/t/cargo-targets/turnstone/debug/turnstone.exe"))
parser.add_argument("--verify", action="store_true")
args = parser.parse_args()
if not re.fullmatch(r"[a-z][a-z0-9-]{0,63}", args.label):
    parser.error("--label must be a lowercase filename component")
features = args.features.split(",")
if not features or any(not re.fullmatch(r"[a-zA-Z0-9_-]+", value) for value in features):
    parser.error("--features must name a nonempty explicit comma-separated feature selection")
manifest = receipt / f"source-manifest-{args.label}.json"
exe = args.exe.resolve(strict=True)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def command(argv):
    return subprocess.check_output(argv, cwd=root, encoding="utf-8").strip()


def input_paths():
    # Include new Rust/Cargo inputs as well as tracked ones. Gitignore keeps
    # generated caches out; receipt outputs are excluded explicitly.
    rust_cargo = command([
        "rg", "--files", "--hidden", "-g", "!.git/**", "-g", "!docs/receipts/**",
        "-g", "*.rs", "-g", "Cargo.toml", "-g", "Cargo.lock",
        "-g", "rust-toolchain*", "-g", ".cargo/config*",
    ]).splitlines()
    paths = {root / name for name in rust_cargo}
    # An ignored local Cargo config can affect the actual command too.
    paths.update(path for path in (root / ".cargo/config", root / ".cargo/config.toml") if path.is_file())
    for name in (
        ".gitattributes",
        "docs/receipts/g3_turnstone_endpoint.html",
        "docs/receipts/browser_family_20261007/run-gate.py",
        "docs/receipts/browser_family_20261007/freeze-current.py",
        "docs/receipts/browser_family_20261007/review-intl-pixels.py",
        "docs/receipts/browser_family_20261007/bind-native-build.py",
        "scenarios/browser_scry_windows.scn",
        "scenarios/browser_weld_direct_windows.scn",
        "scenarios/browser_weld_permission_direct_windows.scn",
        "scenarios/browser_servo_windows.scn",
        "scenarios/browser_servo_intl_windows.scn",
        "scenarios/browser_servo_viewport_windows.scn",
        "scenarios/browser_trio_windows.scn",
        "scenarios/fixtures/browser_decisions_server.ps1",
    ):
        paths.add(root / name)
    for name in ("scenarios/fixtures/browser_scry", "scenarios/fixtures/browser_servo",
                 "vendor/mozjs-icu-collator", "vendor/mozjs-icu-normalizer"):
        paths.update(path for path in (root / name).rglob("*") if path.is_file())
    # The runner itself determines environment, profile reuse and pass/fail gates.
    paths.update(receipt / name for name in (
        "fingerprint.py", "native-runner.ps1", "run-current.ps1", "run-permission.ps1", "run-servo.ps1", "run-mixed.ps1", "run-upstream.ps1", "review-servo-pixels.py",
    ))
    return sorted(paths)


if args.verify:
    record = json.loads(manifest.read_text(encoding="utf-8"))
    changes = []
    if record["features"] != features:
        changes.append("feature selection")
    if record["exe_path"] != str(exe):
        changes.append("executable path")
    current_paths = {path.relative_to(root).as_posix() for path in input_paths()}
    frozen_paths = {item["path"] for item in record["files"]}
    changes.extend(sorted(current_paths.symmetric_difference(frozen_paths)))
    for item in record["files"]:
        path = root / item["path"]
        if not path.is_file() or digest(path) != item["sha256"]:
            changes.append(item["path"])
    if digest(exe) != record["exe_sha256"]:
        changes.append(str(exe))
    for tool, argv in (("cargo", ["cargo", "-V"]), ("rustc", ["rustc", "-vV"])):
        if command(argv) != record["toolchain"][tool]:
            changes.append(tool + " toolchain")
    if changes:
        raise SystemExit("Changed qualification inputs: " + ", ".join(sorted(set(changes))))
    print(f"{args.label}: source, scenarios, fixtures, runners, toolchain and executable fingerprints unchanged")
else:
    if not args.build_command:
        parser.error("--build-command must record the actual completed build invocation")
    if manifest.exists():
        raise SystemExit("Refusing to overwrite frozen qualification: " + str(manifest))
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    record = {
        "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "label": args.label,
        "head": command(["git", "rev-parse", "HEAD"]),
        "dirty_status": command(["git", "status", "--short"]),
        "features": features,
        "actual_build_command": args.build_command,
        "freeze_environment": {name: os.environ.get(name) for name in (
            "CEF_PATH", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC", "CARGO_TARGET_DIR",
            "CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_DEBUG",
            "LIBCLANG_PATH", "CMAKE_GENERATOR", "CC", "CXX",
        )},
        "working_directory": str(root),
        "toolchain": {"cargo": command(["cargo", "-V"]), "rustc": command(["rustc", "-vV"])},
        "dependencies": sorted([
            {"name": item["name"], "version": item["version"], "source": item.get("source"), "checksum": item.get("checksum")}
            for item in lock["package"]
        ], key=lambda item: (item["name"], item["version"], item["source"] or "")),
        "files": [{"path": path.relative_to(root).as_posix(), "sha256": digest(path)} for path in input_paths()],
        "exe_path": str(exe),
        "exe_sha256": digest(exe),
    }
    # Exclusive creation prevents a second freeze from replacing the evidence.
    with manifest.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(record, stream, indent=2)
        stream.write("\n")
    print(f"{args.label}: {record['exe_sha256']}")
