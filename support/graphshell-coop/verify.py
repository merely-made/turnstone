#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Verify a receipt packet: source inputs of both repositories, machine identities, and
either the two-machine co-op behavior or the single-desktop baseline with its positive control."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tomllib

import locks

ROOT = Path(__file__).resolve().parents[2]
GRAPHSHELL = 'https://github.com/merely-made/graphshell.git'


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_sources(files, commit, root=ROOT):
    if not commit:
        for name, expected in files.items():
            assert sha(root / name) == expected, name
        return
    process = subprocess.Popen(['git', 'cat-file', '--batch'], cwd=root,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        for name, expected in files.items():
            process.stdin.write((commit + ':' + name + '\n').encode())
            process.stdin.flush()
            header = process.stdout.readline().decode().split()
            assert len(header) == 3 and header[1] == 'blob', name
            blob = process.stdout.read(int(header[2]))
            assert process.stdout.read(1) == b'\n'
            assert hashlib.sha256(blob).hexdigest() == expected, name
    finally:
        process.stdin.close()
        process.wait(timeout=10)
    assert process.returncode == 0


def assert_generated(names, commit, root):
    """Recorded build-generated inputs: by hash only, so they must be absent from the commit."""
    for name in names:
        listed = subprocess.run(['git', 'cat-file', '-e', commit + ':' + name], cwd=root,
                                capture_output=True)
        assert listed.returncode != 0, ('a tracked file recorded as generated', name)


def verify_graphics(graphics, machine):
    """The native process held the recorded GPU's render node and loaded its Vulkan driver."""
    assert graphics['VK_DRIVER_FILES'] == machine['graphical_environment']['VK_DRIVER_FILES']
    gpu = machine.get('gpu') or {'vendor': '0x1002', 'vulkan_library': 'libvulkan_radeon'}  # Older packets.
    assert graphics['render_nodes'] and all(n['vendor'] == gpu['vendor'] for n in graphics['render_nodes'])
    library = Path(gpu['vulkan_library']).stem
    assert any(library in p for p in graphics['driver_libraries']), (library, graphics['driver_libraries'])


def lock_differences(turnstone, host):
    """Every shared crate (name, version) whose resolution differs: freeze.py's comparison."""
    def index(text):
        table = {}
        for p in tomllib.loads(text)['package']:
            table.setdefault((p['name'], p['version']), set()).add((p.get('source'), p.get('checksum')))
        return table
    left, right = index(turnstone), index(host)
    shared = sorted(left.keys() & right.keys())
    resolved = [k for k in shared if all(s for s, _ in left[k] | right[k])]
    return {'shared': len(shared), 'compared': len(resolved),
            'differ': [{'name': n, 'version': v, 'turnstone': sorted(s for s, _ in left[(n, v)]),
                        'graphshell': sorted(s for s, _ in right[(n, v)])}
                       for n, v in resolved if left[(n, v)] != right[(n, v)]]}


def verify_host(inputs, machine, args):
    """Graphshell host/ at the revision Turnstone's lock pins, its lock and inputs (D130)."""
    host = inputs['host']
    lock = (locks.at_commit(ROOT, args.commit, 'Cargo.lock') if args.commit
            else locks.current(ROOT, 'Cargo.lock'))
    assert hashlib.sha256(lock).hexdigest() == inputs['lock_sha256'] == machine['lock_sha256']
    pins = {p['source'].split('#')[1] for p in tomllib.loads(lock.decode())['package']
            if p.get('source', '').startswith('git+' + GRAPHSHELL)}
    assert pins == {host['source_commit']} == {machine['host_commit']}, pins
    assert host['lock_sha256'] == machine['host_lock_sha256']
    assert args.host_repo, '--host-repo (a Graphshell clone) is required to check host inputs'
    verify_sources(host['files'], host['source_commit'], args.host_repo.resolve())
    assert_generated(host.get('generated_files', {}), host['source_commit'], args.host_repo.resolve())
    assert all(not p['compiled'] for p in host['external_path_packages'])
    assert all(p['head'] == p['source'].split('#')[-1] and not p['tracked_code_changes']
               for p in host['suppliers'])
    assert 'passphrase-fd' in host['djinn_features'] and 'in-process-custody' not in host['djinn_features']
    # Recorded, not required to agree; the record must be exactly what both locks say (coop-harness D2).
    host_lock = locks.at_commit(args.host_repo.resolve(), host['source_commit'], 'host/Cargo.lock')
    assert hashlib.sha256(host_lock).hexdigest() == host['lock_sha256']
    assert inputs['shared_lock_packages'] == lock_differences(lock.decode(), host_lock.decode())
    return host


def never_ready(receipt, chosen):
    """The positive control: the viewer ran its await to the deadline and never reached ready.
    Judged on the viewer's own fields; an expired await leaves the receipt ok."""
    final = (receipt or {}).get('final') or {}
    status = final.get('status')
    return (bool(receipt) and status not in {None, 'ready'} and final.get('attached') != chosen
            and f'status expected "ready", was "{status}"' in (final.get('await-failed') or ''))


def check_capture(evidence, capture):
    path = evidence / Path(capture['path']).name
    data = path.read_bytes()
    assert data[:8] == b'\x89PNG\r\n\x1a\n'
    assert struct.unpack('>II', data[16:24]) == (capture['width'], capture['height'])
    assert capture['width'] > 200 and capture['height'] > 200
    return path.stem


def verify_review(HERE, evidence, names):
    review = read(HERE / 'capture-review.json')
    assert {r['capture'] for r in review} == {name + '.png' for name in names}
    assert all(r['reviewed'] and r['observation']
               and sha(evidence / r['capture']) == r['sha256'] for r in review)


def verify_baseline(HERE, q, args):
    """One desktop: custody, grant, resident, seed, baseline phase, positive control."""
    evidence = HERE / 'accepted'
    assert q['result'] == 'ok' and q['custodian_launches'] == 1
    owner = q['machines']['owner']
    assert owner['operating_system'] in {'Darwin', 'Linux'}
    assert owner['console_present'] and not owner['console_locked']
    inputs = read(HERE / 'build-inputs.json')
    assert owner['source_commit'] == inputs['source_commit']
    assert owner['lock_sha256'] == inputs['lock_sha256']
    verify_sources(inputs['files'] | inputs['harness'], args.commit)
    assert args.commit, '--commit is required: generated inputs are checked against it'
    assert_generated(inputs['generated_files'], args.commit, ROOT)
    host = verify_host(inputs, owner, args)
    assert all(not p['compiled'] for p in inputs['external_path_packages'])
    assert all(p['head'] == p['source'].split('#')[-1] and not p['tracked_code_changes']
               for p in inputs['suppliers'])
    assert q['worker_sha256'] == owner['worker_sha256'] == inputs['harness']['support/graphshell-coop/machine.py']
    assert q['controller_sha256'] == inputs['harness']['support/graphshell-coop/run.py']
    signing = 'ad-hoc,' if owner['operating_system'] == 'Darwin' else 'not applicable: native ELF'
    assert set(owner['binaries']) == set(inputs['binaries'])
    assert all(owner['binaries'][n] == b['sha256'] and b['architecture'] == owner['architecture']
               and b['signing'].startswith(signing) for n, b in inputs['binaries'].items())
    # Custody: the fixture custodian, handed to a production passphrase-fd Djinn.
    assert q['custody'] == owner['custody'] and q['custody'].startswith('fixture-custodian:')
    custodian = read(evidence / 'owner-resident/custodian.json')
    assert custodian['ready'] and custodian['profile'] == 'CrossGraphOwner'
    assert custodian['custody'] == 'fixture-custodian' and custodian['synthetic']
    log = (evidence / 'owner-resident/djinn-1.log').read_text()
    assert 'custody reached' in log and 'in_process=false' in log, 'Djinn did not reach the custodian'
    # Consent: the grant came from Graphshell's own grant_for and is what Djinn loaded.
    grant = read(evidence / 'owner-grant.json')
    assert grant['origin'] == q['grant_origin'] and grant['prepare_sha256'] == owner['binaries']['coop_existing_graph']
    assert grant['grant']['app'] == 'graphshell'
    assert read(evidence / 'owner-settings.json')['applications'] == [grant['grant']]
    status = q['resident_status']
    assert status['ready'] and status['lock'] == 'unlocked'
    events = [json.loads(line) for line in (evidence / 'owner-resident/events-1.jsonl').read_text().splitlines()]
    ready = next(e for e in events if e['event'] == 'ready')
    stopped = next(e for e in events if e['event'] == 'stopped')
    assert stopped['ok'] and stopped['reason'] == 'stop intent' and ready['pid'] == stopped['pid'] == status['pid']
    assert not (evidence / 'owner-resident/events-2.jsonl').exists(), 'one Djinn launch only'
    # Seed through the door, then the viewer attached to the chosen graph and switched.
    chosen, private = q['graphs']['chosen'], q['graphs']['private']
    assert chosen != private and read(evidence / 'existing-graphs.json') == q['graphs']
    result = read(evidence / 'baseline.json')
    assert result['ok'] and not result['errors'] and result['final']['status'] == 'ready'
    assert not result['final'].get('await-failed'), 'the baseline awaited something it never saw'
    assert result['final']['attached'] == chosen
    launch = read(evidence / 'baseline-launch.json')
    pid = int(result['final']['pid'])
    assert launch['launcher_pid'] == pid and q['native_processes'] == [{'machine': 'owner', 'pid': pid}]
    assert launch['machine']['machine_id'] == owner['machine_id'] and launch['machine']['binaries'] == owner['binaries']
    captures = {check_capture(evidence, c): c['fields'] for c in result['captures']}
    assert set(captures) == {'private-before', 'existing-graph'}
    assert captures['private-before']['attached'] == private and captures['existing-graph']['attached'] == chosen
    if owner['operating_system'] == 'Linux':
        graphics = read(evidence / 'baseline-graphics.json')
        assert graphics['pid'] == pid
        verify_graphics(graphics, owner)
    # Positive control: launched after the resident stopped; never ready.
    control, outcome = read(evidence / 'control.json'), q['control']
    trace = [t['method'] for t in read(HERE / 'control-trace.json')]
    assert trace.index('stop') < len(trace) - 1 - trace[::-1].index('launch'), 'control launched before stop'
    assert outcome['failed_as_expected'] and not outcome['terminated'] and never_ready(control, chosen)
    assert int(control['final']['pid']) == read(evidence / 'control-launch.json')['launcher_pid'] != pid
    control_captures = [check_capture(evidence, c) for c in control['captures']]
    cleanup = read(HERE / 'cleanup.json')
    assert cleanup['accepted_cohort'] == Path(owner['public']).parents[1].name
    assert not cleanup['owner']['remaining']
    assert q['all_owned_processes_stopped'] is (not cleanup['owner']['remaining'])
    verify_review(HERE, evidence, [*captures, *control_captures])
    print(json.dumps({'result': 'ok', 'mode': 'baseline',
                      'owner': {'operating_system': owner['operating_system'], 'system_version': owner['system_version'],
                                'architecture': owner['architecture'], 'gpu': owner.get('gpu')},
                      'turnstone': {'commit': inputs['source_commit'], 'lock_sha256': inputs['lock_sha256']},
                      'graphshell': {'commit': host['source_commit'], 'lock_sha256': host['lock_sha256']},
                      'shared_lock_packages': {
                          'compared': inputs['shared_lock_packages']['compared'],
                          'differ': sorted(d['name'] + ' ' + d['version'] for d in inputs['shared_lock_packages']['differ'])},
                      'custody': q['custody'], 'grant_origin': q['grant_origin'],
                      'baseline': {'pid': pid, 'attached': chosen, 'captures': sorted(captures)},
                      'control': {'status': control['final']['status'], 'await_failed': control['final']['await-failed'],
                                  'captures': control_captures},
                      'source_files': len(inputs['files']), 'host_files': len(host['files']),
                      'generated_files': sorted(inputs['generated_files']) + sorted(host['generated_files']),
                      'commit': args.commit}, indent=2))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--commit')
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--host-repo', type=Path, help='a Graphshell clone holding the pinned revision')
    args = parser.parse_args()
    HERE = args.receipt.resolve()
    manifest = read(HERE / 'artifact-sha256.json')
    # The capture review is written after the run, outside the manifest; it binds
    # itself to the captures by their hashes.
    unlisted = {'artifact-sha256.json'} | ({'capture-review.json'} - set(manifest))
    actual = {p.relative_to(HERE).as_posix() for p in HERE.rglob('*')
              if p.is_file() and p.relative_to(HERE).as_posix() not in unlisted}
    assert set(manifest) == actual, 'packet contains unlisted or missing files'
    for name, expected in manifest.items():
        path = (HERE / name).resolve()
        assert path.is_relative_to(HERE) and sha(path) == expected, name
    evidence = HERE / 'accepted'
    q = read(evidence / 'qualification.json')
    if q.get('mode') == 'baseline':
        return verify_baseline(HERE, q, args)
    mode = q.get('mode', 'relay' if (evidence/'relay-server.json').exists() else 'direct')
    assert q['result'] == 'ok' and q['all_owned_processes_stopped']
    assert q['custodian_launches'] == 4
    machines = q['machines']
    assert machines['owner']['machine_id'] != machines['guest']['machine_id']
    # The owner may be Linux or macOS on any architecture; the receipt records which (D130).
    assert machines['owner']['operating_system'] in {'Darwin', 'Linux'}
    assert machines['guest']['architecture'] == 'x86_64'
    assert machines['guest']['operating_system'] == 'Linux'
    assert machines['guest']['desktop']['Type'] == 'wayland'
    assert machines['guest']['desktop']['Active'] == 'yes'
    assert machines['guest']['desktop']['Remote'] == 'no'
    graphics = read(evidence / 'guest-live-graphics.json')
    assert graphics['pid'] == int(read(evidence / 'guest-live.json')['final']['pid'])
    verify_graphics(graphics, machines['guest'])
    inputs = {role: read(HERE / 'build' / role / 'build-inputs.json') for role in ['owner', 'guest']}
    assert inputs['owner']['source_commit'] == inputs['guest']['source_commit']
    owner_files, guest_files = inputs['owner']['files'], inputs['guest']['files']
    assert all(owner_files[n] == guest_files[n] for n in owner_files.keys() & guest_files.keys())
    assert inputs['owner']['harness'] == inputs['guest']['harness']
    verify_sources(owner_files | guest_files | inputs['owner']['harness'], args.commit)
    harness_prefix = next(prefix for prefix in ['support/graphshell-coop/', 'ports/graphshell/docs/receipts/coop_relay_20261020/', 'ports/graphshell/docs/receipts/coop_mac_thinkpad_20261020/'] if prefix+'run.py' in inputs['owner']['harness'])
    assert q['worker_sha256'] == inputs['owner']['harness'][harness_prefix + 'machine.py']
    assert q['controller_sha256'] == inputs['owner']['harness'][harness_prefix + 'run.py']
    process_pairs = {(p['machine'], p['pid']) for p in q['native_processes']}
    assert len(process_pairs) == 7
    custodians = set()
    captures = {}
    capture_roles = {}
    for role in ['owner', 'guest']:
        machine = machines[role]
        source = inputs[role]
        assert machine['source_commit'] == source['source_commit']
        assert machine['lock_sha256'] == source['lock_sha256']
        assert machine['worker_sha256'] == q['worker_sha256']
        assert machine['console_present'] and not machine['console_locked']
        assert all(not p['compiled'] for p in source['external_path_packages'])
        assert all(p['head'] == p['source'].split('#')[-1] and not p['tracked_code_changes']
                   for p in source['suppliers'])
        if 'host' in source:  # Packets from before the two-repository harness carry none.
            verify_host(source, machine, args)
        expected_signing = 'ad-hoc,' if machine['operating_system'] == 'Darwin' else 'not applicable: native ELF'
        assert all(binary['signing'].startswith(expected_signing) for binary in source['binaries'].values())
        assert all(machine['binaries'][name] == binary['sha256']
                   and binary['architecture'] == machine['architecture']
                   for name, binary in source['binaries'].items())
        previous_stop = None
        for launch in [1, 2]:
            events = [json.loads(line) for line in (evidence / (role + '-resident') /
                       ('events-' + str(launch) + '.jsonl')).read_text().splitlines()]
            ready = next(e for e in events if e['event'] == 'ready')
            stopped = next(e for e in events if e['event'] == 'stopped')
            assert stopped['ok'] and stopped['reason'] == 'stop intent'
            assert ready['pid'] == stopped['pid'] and ready['at_ms'] < stopped['at_ms']
            if previous_stop is not None:
                assert previous_stop < ready['at_ms']
            previous_stop = stopped['at_ms']
            custodians.add((role, ready['pid']))
        for name in q['phases'][role]:
            result = read(evidence / (name + '.json'))
            assert result['ok'] and not result['errors'], name
            assert (role, int(result['final']['pid'])) in process_pairs
            launch = read(evidence / (name + '-launch.json'))
            assert launch['launcher_pid'] == int(result['final']['pid']), 'must own actual native PID'
            assert launch['machine']['machine_id'] == machine['machine_id']
            assert launch['machine']['binaries'] == machine['binaries']
            assert not launch['machine']['console_locked']
            for capture in result['captures']:
                path = evidence / Path(capture['path']).name
                data = path.read_bytes()
                assert data[:8] == b'\x89PNG\r\n\x1a\n'
                assert struct.unpack('>II', data[16:24]) == (capture['width'], capture['height'])
                assert capture['width'] > 200 and capture['height'] > 200
                assert path.stem not in captures
                captures[path.stem] = capture['fields']
                capture_roles[path.stem] = role
    assert len(custodians) == 4 and len(captures) == (27 if mode == 'recovery' else 22)
    cleanup = read(HERE / 'cleanup.json')
    assert cleanup['accepted_cohort'] == Path(machines['owner']['public']).parents[1].name
    assert all(not cleanup[role]['remaining'] for role in ['owner', 'guest'])
    assert q['all_owned_processes_stopped'] is (not any(cleanup[r]['remaining'] for r in ['owner', 'guest']))
    chosen, private = q['graphs']['chosen'], q['graphs']['private']
    assert chosen != private
    assert q['original_invitation_sha256'] == q['cold_invitation_sha256']

    def fields(name):
        return captures[name]

    def update(name):
        return json.loads(fields(name)['coop'])

    def state(name):
        return update(name)['data']

    active_routes = set()
    for name, role in capture_roles.items():
        if role != 'guest':
            continue
        assert fields(name)['sessions'] == '0' and fields(name)['archives'] == '0'
        data = state(name)
        if data:
            assert data['graph'] == chosen
            assert all(n['address'].startswith('https://shared.invalid/') for n in data['nodes'])
            ids = {n['id'] for n in data['nodes']}
            assert all(r['from'] in ids and r['to'] in ids for r in data['relations'])
            active_routes.update(p for p in update(name)['connection'] if p.startswith('active '))
    assert active_routes, 'no observed live carrier path'
    if mode != 'direct':
        relay = read(evidence / 'relay-server.json')
        assert relay['exit'] == 0
        ready = next(e for e in relay['events'] if e['event'] == 'ready')
        final = next(e for e in relay['events'] if e['event'] == 'final')
        stopped = next(e for e in relay['events'] if e['event'] == 'stopped')
        assert ready['pid'] == final['pid'] == stopped['pid']
        assert ready['at_ms'] < final['at_ms'] <= stopped['at_ms']
        assert machines['owner']['co_op_relay'] == ready['url']
        assert all(route == 'active relay ' + ready['url'] for route in active_routes)
        assert read(evidence / 'owner-settings.json')['coop_relay_urls'] == [ready['url']]
        assert final['bytes_sent'] > 0 and final['bytes_recv'] > 0
        assert final['packets_sent'] > 0 and final['packets_recv'] > 0
        connections = [e for e in relay['events'] if e['event'] == 'connect']
        assert len(connections) >= 5 and len({e['endpoint'] for e in connections}) == 2
        # Guest custody and relay share a clock. The controller restarts both
        # custodians before launching either cold client; do not compare host clocks.
        guest_events = [json.loads(line) for line in (evidence / 'guest-resident' /
                        'events-2.jsonl').read_text().splitlines()]
        cold_ready = next(e['at_ms'] for e in guest_events if e['event'] == 'ready')
        assert len([e for e in connections if e['at_ms'] >= cold_ready]) >= 2
        for name in q['phases']['guest']:
            constraint = read(evidence / (name + '-network.json'))
            assert constraint['native_pid'] == int(read(evidence / (name + '.json'))['final']['pid'])
            assert constraint['probe']['pid'] == constraint['native_pid']
            assert constraint['probe']['netns'] != constraint['host_netns']
            assert constraint['probe']['relay_tcp'] == 'connected'
            assert constraint['probe']['udp_send_accepted'] == len(b'namespace-relay-probe')
            assert constraint['host_udp_control_received'] and not constraint['host_received_guest_udp']
            assert '--no-udp' in constraint['pasta_command']
            stopped_proxy = read(evidence / (name + '-network-stop.json'))
            assert stopped_proxy['stopped'] and stopped_proxy['proxy_pid'] == constraint['proxy_pid']
        relay_build = read(HERE / 'build/relay/build-inputs.json')
        assert relay_build['sha256'] == relay['sha256']
        assert relay_build['source_commit'] == machines['guest']['source_commit']
        assert relay_build['architecture'] == 'x86_64'
        assert relay_build['registry_files_verified'] > 0
        # Packets from before D139 hold the relay's tracked lock among the files.
        verify_sources(relay_build['files'], args.commit)
    assert fields('owner-private-during-coop')['attached'] == private
    assert state('owner-private-during-coop') is None
    assert 'private.invalid' in fields('owner-private-during-coop')['graph-labels']
    assert state('guest-still-scoped')['graph'] == chosen
    assert len(state('guest-still-scoped')['nodes']) == 2
    assert state('owner-returned-to-chosen')['graph'] == chosen
    assert len(state('guest-push')['nodes']) == 3 and fields('guest-push')['relations'] == '1'
    assert len(state('guest-push')['relations']) == 1
    warm, cold = state('guest-save-retried' if mode == 'recovery' else 'guest-return'), state('owner-cold-retained')
    assert warm['owner'] != warm['guest']
    assert any(n['id'] == q['existing_surface'] and n['title'] == ('Guest retry after relay recovery' if mode == 'recovery' else 'Guest revised existing source')
               for n in warm['nodes'])
    assert any(a['id'] == 'personae:' + warm['guest'] and a['via'] == 'graphshell.coop'
               for a in warm['authors'])
    assert cold['graph'] == chosen and cold['owner'] == warm['owner'] and cold['guest'] == warm['guest']
    assert cold['access'] == 'revoked' and cold['epoch'] > warm['epoch']
    for key in ['nodes', 'relations', 'authors', 'revision']:
        assert cold[key] == warm[key], key
    final = state('owner-cold-edited')
    assert len(final['nodes']) == 4 and final['authors'][-1]['id'] == 'personae:' + final['owner']
    assert state('owner-revoked')['access'] == 'revoked'
    for name in ['guest-withdrawn', 'guest-cold-denied']:
        assert state(name) is None and fields(name)['nodes'] == '0' and fields(name)['selected'] == ''
        assert fields(name)['record'] == '' and fields(name)['record-data'] == ''
    assert 'Revoked' in fields('guest-cold-denied')['coop-status']
    if mode == 'recovery':
        segments = read(evidence / 'relay-server.json')['segments']
        assert len(segments) == 2
        for segment in segments:
            assert segment['exit'] == 0
            assert [e['event'] for e in segment['events']][-2:] == ['final', 'stopped']
        assert segments[0]['events'][0]['url'] == segments[1]['events'][0]['url']
        assert segments[0]['events'][0]['pid'] != segments[1]['events'][0]['pid']
        if q.get('relay_rejoin_barrier'):
            observed = read(evidence / 'relay-ready-for-reconnect.json')
            peers = {e['endpoint'] for e in segments[0]['events'] if e['event'] == 'connect'}
            assert len(peers) == 2 and observed['endpoints'] == sorted(peers)
            assert observed['relay_pid'] == segments[1]['events'][0]['pid']
            assert observed['registrations'] == {
                peer: min(e['at_ms'] for e in segments[1]['events']
                          if e['event'] == 'connect' and e['endpoint'] == peer)
                for peer in peers}
        before_loss, failed = read(evidence/'before-relay-loss.json'), read(evidence/'after-failed-save.json')
        assert before_loss == failed, 'unacknowledged offline edit changed owner storage'
        assert 'Saved' not in fields('guest-save-pending')['coop-status']
        assert 'Saved' not in fields('guest-save-failed')['coop-status']
        assert state('guest-save-failed') is None and fields('guest-save-failed')['nodes'] == '0'
        restored, retried = state('guest-relay-restored'), state('guest-save-retried')
        for key in ['graph','nodes','relations','authors','revision']:
            assert restored[key] == state('guest-return')[key], key
        assert retried['revision'] > restored['revision']
        assert len(retried['authors']) == len(restored['authors']) + 1
        assert retried['authors'][-1]['id'] == 'personae:' + retried['guest']
        assert fields('guest-save-retried')['coop-status'] == 'Saved · owner acknowledged'
    before, after = read(evidence / 'private-before.json'), read(evidence / 'private-after.json')
    for key in ['session', 'graph', 'facets']:
        assert before['graphs'][0][key] == after['graphs'][0][key], key

    def changes(record):
        return next(v['value'] for c in record['cards'] for v in c['values'] if v['label'] == 'Changes')

    assert changes(before) == changes(after)
    review = read(HERE / 'capture-review.json')
    assert {r['capture'] for r in review} == {name + '.png' for name in captures}
    assert all(r['reviewed'] and r['observation']
               and sha(evidence / r['capture']) == r['sha256'] for r in review)
    print(json.dumps({'result': 'ok', 'machines': 2, 'native_processes': 7,
                      'owner': {k: machines['owner'][k] for k in ['operating_system', 'architecture']},
                      'custodians': 4, 'captures': len(captures), 'active_routes': sorted(active_routes),
                      'source_files': len(inputs['owner']['files']), 'commit': args.commit}, indent=2))


if __name__ == '__main__':
    main()
