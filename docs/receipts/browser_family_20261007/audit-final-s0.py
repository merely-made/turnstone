"""Independently verify the closed platform packets and published source inputs."""
import datetime
import hashlib
import json
import pathlib
import re
import subprocess

folder = pathlib.Path(__file__).resolve().parent
root = folder.parents[2]
repair = '8d1f907f4bd31b256737a27d3e63fe8cd8672efc'

def sha(data):
    return hashlib.sha256(data).hexdigest()

def read(name):
    return json.loads((folder / name).read_text(encoding='utf-8'))

def checked_path(base, name):
    p = (base / name).resolve(strict=True)
    assert p.is_relative_to(root.resolve()), name
    return p

indices = ['linux-current-hashes.json', 'linux-sky-diagnostic-hashes.json', 'linux-final-hashes.json', 'linux-final-fd-hashes.json']
checked = []
for name in indices:
    record = read(name)
    for row in record['artifacts']:
        p = checked_path(folder, row['path'])
        assert p.stat().st_size == row['bytes'] and sha(p.read_bytes()) == row['sha256'], p
    checked.append({'path': name, 'sha256': sha((folder / name).read_bytes()), 'verified_artifacts': len(record['artifacts'])})
win = read('windows-final-source-qualification.json')
for row in win['artifacts']:
    p = checked_path(root, row['path'])
    assert p.stat().st_size == row['bytes'] and sha(p.read_bytes()) == row['sha256'], p
linux = read('linux-final-fd-summary.json')
assert linux['source_sha'] == repair and linux['final_source_sha'] == repair
assert linux['all_requested_gates_completed'] and linux['all_completed_exit_zero']
assert not linux['input_hash_changes'] and not linux['tracked_diff_after']
assert len(linux['gates']) == 5 and all(g['actual_exit_code'] == 0 for g in linux['gates'])
text = (folder / 'linux-final-fd-lib.stdout.log').read_text(encoding='utf-8')
matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)
totals = dict(zip(['passed', 'failed', 'ignored'], [sum(int(row[i]) for row in matches) for i in range(3)]))
assert totals == {'passed': 650, 'failed': 0, 'ignored': 9}, totals
metadata_bytes = (folder / 'linux-final-fd-metadata.stdout.log').read_bytes()
provenance = read('linux-final-fd-family-provenance.json')
assert sha(metadata_bytes) == provenance['metadata_sha256']
metadata = json.loads(metadata_bytes)
families = {}
for repo, expected in win['locked_supplier_families'].items():
    packages = [p for p in metadata['packages'] if (p.get('source') or '').startswith('git+https://github.com/merely-made/' + repo + '.git?')]
    sources = {p['source'] for p in packages}
    if packages:
        assert sources == {expected['source']}, repo
    families[repo] = {'metadata_packages': len(packages), 'source': next(iter(sources)) if sources else None}
linux_inputs = read('linux-final-fd-inputs.json')
inventory = {p['path']: p for p in linux_inputs['tracked_file_inventory']}
core_verified = []
for item in win['inputs_after_gates']:
    name = item['path']
    if name.startswith('docs/') or not item['tracked']:
        continue
    if name not in inventory:
        raise AssertionError('Missing Linux source: ' + name)
    blob = subprocess.check_output(['git', 'show', repair + ':' + name], cwd=root)
    assert len(blob) == inventory[name]['bytes'] and sha(blob) == inventory[name]['sha256'], name
    core_verified.append(name)
assert win['cargo_lock_sha256'] == sha((folder / 'linux-final-fd-Cargo.lock').read_bytes())
native = folder / 'artifact-manifest-all3-current-resource-descriptor.json'
proof = folder / 'test-repair-production-equivalence.json'
record = {'recorded_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'published_source': repair, 'windows_compiled_checkout': win['compiled_checkout_head'], 'windows_tests': win['lib_tests'], 'linux_tests': totals, 'linux_packet_indices': checked, 'windows_finished_artifacts_verified': len(win['artifacts']), 'linux_tracked_inventory_count': len(inventory), 'linux_core_inputs_independently_matched_to_published_git_bytes': core_verified, 'locked_supplier_sources': families, 'native_immutable_manifest_sha256': sha(native.read_bytes()), 'production_equivalence_proof_sha256': sha(proof.read_bytes()), 'scope': 'Coordinated source S0 qualification. Linux default features; Windows combined scry,weld,servo. Native qualification remains attached to the original executable/build; default Servo synchronization and foreign accessibility remain release gates.'}
with (folder / 'final-s0-audit.json').open('x', encoding='utf-8', newline='\n') as out:
    json.dump(record, out, indent=2)
    out.write('\n')
print(json.dumps({'linux_artifacts_verified': sum(r['verified_artifacts'] for r in checked), 'windows_artifacts_verified': len(win['artifacts']), 'linux_core_inputs_verified': len(core_verified), 'windows_tests': win['lib_tests'], 'linux_tests': totals}))
