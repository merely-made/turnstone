#!/usr/bin/env python3
"""Verify place IDs, authority, exact shared scope and real cold-return changes."""
import argparse
import hashlib
import json
import re
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def addresses(value):
    if isinstance(value, str):
        return {value} if value.startswith('http://127.0.0.1:') else set()
    if isinstance(value, dict):
        return set().union(*(addresses(v) for v in value.values()))
    if isinstance(value, list):
        return set().union(*(addresses(v) for v in value))
    return set()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--inspect-exited', action='store_true')
    parser.add_argument('--commit')
    options = parser.parse_args()
    output = options.output.resolve()
    inputs = read(output / 'inputs.json')
    for name, digest in inputs['source_sha256'].items():
        data = subprocess.check_output(['git', 'show', options.commit + ':' + name], cwd=ROOT) if options.commit else (ROOT / name).read_bytes()
        assert hashlib.sha256(data).hexdigest() == digest, name + ' source drift'
    complete = read(output / 'native-complete.json')
    assert complete['result'] == 'pass' and complete['origin_stopped_before_cold_reopen']
    assert set(complete['runs']) == {'founder', 'joiner', 'joiner-return', 'reader', 'cold-founder', 'cold-joiner'}
    native_pids = []
    presented_frames = captures = 0
    for phase, process in complete['runs'].items():
        assert process['success'] and process['launcher_exit'] == 0 and not process['timeout'], phase
        assert process['binary_sha256'] == inputs['binary_sha256']
        assert process['activation']['requested'] and process['activation']['exit_code'] == 0
        native_pids.extend(process['activation']['owned_native_pids'])
        done = (output / phase / 'scenario.done').read_text()
        assert done.startswith('RESULT ok\n')
        presentation = re.search(r'presentation frames=(\d+) captures=(\d+) blank=(\d+)', done)
        assert presentation and int(presentation[1]) > 0 and int(presentation[3]) == 0, phase
        assert int(presentation[2]) == process['captures'] > 0, phase
        presented_frames += int(presentation[1])
        captures += int(presentation[2])
    assert len(native_pids) == len(set(native_pids)) == 6
    assert captures == 15

    def record(phase, name):
        value = read(output / phase / (name + '.json'))
        assert value['place'] is not None
        return value

    initial = record('founder', '01_founder_listening')['place']
    before_stop = record('joiner', '07_joiner_before_stop')['place']
    absent = record('founder', '04_absent_authored')['place']
    returned = record('joiner-return', '09_joiner_caught_up')['place']
    final = {
        'founder': record('founder', '05_founder_final'),
        'joiner': record('joiner-return', '10_joiner_final'),
        'reader': record('reader', '12_reader_final'),
    }
    cold_retained = {
        'founder': record('cold-founder', '13_founder_cold_retained'),
        'joiner': record('cold-joiner', '15_joiner_cold_retained'),
    }
    cold = {
        'founder': record('cold-founder', '14_founder_cold_converged'),
        'joiner': record('cold-joiner', '16_joiner_cold_converged'),
    }
    ids = {key: initial[key] for key in ['moot', 'root', 'chat', 'default_channel']}
    assert len({initial[key] for key in ['moot', 'root', 'chat']}) == 3
    assert initial['members'] == 1 and initial['graph_nodes'] == 0 and initial['chat_messages'] == 0
    assert [before_stop['chat_messages'], absent['chat_messages'], returned['chat_messages']] == [2, 3, 4]
    assert len({before_stop['chat_digest'], absent['chat_digest'], returned['chat_digest']}) == 3
    roots = {role: value['place']['personae_root'] for role, value in final.items()}
    first_return = record('joiner-return', '08_joiner_cold_retained')['place']
    assert first_return['personae_root'] == roots['joiner'] == returned['personae_root']
    assert first_return['graph_digest'] == before_stop['graph_digest']
    assert first_return['chat_digest'] == before_stop['chat_digest']
    assert first_return['chat_messages'] == 2 and first_return['lanes'] == []
    assert first_return['local_rendezvous'] == []
    assert len(set(roots.values())) == 3
    graph = final['founder']['place']['graph_digest']
    chat = final['founder']['place']['chat_digest']
    assert graph != initial['graph_digest']
    for role, value in final.items():
        place = value['place']
        assert all(place[key] == expected for key, expected in ids.items())
        assert place['members'] == 3 and place['graph_nodes'] == 1 and place['chat_messages'] == 5
        assert place['graph_digest'] == graph and place['chat_digest'] == chat
    for role, value in cold_retained.items():
        place = value['place']
        assert place['personae_root'] == roots[role]
        assert all(place[key] == expected for key, expected in ids.items())
        assert place['graph_nodes'] == 1 and place['chat_messages'] == 5
        assert place['graph_digest'] == graph and place['chat_digest'] == chat
        assert place['local_rendezvous'] == [] and place['lanes'] == []
        assert any('no local sync lanes opened' in line for line in value['status'])
    foreign_refusal = record('cold-joiner', '15a_foreign_contact_refused')
    assert foreign_refusal['place'] == cold_retained['joiner']['place']
    assert any('contact card belongs to another place' in line
               for line in (output / 'cold-joiner' / 'native.log').read_text().splitlines())
    for role, value in cold.items():
        place = value['place']
        assert place['personae_root'] == roots[role]
        assert all(place[key] == expected for key, expected in ids.items())
        assert place['graph_nodes'] == 1 and place['chat_messages'] == 7
        assert place['graph_digest'] == graph and place['chat_digest'] != chat
    assert cold['founder']['place']['chat_digest'] == cold['joiner']['place']['chat_digest']
    refusal = record('reader', '11_reader_refused')
    assert refusal['place']['graph_nodes'] == 1
    assert any('Shared graph permission: not effective' in line for line in refusal['status'])
    assert any('Message permission: not effective' in line for line in refusal['status'])
    assert read(output / 'origin-stopped.json')['tcp_refused']

    for role in ['founder', 'joiner', 'reader']:
        starts = [read(p) for p in sorted((output / ('resident-' + role)).glob('status-*.json'))]
        assert len(starts) == (1 if role == 'reader' else 2)
        assert all(s['ready'] and s['lock'] == 'unlocked' and not s['installed'] for s in starts)
        assert len({s['pid'] for s in starts}) == len(starts)

    origin = read(output / 'origin.json')['base_url']
    shared = origin + '/shared.html'
    private_urls = {role: origin + '/' + role + '-private.html' for role in ['founder', 'reader']}
    caches = output / 'exited-cache'
    if options.inspect_exited:
        private_root = Path(inputs['private_runtime'])
        for role in ['founder', 'joiner', 'reader']:
            bindings = list((private_root / role / 'turnstone').rglob('place.json'))
            assert len(bindings) == 1, role + ' persisted place binding'
            binding = bindings[0]
            assert all(read(binding)[key] == expected for key, expected in ids.items())
            destination = caches / role
            destination.mkdir(parents=True, exist_ok=False)
            shutil.copy2(binding, destination / 'place.json')
            graphs = list(binding.parent.rglob('graph.json'))
            assert graphs, role + ' persisted graph cache'
            for index, path in enumerate(graphs):
                shutil.copy2(path, destination / ('graph-' + str(index) + '.json'))
    if options.inspect_exited:
        private_root = Path(inputs['private_runtime'])
        binding_path = next((private_root / 'joiner' / 'turnstone').rglob('place.json'))
        descriptor_path = binding_path.parent / 'place-rendezvous.json'
        descriptor = read(descriptor_path)
        fresh_card_path = private_root / 'exchange' / 'cold.place-card.json'
        fresh = read(fresh_card_path)
        original = read(private_root / 'exchange' / 'writer.place-card.json')
        assert descriptor['rendezvous'] == fresh['rendezvous'] != original['rendezvous']
        assert set(descriptor) == {'version', 'moot', 'root', 'chat', 'not_after_ms', 'rendezvous'}
        assert all(descriptor[key] == expected for key, expected in ids.items() if key != 'default_channel')
        contact_proof = {
            'result': 'pass', 'retained_binding_unchanged': True,
            'persisted_current_contact': True, 'different_from_initial_contact': True,
            'no_admission_or_authority_fields': True,
            'descriptor_sha256': sha(descriptor_path), 'current_card_sha256': sha(fresh_card_path),
            'not_after_ms': descriptor['not_after_ms'],
        }
        (output / 'contact-refresh-persistence.json').write_text(json.dumps(contact_proof, indent=2) + '\n')
    assert read(output / 'contact-refresh-persistence.json')['persisted_current_contact']
    scope = {}
    for role in ['founder', 'joiner', 'reader']:
        urls = set().union(*(addresses(read(p)) for p in (caches / role).glob('graph-*.json')))
        assert shared in urls, role + ' selected address missing'
        for owner, private_url in private_urls.items():
            assert (private_url in urls) == (owner == role), (role, owner, 'unshared address scope')
        scope[role] = sorted(urls)
    result = {
        'result': 'pass', 'commit': options.commit, 'place_ids': ids, 'personae_roots': roots,
        'initial_messages': 2, 'absent_messages': 3, 'converged_messages': 5,
        'cold_retained_messages': 5, 'cold_reconnected_messages': 7,
        'shared_graph_digest': graph, 'chat_digest_before_cold': chat,
        'chat_digest_after_cold': cold['founder']['place']['chat_digest'],
        'exited_cache_address_scope': scope, 'origin_gone_before_cold_reopen': True,
        'explicit_current_contact_refresh': True, 'foreign_contact_refused_without_scope_change': True,
        'independent_resident_restarts': True, 'reader_write_refused': True,
        'native_processes': 6, 'native_pids': native_pids,
        'presented_frames': presented_frames, 'nonblank_captures': captures,
        'scope': 'Same-host native place admission, selected address sharing, retained reopening and live catch-up',
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
