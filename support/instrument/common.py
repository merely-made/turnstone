# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Shared path and receipt guards for caller-owned Instrument runs."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def parser(description):
    result = argparse.ArgumentParser(description=description)
    result.add_argument('--repo', type=Path, required=True)
    result.add_argument('--output', type=Path, required=True,
                        help='New directory outside the source checkout and input packets')
    result.add_argument('--max-bytes', type=int, default=100_000_000)
    return result


def fresh_output(path, repo, *inputs):
    path = path.resolve()
    for root in (repo, *inputs):
        root = root.resolve()
        if path == root or path.is_relative_to(root) or root.is_relative_to(path):
            raise ValueError('Output must be separate from source and input trees')
    path.mkdir(parents=True, exist_ok=False)
    return path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8'))


def write_json(path, value):
    with path.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def verify_sources(repo, manifest):
    sources = manifest['source_sha256']
    if not isinstance(sources, dict) or not sources:
        raise ValueError('A nonempty source hash manifest is required')
    for name, digest in sources.items():
        path = (repo / name).resolve()
        if not path.is_relative_to(repo.resolve()) or sha(path) != digest:
            raise ValueError('Source drift or escaped input: ' + name)


def verify_package(package, manifest):
    files = manifest['package_sha256']
    if not isinstance(files, dict) or not {'.js', '.wasm', '.html'} <= {Path(p).suffix for p in files}:
        raise ValueError('A complete site / JavaScript / Wasm hash manifest is required')
    for name, digest in files.items():
        path = (package / name).resolve()
        if not path.is_relative_to(package.resolve()) or sha(path) != digest:
            raise ValueError('Frozen browser package drift or escaped input')


def check_budget(path, cap):
    size = sum(p.stat().st_size for p in path.rglob('*') if p.is_file())
    if cap <= 0 or size > cap:
        raise ValueError(f'Receipt budget exceeded: {size} > {cap}')
    return size


def copy_tree(source, destination, cap):
    check_budget(source, cap)
    if any(p.is_symlink() for p in source.rglob('*')):
        raise ValueError('Input package must contain regular files, not symlinks')
    shutil.copytree(source, destination)


def verify_release(record):
    keys = ('context_closed', 'browser_closed', 'http_closed')
    if (record.get('schema') != 'instrument-release/v2'
            or any(record.get(key) is not True for key in keys)
            or record.get('close_errors') != []):
        raise ValueError('Instrument closure is unverified or failed')


def console_unlocked(value):
    roots = value if isinstance(value, list) else [value]
    if not roots or any(not isinstance(root, dict) for root in roots):
        raise ValueError('Unrecognized macOS console plist')
    known = [root for root in roots if 'IOConsoleLocked' in root or 'IOConsoleUsers' in root]
    if not known:
        raise ValueError('Console lock observation is unavailable')
    return not any(root.get('IOConsoleLocked', False) or
                   any(s.get('CGSSessionScreenIsLocked', False) for s in root.get('IOConsoleUsers', []))
                   for root in known)
