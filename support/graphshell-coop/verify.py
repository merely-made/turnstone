#!/usr/bin/env python3
"""Verify two native machine identities, source inputs and co-op behavior."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_sources(files, commit):
    if not commit:
        for name, expected in files.items():
            assert sha(ROOT / name) == expected, name
        return
    process = subprocess.Popen(['git', 'cat-file', '--batch'], cwd=ROOT,
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--commit')
    parser.add_argument('--receipt', type=Path, required=True)
    args = parser.parse_args()
    HERE = args.receipt.resolve()
    manifest = read(HERE / 'artifact-sha256.json')
    actual = {str(p.relative_to(HERE)) for p in HERE.rglob('*')
              if p.is_file() and p != HERE / 'artifact-sha256.json'}
    assert set(manifest) == actual, 'packet contains unlisted or missing files'
    for name, expected in manifest.items():
        path = (HERE / name).resolve()
        assert path.is_relative_to(HERE) and sha(path) == expected, name
    evidence = HERE / 'accepted'
    q = read(evidence / 'qualification.json')
    mode = q.get('mode', 'relay' if (evidence/'relay-server.json').exists() else 'direct')
    assert q['result'] == 'ok' and q['all_owned_processes_stopped']
    assert q['custodian_launches'] == 4
    machines = q['machines']
    assert machines['owner']['machine_id'] != machines['guest']['machine_id']
    assert machines['owner']['architecture'] == 'x86_64'
    assert machines['guest']['architecture'] == 'x86_64'
    assert machines['owner']['operating_system'] == 'Darwin'
    assert machines['guest']['operating_system'] == 'Linux'
    assert machines['guest']['desktop']['Type'] == 'wayland'
    assert machines['guest']['desktop']['Active'] == 'yes'
    assert machines['guest']['desktop']['Remote'] == 'no'
    graphics = read(evidence / 'guest-live-graphics.json')
    assert graphics['pid'] == int(read(evidence / 'guest-live.json')['final']['pid'])
    assert graphics['VK_DRIVER_FILES'] == machines['guest']['graphical_environment']['VK_DRIVER_FILES']
    assert graphics['render_nodes'] and all(n['vendor'] == '0x1002' for n in graphics['render_nodes'])
    assert any('libvulkan_radeon' in p for p in graphics['driver_libraries'])
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
        assert machine['lock_sha256'] == source['files']['Cargo.lock']
        assert machine['worker_sha256'] == q['worker_sha256']
        assert machine['console_present'] and not machine['console_locked']
        assert all(not p['compiled'] for p in source['external_path_packages'])
        assert all(p['head'] == p['source'].split('#')[-1] and not p['tracked_code_changes']
                   for p in source['suppliers'])
        expected_signing = 'ad-hoc,' if role == 'owner' else 'not applicable: native ELF'
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
                      'custodians': 4, 'captures': len(captures), 'active_routes': sorted(active_routes),
                      'source_files': len(inputs['owner']['files']), 'commit': args.commit}, indent=2))


if __name__ == '__main__':
    main()
