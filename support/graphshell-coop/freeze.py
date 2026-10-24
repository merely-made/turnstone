#!/usr/bin/env python3
"""Freeze native executables and record exact repository/supplier inputs."""
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


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    for name in ['repo', 'artifacts', 'output', 'harness']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--owner', action='store_true')
    args = parser.parse_args()
    root = args.repo.resolve()
    out = args.output.resolve()
    out.mkdir(mode=0o700)
    cargo = str(Path.home() / '.cargo/bin/cargo')
    rustc = str(Path.home() / '.cargo/bin/rustc')

    def run(*command, cwd=root):
        return subprocess.check_output(command, cwd=cwd, text=True)

    names = ['graphshell_mere', 'djinn'] + (['coop_existing_graph'] if args.owner else [])
    files = set()
    for name in run('git', 'ls-files').splitlines():
        path = root / name
        if path.suffix in {'.rs', '.toml', '.wgsl', '.css', '.js', '.html'} or path.name == 'Cargo.lock':
            files.add(name)
    binaries = {}
    for name in names:
        base = args.artifacts / 'debug'
        if name == 'coop_existing_graph':
            base /= 'examples'
        for suffix in ['', '.d']:
            shutil.copy2(base / (name + suffix), out / (name + suffix))
        if name == 'djinn':
            probe = subprocess.run([str(out / name), '--passphrase-fd', 'invalid'],
                                   capture_output=True, text=True)
            assert probe.returncode == 2 and '--passphrase-fd takes a number' in probe.stderr, \
                'custodian must carry the explicit passphrase-fd test feature'
        if platform.system() == 'Linux':
            description = run('file', '-b', str(out / name)).strip()
            assert description.startswith('ELF 64-bit') and 'x86-64' in description, description
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
            run('/usr/bin/codesign', '--force', '--sign', '-', '--timestamp=none', str(bundle))
            run('/usr/bin/codesign', '--verify', '--strict', str(bundle))
            shutil.copy2(executable, out / name)
        else:
            run('/usr/bin/codesign', '--force', '--sign', '-', '--timestamp=none',
                '--identifier', 'made.merely.coop.acceptance.' + name, str(out / name))
            run('/usr/bin/codesign', '--verify', '--strict', str(out / name))
        binaries[name] = {'sha256': sha(out / name), 'bytes': (out / name).stat().st_size,
                          'architecture': (run('/usr/bin/lipo', '-archs', str(out / name)).strip()
                                           if platform.system() == 'Darwin' else platform.machine()),
                          'format': (run('file', '-b', str(out / name)).strip()),
                          'signing': ('ad-hoc, timestamp=none; no personal signing identity'
                                      if platform.system() == 'Darwin' else 'not applicable: native ELF')}
        for dependency in shlex.split((out / (name + '.d')).read_text().split(': ', 1)[1]):
            path = Path(dependency).resolve()
            if path.is_relative_to(root) and path.is_file():
                files.add(str(path.relative_to(root)))
    metadata = json.loads(run(cargo, '+1.99.1', 'metadata', '--locked', '--offline', '--format-version', '1'))
    groups = {}
    for package in metadata['packages']:
        source = package['source']
        if not source or not source.startswith('git+'):
            continue
        repo = run('git', 'rev-parse', '--show-toplevel', cwd=Path(package['manifest_path']).parent).strip()
        item = groups.setdefault(repo, {'source': source, 'packages': []})
        item['packages'].append(package['name'])
    suppliers = []
    for repo, item in sorted(groups.items()):
        head = run('git', 'rev-parse', 'HEAD', cwd=repo).strip()
        assert head == item['source'].split('#')[-1], (repo, head)
        changed = run('git', 'diff', '--no-ext-diff', '--name-only', 'HEAD', '--', '*.rs', '*.toml',
                      'Cargo.lock', '*.wgsl', '*.c', '*.h', '*.cpp', '*.hpp', cwd=repo).strip()
        assert not changed, (repo, changed)
        suppliers.append({'checkout': repo, 'source': item['source'], 'head': head,
                          'packages': sorted(item['packages']), 'tracked_code_changes': []})
    trees = []
    for package, extra in [('graphshell-mere', []), ('djinn', ['--features', 'passphrase-fd'])]:
        tree = run(cargo, '+1.99.1', 'tree', '--locked', '--offline', '-p', package,
                   *extra, '--edges', 'normal,build')
        (out / (package + '-tree.txt')).write_text(tree)
        trees.append(tree)
    external_paths = []
    for package in metadata['packages']:
        manifest = Path(package['manifest_path']).resolve()
        if package['source'] is None and not manifest.is_relative_to(root):
            compiled = any(str(manifest.parent) in tree for tree in trees)
            external_paths.append({'name': package['name'], 'manifest': str(manifest),
                                   'compiled': compiled})
            assert not compiled, ('unrecorded external path supplier', package['name'], manifest)
    names = ['run.py', 'machine.py', 'native_machine.py', 'network_machine.py',
             'resident.py', 'freeze.py', 'relay.py', 'verify.py', 'freeze_relay.py',
             'relay_server/Cargo.toml', 'relay_server/Cargo.lock', 'relay_server/src/main.rs']
    harness = {'support/graphshell-coop/' + name: sha(args.harness / name) for name in names}
    result = {'source_commit': run('git', 'rev-parse', 'HEAD').strip(),
              'files': {name: sha(root / name) for name in sorted(files)},
              'harness': harness, 'binaries': binaries,
              'rustc': run(rustc, '+1.99.1', '-Vv'), 'cargo': run(cargo, '+1.99.1', '-V'),
              'suppliers': suppliers,
              'external_path_packages': external_paths,
              'supplier_scope': 'Resolved Git checkouts, including uncompiled packages. Tracked Rust/TOML/lock/WGSL/C/C++ only; excludes untracked and asset-only changes.',
              'environment': {key: os.environ.get(key) for key in ['CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR',
                              'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG']}}
    (out / 'build-inputs.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'result': 'ok', 'source': result['source_commit'], 'files': len(files),
                      'git_suppliers': len(suppliers), 'binaries': binaries}), flush=True)


if __name__ == '__main__':
    main()
