#!/usr/bin/env python3
"""Freeze the exact built candidate after the orchestrator provides its SHA.
No product launch, Cargo invocation or workspace mutation occurs here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

BASE = Path(__file__).resolve().parent
WORKTREE = Path('/Users/markik/Code/worktrees/turnstone-tabard-apps')

def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--expected-sha256', required=True)
    options = parser.parse_args()
    if not re.fullmatch('[a-f0-9]{64}', options.expected_sha256):
        raise RuntimeError('expected production SHA must come from completed root build')
    before = sha(options.binary)
    if before != options.expected_sha256:
        raise RuntimeError('actual binary differs from orchestrator handoff')
    preparation = json.loads((BASE / 'preparation.json').read_text())
    for name, expected in preparation['shipped_fixture_sha256'].items():
        if sha(BASE / 'scenarios' / name) != expected or sha(WORKTREE / 'scenarios' / name) != expected:
            raise RuntimeError('shipped fixture/copy changed: ' + name)
    listed = subprocess.check_output(['git', '-C', str(WORKTREE), 'ls-files', '-z',
            '--cached', '--others', '--exclude-standard', '--', 'Cargo.toml', 'Cargo.lock', 'src', 'scenarios'])
    names = sorted(set(name.decode() for name in listed.split(b'\0') if name))
    if any('appearance_source_probe' in name or 'appearance_stage_probe' in name or
           'appearance_registration_probe' in name for name in names):
        raise RuntimeError('temporary source observer is present')
    inputs = {name: sha(WORKTREE / name) for name in names if (WORKTREE / name).is_file()}
    digest = hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    manifest = (WORKTREE / 'Cargo.toml').read_text()
    edges = re.findall(r'^([^\s=]+)\s*=\s*\{[^\n]*git\s*=\s*"([^"]+)"[^\n]*rev\s*=\s*"([^"]+)"', manifest, re.M)
    families = {}
    for alias, repository, revision in edges:
        families.setdefault(repository, {}).setdefault(revision, []).append(alias)
    lock_counts = {}
    for source in re.findall(r'^source = "(git[^"\n]+)"$', (WORKTREE / 'Cargo.lock').read_text(), re.M):
        lock_counts[source] = lock_counts.get(source, 0) + 1
    if sha(options.binary) != before:
        raise RuntimeError('binary changed during CPU freeze')
    record = {'candidate': 'native13 Tabard field styling with DR-C/D migrated production, prepared for native acceptance',
      'head': subprocess.check_output(['git', '-C', str(WORKTREE), 'rev-parse', 'HEAD'], text=True).strip(),
      'binary_sha256': before, 'build_input_sha256': inputs, 'build_input_digest': digest,
      'input_count': len(inputs), 'excludes': ['docs/receipts', 'design_docs', 'targets', 'private profiles'],
      'manifest_git_revision_edges': families, 'lock_git_source_package_counts_static_only': lock_counts,
      'metadata_closure': 'orchestrator-owned validation; static lock counts are not cargo metadata qualification',
      'acceptance_configuration': preparation,
      'native_result': 'not claimed by source freeze; requires actual four lanes and visual/byte gates'}
    output = BASE / 'source-manifest.json'
    if output.exists():
        raise RuntimeError('source freeze already exists; preserve prior evidence')
    output.write_text(json.dumps(record, indent=2) + '\n')
    output.chmod(0o600)
    print(json.dumps({'source_manifest': str(output), 'input_count': len(inputs),
      'build_input_digest': digest, 'binary_sha256': before}, indent=2))

if __name__ == '__main__':
    main()
