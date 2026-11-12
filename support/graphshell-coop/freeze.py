#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Build or accept, then freeze, the native executables of two repositories.

Turnstone is the root (engine-split D130): the viewer comes from `--repo`; Djinn, the
seeder and the fixture custodian come from Graphshell `host/`, checked out at the exact
revision Turnstone's Cargo.lock pins for graphshell.git and built with host/Cargo.lock.
Both repositories' commits, locks, input files and resolved suppliers are recorded.
"""
import argparse
import hashlib
import json
import os
import plistlib
import platform
from pathlib import Path
import shlex
import shutil
import subprocess
import tomllib

GRAPHSHELL = 'https://github.com/merely-made/graphshell.git'
VIEWER = ['graphshell_mere']
HOST = ['djinn', 'coop_existing_graph', 'coop_fixture_custodian']
HOST_EXAMPLES = {'coop_existing_graph', 'coop_fixture_custodian'}
CODE = {'.rs', '.toml', '.wgsl', '.css', '.js', '.html'}
CARGO = str(Path.home() / '.cargo/bin/cargo')
RUSTC = str(Path.home() / '.cargo/bin/rustc')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(*command, cwd):
    return subprocess.check_output(command, cwd=cwd, text=True)


def lock_packages(path):
    return tomllib.loads(path.read_text())['package']


def graphshell_pin(lock):
    revisions = {p['source'].split('#')[1] for p in lock_packages(lock)
                 if p.get('source', '').startswith('git+' + GRAPHSHELL)}
    assert len(revisions) == 1, ('Turnstone must pin one graphshell.git revision', revisions)
    return revisions.pop()


def shared_lock_packages(turnstone, host):
    """Packages both locks name at one version: sources and checksums must agree."""
    def index(path):
        table = {}
        for p in lock_packages(path):
            table.setdefault((p['name'], p['version']), set()).add((p.get('source'), p.get('checksum')))
        return table
    left, right = index(turnstone), index(host)
    shared = sorted(left.keys() & right.keys())
    # A path package on either side is the Graphshell checkout itself (or a workspace
    # member), already tied to the pin; only resolved sources are compared.
    resolved = [k for k in shared if all(s for s, _ in left[k] | right[k])]
    differ = [{'name': n, 'version': v, 'turnstone': sorted(s for s, _ in left[(n, v)]),
               'graphshell': sorted(s for s, _ in right[(n, v)])}
              for n, v in resolved if left[(n, v)] != right[(n, v)]]
    return {'shared': len(shared), 'compared': len(resolved), 'differ': differ}


def checkout(host, revision):
    """Clone Graphshell when absent; an existing checkout is only checked, never moved."""
    if not host.exists():
        subprocess.check_call(['git', 'clone', '--quiet', '--no-checkout', GRAPHSHELL, str(host)])
        subprocess.check_call(['git', '-C', str(host), 'checkout', '--quiet', '--detach', revision])
    assert run('git', 'rev-parse', 'HEAD', cwd=host).strip() == revision, \
        (str(host), 'is not at the revision Turnstone pins', revision)
    assert not run('git', 'status', '--porcelain', '--untracked-files=no', cwd=host).strip(), \
        (str(host), 'has tracked changes')


def build(root, artifacts, host, host_artifacts):
    cargo = [CARGO, '+1.99.1', 'build', '--locked']
    subprocess.check_call([*cargo, '-p', 'graphshell-mere', '--bin', 'graphshell_mere',
                           '--target-dir', str(artifacts)], cwd=root)
    # Djinn alone, so the examples' dev-dependency features (in-process custody) never
    # reach it: this is the production passphrase-fd build.
    subprocess.check_call([*cargo, '-p', 'mere-resident', '--bin', 'djinn', '--features', 'passphrase-fd',
                           '--target-dir', str(host_artifacts)], cwd=host / 'host')
    subprocess.check_call([*cargo, '-p', 'mere-resident', '--example', 'coop_existing_graph',
                           '--example', 'coop_fixture_custodian', '--target-dir', str(host_artifacts)],
                          cwd=host / 'host')


def freeze_binary(name, artifacts, out):
    base = artifacts / 'debug' / ('examples' if name in HOST_EXAMPLES else '')
    for suffix in ['', '.d']:
        shutil.copy2(base / (name + suffix), out / (name + suffix))
    if name == 'djinn':
        probe = subprocess.run([str(out / name), '--passphrase-fd', 'invalid'],
                               capture_output=True, text=True)
        assert probe.returncode == 2 and '--passphrase-fd takes a number' in probe.stderr, \
            'Djinn must carry the explicit passphrase-fd test feature'
    if platform.system() == 'Linux':
        description = run('file', '-b', str(out / name), cwd=out).strip()
        assert description.startswith('ELF 64-bit'), description
    elif name == 'graphshell_mere':
        bundle = out / 'GraphshellCoop.app'
        executable = bundle / 'Contents/MacOS/graphshell_mere'
        executable.parent.mkdir(parents=True)
        shutil.copy2(out / name, executable)
        with (bundle / 'Contents/Info.plist').open('wb') as stream:
            plistlib.dump({'CFBundleIdentifier': 'made.merely.graphshell.coop.acceptance',
                           'CFBundleName': 'Graphshell co-op test',
                           'CFBundleExecutable': name, 'CFBundlePackageType': 'APPL',
                           'CFBundleVersion': '1', 'NSHighResolutionCapable': True,
                           'NSLocalNetworkUsageDescription':
                           'Connect to the chosen co-op graph on another computer.'}, stream)
        run('/usr/bin/codesign', '--force', '--sign', '-', '--timestamp=none', str(bundle), cwd=out)
        run('/usr/bin/codesign', '--verify', '--strict', str(bundle), cwd=out)
        shutil.copy2(executable, out / name)
    else:
        run('/usr/bin/codesign', '--force', '--sign', '-', '--timestamp=none',
            '--identifier', 'made.merely.coop.acceptance.' + name, str(out / name), cwd=out)
        run('/usr/bin/codesign', '--verify', '--strict', str(out / name), cwd=out)
    return {'sha256': sha(out / name), 'bytes': (out / name).stat().st_size,
            'architecture': (run('/usr/bin/lipo', '-archs', str(out / name), cwd=out).strip()
                             if platform.system() == 'Darwin' else platform.machine()),
            'format': run('file', '-b', str(out / name), cwd=out).strip(),
            'signing': ('ad-hoc, timestamp=none; no personal signing identity'
                        if platform.system() == 'Darwin' else 'not applicable: native ELF')}


def inputs(root, workspace, names, out, trees):
    """One repository's commit, lock, input files, git suppliers and external paths."""
    files = {name for name in run('git', 'ls-files', cwd=root).splitlines()
             if Path(name).suffix in CODE or Path(name).name == 'Cargo.lock'}
    for name in names:
        for dependency in shlex.split((out / (name + '.d')).read_text().split(': ', 1)[1]):
            path = Path(dependency).resolve()
            if path.is_relative_to(root) and path.is_file():
                files.add(path.relative_to(root).as_posix())
    metadata = json.loads(run(CARGO, '+1.99.1', 'metadata', '--locked', '--offline',
                              '--format-version', '1', cwd=workspace))
    groups = {}
    for package in metadata['packages']:
        source = package['source']
        if not source or not source.startswith('git+'):
            continue
        repo = run('git', 'rev-parse', '--show-toplevel', cwd=Path(package['manifest_path']).parent).strip()
        groups.setdefault(repo, {'source': source, 'packages': []})['packages'].append(package['name'])
    suppliers = []
    for repo, item in sorted(groups.items()):
        head = run('git', 'rev-parse', 'HEAD', cwd=repo).strip()
        assert head == item['source'].split('#')[-1], (repo, head)
        changed = run('git', 'diff', '--no-ext-diff', '--name-only', 'HEAD', '--', '*.rs', '*.toml',
                      'Cargo.lock', '*.wgsl', '*.c', '*.h', '*.cpp', '*.hpp', cwd=repo).strip()
        assert not changed, (repo, changed)
        suppliers.append({'checkout': repo, 'source': item['source'], 'head': head,
                          'packages': sorted(item['packages']), 'tracked_code_changes': []})
    texts = []
    for package, extra in trees:
        tree = run(CARGO, '+1.99.1', 'tree', '--locked', '--offline', '-p', package, *extra,
                   '--edges', 'normal,build', cwd=workspace)
        (out / (package + '-tree.txt')).write_text(tree)
        texts.append(tree)
    external = []
    for package in metadata['packages']:
        manifest = Path(package['manifest_path']).resolve()
        if package['source'] is None and not manifest.is_relative_to(root):
            compiled = any(str(manifest.parent) in tree for tree in texts)
            external.append({'name': package['name'], 'manifest': str(manifest), 'compiled': compiled})
            assert not compiled, ('unrecorded external path supplier', package['name'], manifest)
    return {'source_commit': run('git', 'rev-parse', 'HEAD', cwd=root).strip(),
            'lock_sha256': sha(workspace / 'Cargo.lock'),
            'files': {name: sha(root / name) for name in sorted(files)},
            'suppliers': suppliers, 'external_path_packages': external}


