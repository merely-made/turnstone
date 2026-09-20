#!/usr/bin/env python3
"""Explicit local resolution and portable-lock verification (Python 3.11+).

Kept identical in Mere and Turnstone so either checkout works independently.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def output(args, **kwargs):
    return subprocess.check_output(args, text=True, encoding="utf-8", **kwargs).strip()


def manifest_arg(args):
    for i, arg in enumerate(args):
        if arg == "--manifest-path":
            return [arg, str(Path(args[i + 1]).resolve())]
        if arg.startswith("--manifest-path="):
            return ["--manifest-path", str(Path(arg.split("=", 1)[1]).resolve())]
    return []


def workspace(cargo, args):
    return Path(output(cargo + ["locate-project", "--workspace", "--message-format", "plain"]
                       + manifest_arg(args))).resolve().parent


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None


def config_paths(root):
    for directory in [root, *root.parents]:
        for name in ["config", "config.toml"]:
            yield directory / ".cargo" / name
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    for name in ["config", "config.toml"]:
        yield cargo_home / name


def portable_config(root):
    for key in os.environ:
        if key.startswith(("CARGO_RESOLVER_LOCKFILE_PATH", "CARGO_SOURCE_", "CARGO_PATCH_")):
            raise ValueError(f"Portable mode refuses environment override {key}")
    for path in set(config_paths(root)):
        if not path.exists():
            continue
        data = tomllib.loads(path.read_text(encoding="utf-8-sig"))
        if any(key in data for key in ["patch", "paths", "source", "include"]) or "lockfile-path" in data.get("resolver", {}):
            raise ValueError(f"Portable mode needs a redirect-free config: {path}")


def setup(repo):
    # Only tracked workspace manifests; never crawl target/cache directories.
    manifests = output(["git", "-C", str(repo), "ls-files", "*Cargo.toml"]).splitlines()
    roots = {repo}
    for name in manifests:
        path = repo / name
        if path.name == "Cargo.toml" and "workspace" in tomllib.loads(path.read_text(encoding="utf-8-sig")):
            roots.add(path.parent)
    moves = []
    for root in sorted(roots):
        candidates = [root / ".cargo" / name for name in ["config", "config.toml"]]
        present = [p for p in candidates if p.exists()]
        if len(present) > 1:
            raise ValueError(f"Resolve competing config/config.toml before setup: {root}")
        for path in present:
            if subprocess.run(["git", "-C", str(repo), "ls-files", "--error-unmatch", str(path)],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
                continue  # Portable tracked configuration stays in place.
            dest = path.with_name("config.local.toml")
            if dest.exists():
                raise ValueError(f"Will not overwrite {dest}; reconcile it with {path}")
            moves.append((path, dest))
    for root in sorted(roots):
        old, local = root / "Cargo.lock", root / ".cargo/local/Cargo.lock"
        if old.exists() and not local.exists():
            local.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(old, local)
            print(f"Preserved {local}")
    for old, new in moves:
        old.rename(new)
        print(f"Local config: {old} -> {new}")


def main():
    args = sys.argv[1:]
    if not args or args[0] not in {"setup", "local", "verify"}:
        raise ValueError("Usage: cargo_mode.py setup | local [ +toolchain ] <cargo args> | verify [ +toolchain ] [--manifest-path PATH] [--metadata-only]")
    mode = args.pop(0)
    cargo = ["cargo"]
    if args and args[0].startswith("+"):
        cargo.append(args.pop(0))
    repo = Path(__file__).resolve().parents[1]
    root = repo if mode == "setup" else workspace(cargo, args)
    version = output(cargo + ["--version"], cwd=root)
    match = re.search(r"cargo (\d+)\.(\d+)", version)
    if not match or tuple(map(int, match.groups())) < (1, 97):
        raise ValueError(f"Cargo 1.97+ required before resolution: {version}")
    if mode == "setup":
        if args:
            raise ValueError("setup takes no Cargo arguments")
        setup(repo)
        return
    if not root.is_relative_to(repo):
        raise ValueError(f"Workspace is outside this checkout: {root}")
    lock = root / "Cargo.lock"
    before = digest(lock)
    if mode == "local":
        if not args or "--config" in args or any(a.startswith("--config=") for a in args):
            raise ValueError("Supply a Cargo command; put local configuration in config.local.toml")
        configs = []
        for directory in reversed([root, *root.parents]):
            if directory.is_relative_to(repo):
                path = directory / ".cargo/config.local.toml"
                if path.exists():
                    configs += ["--config", str(path)]
        if not configs:
            raise ValueError("No local config. Run setup for existing overrides or copy the example to .cargo/config.local.toml")
        local = root / ".cargo/local/Cargo.lock"
        if not local.exists() and lock.exists():
            local.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(lock, local)
        override = "resolver.lockfile-path=" + json.dumps(local.as_posix())
        selected_manifest = manifest_arg(args)
        normalized = []
        i = 0
        while i < len(args):
            if args[i] == "--manifest-path":
                normalized += selected_manifest
                i += 2
            elif args[i].startswith("--manifest-path="):
                normalized += selected_manifest
                i += 1
            else:
                normalized.append(args[i])
                i += 1
        print(f"Local workspace: {root}\nLocal lock: {local}", file=sys.stderr)
        try:
            run(cargo + configs + ["--config", override] + normalized, cwd=root)
        finally:
            if digest(lock) != before:
                raise ValueError(f"Portable lock changed during local command: {lock}")
        return
    parser = argparse.ArgumentParser(prog="cargo_mode.py verify")
    parser.add_argument("--manifest-path")
    parser.add_argument("--metadata-only", action="store_true")
    options = parser.parse_args(args)
    metadata_only = options.metadata_only
    allowed = manifest_arg(args)
    portable_config(root)
    if before is None:
        raise ValueError(f"Missing portable lock: {lock}")
    run(["git", "-C", str(repo), "ls-files", "--error-unmatch", str(lock)], stdout=subprocess.DEVNULL)
    try:
        metadata = json.loads(output(cargo + ["metadata", "--locked", "--format-version", "1"] + allowed, cwd=root))
        for package in metadata["packages"]:
            if package["source"] is None:
                path = Path(package["manifest_path"]).resolve()
                if not path.is_relative_to(repo):
                    raise ValueError(f"Outside path package: {package['name']} at {path}")
                run(["git", "-C", str(repo), "ls-files", "--error-unmatch", str(path)], stdout=subprocess.DEVNULL)
        if not metadata_only:
            run(cargo + ["check", "--workspace", "--locked"] + allowed, cwd=root)
    finally:
        if digest(lock) != before:
            raise ValueError(f"Portable lock changed: {lock}")
    print(f"Verified {root}: {version}; lock sha256={before}; packages={len(metadata['packages'])}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, subprocess.CalledProcessError) as error:
        print(f"cargo_mode: {error}", file=sys.stderr)
        sys.exit(1)
