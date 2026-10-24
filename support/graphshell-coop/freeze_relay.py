#!/usr/bin/env python3
"""Freeze the separate relay; verify compiled registry packages against locked archives."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import tarfile
import tomllib


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser()
    for name in ['repo', 'artifacts', 'build-dir', 'output']:
        p.add_argument('--' + name, type=Path, required=True)
    args = p.parse_args()
    root, out = args.repo.resolve(), args.output.resolve()
    fixture = root / 'support/graphshell-coop/relay_server'
    out.mkdir(mode=0o700)
    binary = args.artifacts / 'debug/graphshell-coop-relay-receipt'
    shutil.copy2(binary, out / 'relay')
    registry = Path.home() / '.cargo/registry/src'
    packages = set()
    for dep in (args.build_dir / 'debug/deps').glob('*.d'):
        for token in shlex.split(dep.read_text().replace('\\\n', ' ')):
            path = Path(token.removesuffix(':'))
            if path.is_file() and path.is_relative_to(registry):
                parts = path.relative_to(registry).parts
                packages.add((parts[0], parts[1]))
    lock = tomllib.loads((fixture / 'Cargo.lock').read_text())
    expected = {x['name'] + '-' + x['version']: x['checksum']
                for x in lock['package'] if 'checksum' in x}
    verified, audit = 0, []
    for index, package in sorted(packages):
        archive = Path.home() / '.cargo/registry/cache' / index / (package + '.crate')
        assert sha(archive) == expected[package], package
        count = 0
        with tarfile.open(archive) as tar:
            for member in tar.getmembers():
                if member.isfile():
                    relative = Path(member.name).relative_to(package)
                    path = registry / index / package / relative
                    assert hashlib.sha256(tar.extractfile(member).read()).hexdigest() == sha(path), path
                    count += 1
        verified += count
        audit.append({'package': package, 'archive_sha256': expected[package], 'source_files_verified': count})
    assert verified > 0
    files = {str(path.relative_to(root)): sha(path) for path in
             [fixture / 'Cargo.toml', fixture / 'Cargo.lock', fixture / 'src/main.rs', Path(__file__).resolve()]}
    result = {'source_commit': subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(),
              'sha256': sha(out / 'relay'), 'bytes': (out / 'relay').stat().st_size,
              'architecture': platform.machine(), 'files': files,
              'rustc': subprocess.check_output([str(Path.home()/'.cargo/bin/rustc'), '+1.99.1', '-Vv'], text=True),
              'registry_files_verified': verified, 'registry_packages': audit,
              'format': subprocess.check_output(['file', '-b', str(out/'relay')], text=True).strip()}
    (out / 'build-inputs.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'result': 'ok', 'sha256': result['sha256'], 'registry_files_verified': verified,
                      'registry_packages': len(audit)}))


if __name__ == '__main__':
    main()