def main():
    parser = argparse.ArgumentParser()
    for name in ['repo', 'artifacts', 'host-repo', 'host-artifacts', 'output', 'harness']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--build', action='store_true',
                        help='clone Graphshell at the pin when absent and build both repositories')
    args = parser.parse_args()
    root, host = args.repo.resolve(), args.host_repo.resolve()
    pin = graphshell_pin(root / 'Cargo.lock')
    if args.build:
        checkout(host, pin)
        build(root, args.artifacts.resolve(), host, args.host_artifacts.resolve())
    checkout(host, pin)
    out = args.output.resolve()
    out.mkdir(mode=0o700)
    binaries = {name: freeze_binary(name, args.artifacts, out) | {'repository': 'turnstone'}
                for name in VIEWER}
    binaries |= {name: freeze_binary(name, args.host_artifacts, out) | {'repository': 'graphshell'}
                 for name in HOST}
    # The production build: passphrase-fd, never in-process custody (D130).
    features = run(CARGO, '+1.99.1', 'tree', '--locked', '--offline', '-p', 'mere-resident',
                   '--features', 'passphrase-fd', '--edges', 'normal,build', '--depth', '0',
                   '--format', '{f}', cwd=host / 'host').strip().split(',')
    assert 'passphrase-fd' in features and 'in-process-custody' not in features, features
    turnstone = inputs(root, root, VIEWER, out, [('graphshell-mere', [])])
    graphshell = inputs(host, host / 'host', HOST, out,
                        [('mere-resident', ['--features', 'passphrase-fd'])])
    assert graphshell['source_commit'] == pin
    shared = shared_lock_packages(root / 'Cargo.lock', host / 'host/Cargo.lock')
    names = ['run.py', 'machine.py', 'native_machine.py', 'network_machine.py',
             'resident.py', 'freeze.py', 'relay.py', 'verify.py', 'freeze_relay.py',
             'relay_server/Cargo.toml', 'relay_server/Cargo.lock', 'relay_server/src/main.rs']
    harness = {'support/graphshell-coop/' + name: sha(args.harness / name) for name in names}
    result = {**{k: turnstone[k] for k in ['source_commit', 'files', 'suppliers', 'external_path_packages']},
              'lock_sha256': turnstone['lock_sha256'],
              'host': graphshell | {'repository': GRAPHSHELL, 'workspace': 'host',
                                    'pinned_by': 'Turnstone Cargo.lock, graphshell.git',
                                    'djinn_features': sorted(features)},
              'shared_lock_packages': shared,
              'harness': harness, 'binaries': binaries,
              'rustc': run(RUSTC, '+1.99.1', '-Vv', cwd=root), 'cargo': run(CARGO, '+1.99.1', '-V', cwd=root),
              'supplier_scope': 'Resolved Git checkouts, including uncompiled packages. Tracked Rust/TOML/lock/WGSL/C/C++ only; excludes untracked and asset-only changes.',
              'environment': {key: os.environ.get(key) for key in ['CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR',
                              'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_BUILD_JOBS']}}
    (out / 'build-inputs.json').write_text(json.dumps(result, indent=2) + '\n')
    # Ruled (D130): packages both locks share resolve the same.
    assert not shared['differ'], ('the two locks resolve shared packages differently', shared['differ'])
    print(json.dumps({'result': 'ok', 'source': result['source_commit'], 'host': graphshell['source_commit'],
                      'files': len(turnstone['files']), 'host_files': len(graphshell['files']),
                      'git_suppliers': len(turnstone['suppliers']) + len(graphshell['suppliers']),
                      'shared_lock_packages': shared['compared'], 'binaries': binaries}), flush=True)


if __name__ == '__main__':
    main()
