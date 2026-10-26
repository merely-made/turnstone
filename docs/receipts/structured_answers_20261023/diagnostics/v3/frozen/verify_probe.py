#!/usr/bin/env python3
"""Gate headed acceptance on every frozen real-model answer, never libtest alone."""
import argparse
import hashlib
import json
import traceback
from pathlib import Path
from verify_native import verify_answer

HERE = Path(__file__).resolve().parent

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('probe', type=Path)
    args = parser.parse_args()
    cases_path = HERE / 'cases.json'
    cases = json.loads(cases_path.read_text())
    freeze = json.loads((HERE / 'case-freeze.json').read_text())
    assert hashlib.sha256(cases_path.read_bytes()).hexdigest() == freeze['cases_sha256']
    checks = {}
    for case in cases['asks']:
        path = args.probe / (case['id'] + '.json')
        partial = args.probe / (case['id'] + '.partial.json')
        if not path.exists():
            entry = {'result': 'incomplete' if partial.exists() else 'not_run'}
            if partial.exists():
                run = json.loads(partial.read_text())
                entry.update(state=run['state'], tokens=run['tokens'], raw_output=run['text'])
                try:
                    json.loads(run['text'])
                    entry['partial_json'] = 'parseable'
                except json.JSONDecodeError as error:
                    entry['partial_json'] = str(error)
            checks[case['id']] = entry
            continue
        run = json.loads(path.read_text())
        timeout = args.probe / (case['id'] + '.timeout.json')
        if timeout.exists() or run.get('state') != 'finished':
            checks[case['id']] = {'result': 'incomplete', 'state': run.get('state'), 'tokens': run.get('tokens'), 'timeout': json.loads(timeout.read_text()) if timeout.exists() else None}
            continue
        try:
            detail = verify_answer(run, case, cases)
            checks[case['id']] = {'result': 'pass', **detail}
        except (AssertionError, KeyError, TypeError, ValueError) as error:
            checks[case['id']] = {'result': 'fail', 'error': str(error) or ('Check failed at ' + str(traceback.extract_tb(error.__traceback__)[-1].lineno) + ' in verify_native.py'), 'state': run.get('state')}
    result = {
        'result': 'pass' if all(v['result'] == 'pass' for v in checks.values()) else 'gate_not_passed',
        'cases_sha256': freeze['cases_sha256'],
        'cases': checks,
        'headed_allowed': all(v['result'] == 'pass' for v in checks.values()),
        'boundary': 'Partial or absent output is not a completed semantic measurement. All four completed frozen answers must pass before headed acceptance.'
    }
    (args.probe / 'gate.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result['headed_allowed'] else 1)

if __name__ == '__main__':
    main()
