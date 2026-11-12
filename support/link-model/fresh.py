# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Verify a new link-family qualification without rewriting historical records."""
import argparse
from collections import Counter
import hashlib
from pathlib import Path
import re
import subprocess
import sys
import ordinary_paths as check
import shared_attachments as attachments
import counted_topology as topology

BROWSER_SETUP = 'assert snap shell-inspector-open == false\nassert snap shell-graph-controls == false\nclick role:button Inspector\nclick role:button Panes\nclick role:button Graph controls\nsettle 10\nassert snap shell-inspector-open == true\nassert snap shell-graph-controls == true\nassert snap shell-panes-open == false\n'
TOPOLOGY_SETUP = 'assert snap shell-inspector-open == false\nassert snap shell-graph-controls == false\nclick role:button Inspector\nsettle 10\nassert snap shell-inspector-open == true\nassert snap shell-graph-controls == false\n'


def definition(family):
    values = {
        'ordinary': (check.SEED_SCENARIO, check.REOPEN_SCENARIO,
                     check.SEED_CAPTURES, check.REOPEN_CAPTURES, 'link-rest'),
        'attachments': (attachments.SEED, attachments.REOPEN, attachments.SEED_CAPTURES,
                        attachments.REOPEN_CAPTURES, 'link-rest'),
        'topology': (topology.SEED, topology.REOPEN, topology.SEED_CAPTURES,
                     topology.REOPEN_CAPTURES, 'promoted-link'),
    }
    if family not in values:
        check.fail('unknown fresh link family')
    seed, reopen, seed_captures, reopen_captures, fixture = values[family]
    return [{'phase': phase, 'name': name, 'fixture': fixture, 'captures': sorted(captures)}
            for phase, name, captures in [('seed', seed, seed_captures),
                                          ('reopen', reopen, reopen_captures)]]


def git_blob(repo, revision, relative):
    result = subprocess.run(['git', '-C', str(repo), 'show', revision + ':' + relative], capture_output=True)
    if result.returncode:
        check.fail('cannot read committed qualification input: ' + relative)
    return result.stdout


def complete_test_scope(command, role, package, listing=None):
    """Allow scheduling knobs, never a test-name or target filter."""
    if '--' not in command:
        check.fail('explicit libtest arguments required')
    cargo, libtest = command[1:command.index('--')], command[command.index('--') + 1:]
    if cargo and cargo[0].startswith('+'):
        cargo = cargo[1:]
    if not cargo or cargo.pop(0) != 'test':
        check.fail('test command required')
    flags = set()
    options = {}
    while cargo:
        token = cargo.pop(0)
        if token in {'--locked', '--offline', '--lib', '--no-default-features'}:
            flags.add(token)
        elif token in {'-p', '--package', '--features', '-j', '--jobs'} and cargo:
            options[token] = cargo.pop(0)
        else:
            check.fail('filtered or unknown Cargo test scope: ' + token)
    if options.get('-p', options.get('--package')) != package:  # --locked is allowed, not required: no lock is tracked.
        check.fail('complete package selection required')
    if role == 'canvas' and (options.get('--features') != 'canvas' or '--lib' in flags or '--no-default-features' in flags):
        check.fail('Canvas requires its full canvas-feature suite, including integration/doc tests')
    if role == 'engine' and ('--lib' in flags or '--no-default-features' in flags or '--features' in options):
        check.fail('Seiche requires its complete default suite')
    if role == 'graphshell' and (options.get('--features') != 'web' or '--no-default-features' not in flags or '--lib' not in flags):
        check.fail('Graphshell requires the complete web-library scope')
    if listing is not None:
        expected = ['--list'] + (['--ignored'] if listing else []) + ['--format', 'terse']
        if libtest != expected:
            check.fail('test inventory must be unfiltered')
    elif not (len(libtest) == 1 and re.fullmatch(r'--test-threads=[1-9][0-9]*', libtest[0])):
        check.fail('only the declared thread count may configure test execution')


