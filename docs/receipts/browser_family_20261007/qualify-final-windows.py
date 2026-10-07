"""Bind completed Windows gates to the published test repair and current source."""
import datetime
import hashlib
import json
import pathlib
import re
import subprocess
import tomllib

folder = pathlib.Path(__file__).resolve().parent
root = folder.parents[2]

def git(*args):
    return subprocess.check_output(['git', *args], cwd=root)

def sha(data):
    return hashlib.sha256(data).hexdigest()

head = git('rev-parse', 'HEAD').decode().strip()
repair = '8d1f907f4bd31b256737a27d3e63fe8cd8672efc'
assert not git('diff', '--name-only', 'HEAD').strip()
since_repair = git('diff', '--name-only', repair, head).decode().splitlines()
assert all(p.startswith(('docs/', 'design_docs/')) for p in since_repair)
old = json.loads((root / 'docs/receipts/browser_supplier_integration_20261006/source-manifest-all3-current-resource-descriptor.json').read_text())
paths = {p['path'] for p in old['files']}
paths.update(json.loads((folder / 'test-repair-production-equivalence.json').read_text())['additional_test_inputs'])
inputs = []
for name in sorted(paths):
    data = (root / name).read_bytes()
    tracked = subprocess.run(['git', 'ls-files', '--error-unmatch', name], cwd=root, capture_output=True).returncode == 0
    row = {'path': name, 'bytes': len(data), 'sha256': sha(data), 'tracked': tracked}
    if tracked:
        blob = git('rev-parse', f'{head}:{name}').decode().strip()
        filtered = subprocess.check_output(['git', 'hash-object', '--path=' + name, '--stdin'], input=data, cwd=root).decode().strip()
        assert filtered == blob, name
        row['git_object'] = blob
        row['git_attributes_filtered_match'] = True
    else:
        assert name == '.cargo/config.local.toml', name
        row['scope'] = 'Ignored inactive local config, not a tracked source input'
    inputs.append(row)
labels = ['windows-final-all3-lib-tests', 'windows-final-all3-all-targets', 'windows-final-cargo-mode', 'windows-final-cargo-tree']
gates = {}
artifacts = []
for label in labels:
    result = json.loads((folder / (label + '.result.json')).read_text())
    assert result['exit_code'] == 0, label
    gates[label] = result
    for suffix in ['.result.json', '.stdout.log', '.stderr.log']:
        p = folder / (label + suffix)
        artifacts.append({'path': p.relative_to(root).as_posix(), 'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())})
matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', (folder / (labels[0] + '.stdout.log')).read_text(encoding='utf-8'))
totals = dict(zip(['passed', 'failed', 'ignored'], [sum(int(row[i]) for row in matches) for i in range(3)]))
assert totals == {'passed': 681, 'failed': 0, 'ignored': 9}, totals
lock_bytes = (root / 'Cargo.lock').read_bytes()
lock = tomllib.loads(lock_bytes.decode())
families = {}
for repo in ['mere', 'genet', 'knot-editor', 'woodshed', 'wgpu-graft', 'wgpu-scry', 'wgpu-weld']:
    packages = [p for p in lock['package'] if p.get('source', '').startswith('git+https://github.com/merely-made/' + repo + '.git?')]
    sources = {p['source'] for p in packages}
    assert len(sources) == 1, (repo, sources)
    families[repo] = {'packages': len(packages), 'source': sources.pop()}
record = {'recorded_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'compiled_checkout_head': head, 'published_test_source': repair, 'changes_since_test_source': since_repair, 'tracked_clean_after_gates': True, 'untracked_files_excluded': True, 'inputs_after_gates': inputs, 'gates': gates, 'lib_tests': totals, 'cargo_lock_sha256': sha(lock_bytes), 'locked_supplier_families': families, 'artifacts': artifacts, 'source_identity_scope': 'Tracked clean guards surrounded the sequential run. This record independently checks post-run inputs against checkout Git objects; no separate before/after byte inventory was captured for each Windows final command.', 'native_scope': 'Original native executable/build remains bound to abb349cf production source and its immutable archive. Published 8d1f907 changes tests/fixtures only, proven separately; no old native binary is relabelled as rebuilt.'}
output = folder / 'windows-final-source-qualification.json'
with output.open('x', encoding='utf-8', newline='\n') as stream:
    json.dump(record, stream, indent=2)
    stream.write('\n')
print(json.dumps({'source': head, 'lib_tests': totals, 'inputs': len(inputs), 'supplier_families': len(families), 'gates': len(gates)}))
