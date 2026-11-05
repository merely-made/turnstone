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
import hashlib

from common import parser, fresh_output, check_budget
cli = parser(__doc__)
cli.add_argument('--target', type=Path, required=True, help='Existing stable Cargo target')
cli.add_argument('--cargo-wrapper', type=Path, help='Existing build-storage launcher; does not install tools')
cli.add_argument('--cargo-lane', help='Stable lane required with --cargo-wrapper')
cli.add_argument('--wrapper-python', default=sys.executable)
args = cli.parse_args()
repo = args.repo.resolve()
if not args.target.is_dir():
    cli.error('Reuse an existing stable Cargo target')
if bool(args.cargo_wrapper) != bool(args.cargo_lane):
    cli.error('--cargo-wrapper and --cargo-lane must be supplied together')
out = fresh_output(args.output, repo)
build_dir = args.target.resolve().parent / 'build' if args.cargo_wrapper else args.target.resolve() / 'build'
env = dict(os.environ, CARGO_TARGET_DIR=str(args.target.resolve()), CARGO_BUILD_BUILD_DIR=str(build_dir))
compiler = subprocess.check_output(['rustc', '--version'], cwd=repo, env=env, text=True).strip()
web = ['cargo', 'check', '--locked',
       '--offline', '--target', 'wasm32-unknown-unknown', '--no-default-features', '-q']
checks = [
    ('shared-header', ['cargo', 'test', '-p', 'cambium', '--test', 'instrument', '--locked', '--offline', '-q']),
    ('workshop', ['cargo', 'test', '-p', 'tabard-workshop', '--test', 'embedding_api', '--test', 'shared_components',
                  '--test', 'surface', '--test', 'usability', '--test', 'instrument', '--locked', '--offline', '-q']),
    ('viewer', web + ['--features', 'presentation-webgpu']),
    ('product-applets', web + ['--features', 'product,applets,presentation-webgpu']),
    ('port-boundaries', [sys.executable, 'scripts/check_port_boundaries.py']),
    ('source-headers', [sys.executable, 'scripts/relicense_headers.py', '--check', '--repo', '.']),
    ('whitespace', ['git', 'diff', '--check']),
]
source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip()
results = []
for label, command in checks:
    cwd = repo / 'ports/graphshell/web' if label in {'viewer', 'product-applets'} else repo
    actual = command
    if args.cargo_wrapper and command[0] == 'cargo':
        reserve = '6' if command[1] == 'check' else '40'
        actual = [args.wrapper_python, str(args.cargo_wrapper.resolve()), 'build',
                  '--lane', args.cargo_lane, '--cwd', str(cwd), '--reserve-gib', reserve, '--', *command[1:]]
    before = subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True)
    if before:
        raise ValueError('CPU qualification requires clean source')
    start = time.monotonic()
    with (out / (label + '.log')).open('w') as log:
        log.write(json.dumps({'launch': actual, 'cwd': str(cwd), 'source': source}) + '\n'); log.flush()
        result = subprocess.run(actual, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
    after = subprocess.check_output(['git', 'status', '--porcelain'], cwd=repo, text=True)
    results.append({'label': label, 'command': command, 'actual_command': actual, 'cwd': str(cwd),
                    'exit': result.returncode, 'status_before': before, 'status_after': after,
                    'log_sha256': hashlib.sha256((out / (label + '.log')).read_bytes()).hexdigest(),
                    'elapsed_seconds': round(time.monotonic() - start, 3)})
    (out / 'gates.json').write_text(json.dumps({'source': source, 'compiler': compiler,
        'target': env['CARGO_TARGET_DIR'], 'build_dir': str(build_dir),
        'environment': {k:env[k] for k in ('CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR',
          'CARGO_INCREMENTAL', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG', 'CARGO_BUILD_JOBS') if k in env},
        'checks': results}, indent=2) + '\n')
    print(json.dumps(results[-1]), flush=True)
    if after:
        raise SystemExit(1)
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
raise SystemExit(int(any(check['exit'] for check in results)))