def test_inventory(root, revision, role, inventory):
    package = {'canvas': 'pictograph', 'graphshell': 'graphshell', 'engine': 'seiche'}[role]
    if inventory.get('package') != package:
        check.fail('test inventory package mismatch: ' + role)
    names = []
    for key in ('list_record', 'ignored_record'):
        path = check.resolve_relative(root, inventory[key], key, '.json')
        check.verify_command_record(root, path, revision, True, ('cargo', 'test', '--list', package))
        record = check.read_json(path)
        command = record['command']
        complete_test_scope(command, role, package, key == 'ignored_record')
        if '-p' not in command or command[command.index('-p') + 1] != package:
            check.fail('inventory does not identify its package')
        if role == 'canvas' and ('--features' not in command or command[command.index('--features') + 1] != 'canvas'):
            check.fail('canvas test inventory must enable canvas')
        if role == 'graphshell' and ('--no-default-features' not in command or '--features' not in command
                                    or command[command.index('--features') + 1] != 'web' or '--lib' not in command):
            check.fail('Graphshell inventory must identify the web library scope')
        if key == 'ignored_record' and '--ignored' not in command:
            check.fail('ignored inventory must explicitly list ignored tests')
        parsed = sorted(re.findall(r'^(.+): test$', path.with_suffix('.log').read_text(encoding='utf-8'), re.M))
        expected = inventory['tests' if key == 'list_record' else 'ignored']
        if parsed != sorted(expected):
            check.fail('test inventory differs from its actual listing log')
        names.append(Counter(parsed))
    if not names[0] or names[1] - names[0]:
        check.fail('empty or invalid ignored test inventory')
    return sum(names[0].values()), sum(names[1].values())


def verify_browser_setup(root, scenario, result, family='ordinary'):
    folder = root / 'browser-qualified' / scenario['name']
    setup = check.read_json(folder / 'setup.json')
    expected = TOPOLOGY_SETUP if family == 'topology' else BROWSER_SETUP
    if (setup.get('script') != expected
            or setup.get('sha256') != hashlib.sha256(expected.encode()).hexdigest()
            or setup.get('scenario', {}).get('state') != 'ok'
            or setup['scenario'].get('errors') != [] or setup['scenario'].get('gate_failures') != []
            or setup['scenario'].get('captures') != []
            or setup['scenario'].get('result', {}).get('result') != 'ok'):
        check.fail('Inspector setup must pass before the original scenario')
    path = root / 'site/ports/graphshell/web/scenarios' / (scenario['name'] + '.scn')
    if result.get('executed_scenario_sha256') != check.sha256_file(path):
        check.fail('executed scenario bytes differ from the sealed input')


