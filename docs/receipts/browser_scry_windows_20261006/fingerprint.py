"""Freeze or verify the exact Turnstone source and native executable."""
import argparse, datetime, hashlib, json, pathlib, subprocess
r = pathlib.Path(__file__).resolve().parent
root = r.parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--verify', action='store_true')
parser.add_argument('--manifest', default='final-source-manifest.json')
args = parser.parse_args()
def digest(p):
    with p.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()
exe = pathlib.Path('C:/t/cargo-targets/turnstone/debug/turnstone.exe')
manifest = r / args.manifest
if args.verify:
    record = json.loads(manifest.read_text(encoding='utf-8'))
    changes = [v['path'] for v in record['files'] if digest(root / v['path']) != v['sha256']]
    assets = r / 'fixture-assets-manifest.json'
    if assets.exists():
        asset_record = json.loads(assets.read_text(encoding='utf-8'))
        changes.extend(v['path'] for v in asset_record['files'] if digest(root / v['path']) != v['sha256'])
    if digest(exe) != record['exe_sha256']: changes.append(str(exe))
    if changes: raise SystemExit('Changed qualification inputs: ' + ', '.join(changes))
    print('All source/scenario and executable fingerprints unchanged')
else:
    if manifest.exists(): raise SystemExit('Refusing to overwrite existing fingerprints')
    paths = [root/'Cargo.toml', root/'Cargo.lock', *sorted((root/'src').rglob('*.rs')),
        root/'scenarios/browser_scry_windows.scn',
        root/'scenarios/browser_weld_direct_windows.scn',
        root/'scenarios/browser_weld_permission_direct_windows.scn',
        root/'scenarios/fixtures/browser_scry/restart_verify.scn',
        root/'scenarios/fixtures/browser_decisions_server.ps1',
        *sorted((root/'scenarios/fixtures/browser_scry').glob('*.html')),
        *sorted((root/'scenarios/fixtures/browser_scry').glob('*.css')),
        *sorted((root/'scenarios/fixtures/browser_scry').glob('*.js'))]
    record = dict(base_head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        recorded_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        features=['scry','weld'],
        files=[dict(path=p.relative_to(root).as_posix(),sha256=digest(p)) for p in paths],
        exe_sha256=digest(exe),
        supplier_revisions=dict(scrying='39818a7eddb8bec33f7c61ed144da226bc0a1d04',
            welding='4784d07c4064195c33136b9b91c8231913f08e06',
            mere='bd5912fbbb8f468defc3bbeee7eac5a4f7d2b2f3',
            grafting='403a30c2fab39c573d1eebb57a0995e2c3347ff1'))
    manifest.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    print(record['exe_sha256'])
