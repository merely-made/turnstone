# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Renew the bounded Instrument CPU/configuration gates on the current source."""
from pathlib import Path
import json
import os
import subprocess
import time
import sys

from common import parser, fresh_output, check_budget
cli = parser(__doc__)
cli.add_argument('--target', type=Path, required=True, help='Existing stable Cargo target')
args = cli.parse_args()
repo = args.repo.resolve()
if not args.target.is_dir():
    cli.error('Reuse an existing stable Cargo target')
out = fresh_output(args.output, repo)
env = dict(os.environ, CARGO_TARGET_DIR=str(args.target.resolve()),
           CARGO_BUILD_BUILD_DIR=str(args.target.resolve() / 'build'))
compiler = subprocess.check_output(['rustc', '--version'], env=env, text=True).strip()
web = ['cargo', 'check', '--manifest-path', 'ports/graphshell/web/Cargo.toml', '--locked',
       '--offline', '--target', 'wasm32-unknown-unknown', '--no-default-features', '-q']
checks = [
    ('shared-header', ['cargo', 'test', '-p', 'cambium', '--test', 'instrument', '--locked', '--offline', '-q']),
    ('workshop', ['cargo', 'test', '-p', 'tabard-workshop', '--test', 'embedding_api', '--test', 'shared_components',
                  '--test', 'surface', '--test', 'usability', '--test', 'instrument', '--locked', '--offline', '-q']),
    ('viewer', web),
    ('main-page', web + ['--features', 'main-page,applets']),
    ('port-boundaries', [sys.executable, 'scripts/check_port_boundaries.py']),
    ('source-headers', [sys.executable, 'scripts/relicense_headers.py', '--check', '--repo', '.']),
    ('whitespace', ['git', 'diff', '--check']),
]
source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
results = []
for label, command in checks:
    start = time.monotonic()
    with (out / (label + '.log')).open('w') as log:
        result = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    results.append({'label': label, 'command': command, 'exit': result.returncode,
                    'elapsed_seconds': round(time.monotonic() - start, 3)})
    (out / 'gates.json').write_text(json.dumps({'source': source, 'compiler': compiler,
        'target': env['CARGO_TARGET_DIR'], 'environment': {k:env[k] for k in env if k.startswith('CARGO_')},
        'checks': results}, indent=2) + '\n')
    print(json.dumps(results[-1]), flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
# Audit the complete metadata ledger, including dev/test and target-specific edges.
result = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--locked', '--offline'],
                        cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
assert result.returncode == 0, result.stderr
ledger = json.loads(result.stdout)
violations = []
for package in ledger['packages']:
    if not Path(package['manifest_path']).resolve().is_relative_to(repo / 'crates'):
        continue
    for dependency in package['dependencies']:
        if dependency.get('path') and Path(dependency['path']).resolve().is_relative_to(repo / 'ports'):
            violations.append({'package': package['name'], 'dependency': dependency})
(out / 'full-metadata-boundaries.json').write_text(json.dumps({'source': source,
    'packages': len(ledger['packages']), 'resolve_nodes': len(ledger['resolve']['nodes']),
    'all_dependency_kinds_and_targets': True, 'violations': violations}, indent=2) + '\n')
assert not violations

check_budget(out, args.max_bytes)