def verify(root, repo):
    fresh = check.read_json(root / 'fresh.json')
    if fresh.get('schema') != 'fresh-link-family/v1':
        check.fail('fresh qualification schema missing')
    scenarios = definition(fresh.get('family'))
    if fresh.get('scenarios') != scenarios:
        check.fail('fresh scenario/fixture/capture definition changed')
    revision, records = check.verify_build_manifest(root)
    if fresh.get('source_revision') != revision:
        check.fail('fresh descriptor and build source differ')
    build = check.read_json(root / 'build.json')
    artifacts = {row['path'] for row in build['artifacts']}
    if not {'pkg/graphshell_web.js', 'pkg/graphshell_web.d.ts', 'pkg/graphshell_web_bg.wasm',
            'pkg/graphshell_web_bg.wasm.d.ts'}.issubset(artifacts):
        check.fail('all four generated package inputs must be sealed')
    for name, tokens in [('build', ('cargo', 'build')), ('bindgen', ('bindgen',))]:
        check.verify_command_record(root, records[name], revision, True, tokens)
    if fresh['family'] == 'topology':
        for name, tokens in [('viewer', ('cargo', 'check', 'presentation-webgpu')),
                             ('boundaries', ('check_port_boundaries.py',))]:
            path = check.resolve_relative(root, fresh[name + '_record'], name, '.json')
            check.verify_command_record(root, path, revision, True, tokens)
    inventories = check.read_json(root / 'test-inventories.json')
    if set(inventories) != {'canvas', 'graphshell', 'engine'}:
        check.fail('all three complete CPU inventories are required')
    totals = {}
    for role, package in [('canvas', 'pictograph'), ('graphshell', 'graphshell'), ('engine', 'seiche')]:
        all_count, ignored_count = test_inventory(root, revision, role, inventories[role])
        record_path = records[role] if role in records else check.resolve_relative(root, fresh['engine_record'], 'engine_record', '.json')
        passed = check.verify_cargo_qualification(root, record_path, revision, package)
        record = check.read_json(record_path)
        if record.get('status_after') != '':
            check.fail('test command did not leave its source clean')
        complete_test_scope(record['command'], role, package)
        listing = check.read_json(check.resolve_relative(root, inventories[role]['list_record'], 'listing', '.json'))
        # Only the libtest arguments may differ between inventory and execution.
        if record['command'][:record['command'].index('--')] != listing['command'][:listing['command'].index('--')]:
            check.fail('test execution and inventory have different Cargo/feature scopes')
        if (passed != all_count - ignored_count or
                sum(s['ignored'] for s in record['suites']) != ignored_count or
                any(s['filtered_out'] or s['measured'] for s in record['suites'])):
            check.fail('passing/ignored counts differ from complete test inventories')
        totals[role] = {'passed': passed, 'ignored': ignored_count}
    expected_inputs = {'ports/graphshell/web/scenarios/' + s['name'] + '.scn' for s in scenarios}
    inputs = build.get('qualification_inputs', [])
    if len(inputs) != 2 or {row.get('path') for row in inputs} != expected_inputs:
        check.fail('exactly the two current scenario inputs must be sealed')
    for row in inputs:
        path = check.resolve_relative(root / 'site', row['path'], 'scenario', '.scn')
        if (check.sha256_file(path) != row['sha256'] or
                hashlib.sha256(git_blob(repo, revision, row['path'])).hexdigest() != row['sha256']):
            check.fail('executed scenario differs from committed source input')
    seed, reopened = [check.verify_browser_scenario(root, s['name'], set(s['captures'])) for s in scenarios]
    for scenario, result in zip(scenarios, (seed, reopened)):
        verify_browser_setup(root, scenario, result, fresh['family'])
    for result in (seed, reopened):
        if (result.get('page_errors') != [] or result.get('runtime', {}).get('visibility') != 'visible'
                or result.get('runtime', {}).get('viewport') != [1600, 1000]
                or result.get('root', {}).get('data-selected-render-backend') != 'webgpu'):
            check.fail('visible headed WebGPU scenario result is incomplete')
    check.verify_source_comparison(root, seed, reopened)
    if fresh['family'] in {'attachments', 'topology'} and reopened['scenario']['result']['saved_graph'].get('reopened') is not True:
        check.fail('fresh process did not reopen stored graph data')
    expected_images = {f"browser-qualified/{s['name']}/{capture}.png" for s in scenarios for capture in s['captures']}
    reviews = check.read_json(root / 'browser-review.json').get('images', [])
    if len(reviews) != len(expected_images) or {r.get('path') for r in reviews} != expected_images:
        check.fail('visual review must cover every accepted image exactly once')
    for row in reviews:
        if row.get('reviewed') is not True or check.sha256_file(root / row['path']) != row['sha256']:
            check.fail('visual review image/hash mismatch')
        observations = row.get('reviews', [])
        if not observations:
            check.fail('named visual findings required')
        for observation in observations:
            check.require_string(observation, 'reviewer', 'visual review')
            check.require_string(observation, 'finding', 'visual review')
            if observation.get('disposition') != 'accepted':
                check.fail('visual review did not accept the capture')
    process = check.read_json(root / 'browser-processes.json')
    if process.get('source_revision') != revision:
        check.fail('browser source binding differs')
    observations = process.get('observations', [])
    if [row.get('phase') for row in observations] != ['seed-before-navigation', 'seed-exit',
                                                    'reopen-before-navigation', 'reopen-exit']:
        check.fail('exact seed/reopen process observation sequence required')
    rows = {row['phase']: row['chrome_pids'] for row in observations}
    before, after = rows.get('seed-before-navigation', []), rows.get('reopen-before-navigation', [])
    if len(before) != 1 or len(after) != 1 or before == after or rows.get('seed-exit') != [] or rows.get('reopen-exit') != []:
        check.fail('distinct owned seed/reopen Chrome processes and observed exits required')
    runtime = check.read_json(root / 'browser-runtime.json')
    if (runtime.get('source_revision') != revision or runtime.get('headed') is not True
            or runtime.get('render_backend') != 'webgpu' or not runtime.get('adapter')
            or not re.search(r'amd|ati|radeon', str(runtime['adapter']), re.I)
            or re.search(r'swiftshader|llvmpipe', str(runtime['adapter']), re.I)):
        check.fail('required owned headed AMD WebGPU runtime was not observed')
    if (runtime.get('owned_pids_remaining') != [] or runtime.get('profile_removed') is not True
            or runtime.get('http_closed') is not True or runtime.get('http_port_rebound') is not True
            or runtime.get('close_errors') != [] or runtime.get('error')):
        check.fail('owned process/profile/listener cleanup was not confirmed')
    return {'source_revision': revision, 'family': fresh['family'], 'tests': totals,
            'images': len(expected_images), 'fresh_qualification': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('qualification_root', type=Path)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    result = verify(args.qualification_root.resolve(), args.repo.resolve())
    import json
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    try:
        main()
    except check.VerificationError as error:
        print('fresh verification failed: ' + str(error), file=sys.stderr)
        raise SystemExit(1)
