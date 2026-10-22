#!/usr/bin/env python3
"""Verify this bounded format/local-note receipt and its committed source cut."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    return json.loads(path.read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit')
    options = parser.parse_args()
    for name, digest in read(HERE / 'artifact-sha256.json').items():
        assert sha((HERE / name).read_bytes()) == digest, name
    inputs = read(HERE / 'native/inputs.json')
    for name, digest in inputs['source_sha256'].items():
        data = subprocess.check_output(['git', 'show', options.commit + ':' + name], cwd=ROOT) if options.commit else (ROOT / name).read_bytes()
        assert sha(data) == digest, name + ' source drift'
    for name, digest in inputs['harness_sha256'].items():
        assert sha((HERE / name).read_bytes()) == digest, name + ' harness drift'
    assert sha((HERE / 'run_macos_scenario.py').read_bytes()) == inputs['launcher_sha256']
    native = read(HERE / 'native/qualification.json')
    assert native['result'] == 'pass' and native['unselected_note_unchanged']
    assert [run['phase'] for run in native['runs']] == ['create', 'seed', 'reopen']
    for run in native['runs']:
        process = run['process']
        assert run['launcher_exit'] == 0 and process['success'] and not process['timeout']
        assert process['binary_sha256'] == inputs['binary_sha256']
        assert process['activation']['requested'] and process['activation']['exit_code'] == 0
    identity = None
    for phase, name in [('seed', '06_saved_passage'), ('seed', '09_follow_exact_citation'), ('seed', '11_old_A1_after_A2'), ('reopen', '13_cold_exact_A1'), ('reopen', '15_cold_old_A1_after_A2')]:
        observation = read(HERE / 'native' / phase / (name + '.saved-sources.json'))
        readers = [r for r in observation['readers'] if r['phase'] == 'Ready' and r.get('content', {}).get('text') == "The Pelican observatory's access word is café."]
        assert readers, (phase, name)
        if identity is None:
            identity = readers[-1]['target']
        reader = next(r for r in readers if r['target'] == identity)
        selector = reader['target']['selector']
        assert reader['content']['text'][selector['start']:selector['end']] == 'café'
    for phase in ['seed', 'reopen']:
        report = read(HERE / 'native' / (phase + '-store-inspection.json'))
        assert report['result'] == 'pass' and report['independent_after_app_exit']
        citation = report['citations'][0]
        assert citation['target'] == identity and citation['quote'] == 'café'
        assert citation['verified_against_turnstone_capture'] and citation['portable_and_transport_hashes_verified'] and citation['corrupt_bytes_refused']
    for phase in [0, 1]:
        stop = read(HERE / 'native' / ('origin-stop-' + str(phase) + '.json'))
        assert stop['exited'] and stop['tcp_refused']
    assert (HERE / 'native/requests-at-stop-1.json').read_bytes() == (HERE / 'native/final-requests.json').read_bytes()
    review = read(HERE / 'capture-review.json')
    captures = sorted(str(p.relative_to(HERE)) for p in (HERE / 'native').glob('*/*.png'))
    assert sorted(review['captures']) == captures and len(captures) == native['captures']
    qualification = read(HERE / 'qualification.json')
    assert qualification['result'] == 'pass' and qualification['selected_sharing'] == 'not implemented or qualified'
    print(json.dumps({'result': 'pass', 'commit': options.commit, 'reviewed_captures': len(captures), 'scope': qualification['scope'], 'selected_sharing': qualification['selected_sharing']}, indent=2))


if __name__ == '__main__':
    main()
