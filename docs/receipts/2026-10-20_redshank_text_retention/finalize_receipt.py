from pathlib import Path
import hashlib,json,re,shutil
OUT=Path(__file__).parent; ROOT=OUT/'runtime'
WORK=Path('/Users/markik/Code/worktrees/turnstone-knot-drag-20261020')
PACKET=WORK/'docs/receipts/2026-10-20_redshank_text_retention'
assert (ROOT/'fixture-marker').read_text()=='redshank-text-retention-20261020\n'
phases=['producer','producer-cold','receiver','receiver-cold']
results={name:json.loads((OUT/name/'process.json').read_text()) for name in phases}
assert all(p['success'] and not p['timeout'] for p in results.values())
assert len({p['binary_sha256'] for p in results.values()})==1
for name in phases:assert (OUT/name/'scenario.done').read_text().startswith('RESULT ok\n')
assert hashlib.sha256((OUT/'target/debug/turnstone').read_bytes()).hexdigest()==results['producer']['binary_sha256']
selected=json.loads((OUT/'selected-text-reference.json').read_text())
cold=json.loads((ROOT/'cold-text.mereclip').read_text())
assert cold==selected
owner=json.loads((OUT/'expected-note-export.json').read_text())
annotation=owner['annotation'];anchor=cold['clip']['anchors'][0]
assert anchor['selector']==annotation['target']
assert anchor['evidence']['body']==annotation['body']
assert anchor['source']==owner['source'] and anchor['units']=='milliseconds'
assert cold['clip']['clip_id']==annotation['id']=='embedded-text-selected'
assert cold['clip']['source']['entity']=='mere://redshank/note/embedded-text-selected'
assert len(cold['payloads'])==len(cold['clip']['parts'])==len(cold['clip']['anchors'])==1
body=annotation['body']['plain_text'].encode();assert bytes(cold['payloads'][0]['bytes'])==body
assert cold['clip']['parts'][0]['media_type']=='text/plain;charset=utf-8'
assert anchor['selector']['offset_ms']==1250 and anchor['selector']['end_offset_ms']==4500 and anchor['selector']['pressed_offset_ms']==1400
for word in ['PRIVATE CONTROL','UNSELECTED CONTROL']:
    assert word not in json.dumps(cold,ensure_ascii=False)
    assert word.encode() not in bytes(cold['payloads'][0]['bytes'])
removal=json.loads((OUT/'source-removal.json').read_text())
assert not Path(removal['removed_source_session']).exists() and not Path(removal['removed_recording']).exists()
assert not (ROOT/'selected-text.mereclip').exists() and not (ROOT/'onward-text.mereclip').exists()
privacy={}
for profile in ['producer','receiver']:
    files=[p for p in (ROOT/profile/'retained-clips').rglob('*') if p.is_file()];assert files
    hits=[str(p.relative_to(ROOT/profile)) for p in files if body in p.read_bytes() or b'Saved from the embedded listener' in p.read_bytes() or b'PRIVATE CONTROL' in p.read_bytes() or b'UNSELECTED CONTROL' in p.read_bytes()]
    wav=[str(p.relative_to(ROOT/profile)) for p in (ROOT/profile).rglob('*.wav')]
    redshank=[str(p.relative_to(ROOT/profile)) for p in (ROOT/profile).glob('sessions/*/redshank/state-*.json')]
    assert not hits and not wav and not redshank
    privacy[profile]={'store_files':len(files),'clear_selected_or_control_note_hits':hits,'wav_cache_files':wav,'owner_model_generation_files':redshank}
sources=json.loads((OUT/'source-inputs.json').read_text())
assert all(hashlib.sha256((WORK/p).read_bytes()).hexdigest()==h for p,h in sources['files'].items())
checks={}
for name,filename in [('redshank_host','host-tests.log'),('retained_clip','retained-clip-tests.log'),('producer_retention','producer-retention-tests.log'),('redshank_app','redshank-arms-tests.log'),('redshank_surface','redshank-surface-tests.log')]:
    m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',(OUT/filename).read_text());assert m and int(m[1])>0
    checks[name]={'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3])}
assert sum(v['passed'] for v in checks.values())==36
assert '1 passed; 0 failed' in (OUT/'fixture.log').read_text()
restarts=[json.loads((OUT/name).read_text()) for name in ['custodian-first-cold-process.json','custodian-receiver-cold-process.json']]
assert all(p['same_persisted_fixture_wallet'] and p['pid']!=p['previous_pid'] for p in restarts)
PACKET.mkdir(parents=True,exist_ok=True)
for name in phases:
    dest=PACKET/name;dest.mkdir(exist_ok=True)
    for p in (OUT/name).iterdir():
        if p.suffix=='.png' or p.name in ['process.json','scenario.done','native.log']:
            shutil.copy2(p,dest/p.name)
    shutil.copy2(OUT/(name+'.scn'),dest/'scenario.scn')
for name in ['source-inputs.json','test-artifact.json','host-tests.log','retained-clip-tests.log','producer-retention-tests.log','redshank-arms-tests.log','redshank-surface-tests.log','fixture.log','native-build.log','content-reference.json','selected-text-reference.json','expected-note-export.json','seed-model.json','source-removal.json','receiver-cold-prerequisites.json','custodian-first-cold-process.json','custodian-receiver-cold-process.json','run_native.py','verify_and_prepare.py','restart_custodian.py','finalize_receipt.py']:
    shutil.copy2(OUT/name,PACKET/name)
for log in PACKET.rglob('*.log'):log.write_text(log.read_text().rstrip()+'\n')
proof={
 'date':'2026-10-20','passed':True,
 'scope':'Embedded Redshank selected text-note retention in shipping macOS Turnstone, source-absent cold reopen, onward export and independent receive plus cold re-export',
 'source_base':sources['base_commit'],'sources':sources,'phases':results,'checks':checks,
 'explicit_fixture_setup_tests':1,'reviewed_captures':8,
 'fixture':{'owner':'Turnstone embedded RedshankHost and JsonDirectoryStore','selected_id':annotation['id'],'selected_initial_privacy':'private','shareability_command':'RedshankHost::command(SetNotePrivacy)','note_authoring':'Typed isolated owner-model fixture; not headed authoring','controls':['private text note','unselected shareable text note'],'recording':'14 second synthetic mono 48 kHz PCM16 silence; never played'},
 'selected_text':annotation['body']['plain_text'],'selected_utf8_bytes':len(body),
 'native_target':annotation['target'],'content_reference':json.loads((OUT/'content-reference.json').read_text())['text'],
 'private_retention_offer_unavailable':True,'only_selected_text_payload_exported':True,
 'cold_export_package_object_equals_owner_selected_original':True,'onward_package_object_equal_before_receiver':True,
 'original_source_session_and_recording_removed':True,'all_transfer_inputs_removed_before_independent_cold_reopen':True,
 'raw_store_privacy_scan':privacy,'custodian_restarts':2,'offline_acquisition_attempts':0,
 'limits':['Synthetic wallet-backed custody IPC fixture; no personal vault migration','Typed owner-model fixture contains the notes; full Redshank note authoring/privacy menu is not embedded or headed-qualified','Privacy host command is focused-tested for episode scoping, persistence, unchanged body/target and export revocation','Native scenario selects projected note addresses and invokes the ordinary retention/export actions; export uses an explicit new-file destination and does not automate the OS save dialog','Text-only qualification; no recording, microphone, acoustic or renewed audio/playback claim','36 focused tests plus one explicit fixture setup; two process helpers ignored in ordinary checks; not a full workspace gate']}
assert sum(p['captures'] for p in results.values())==8
(PACKET/'qualification.json').write_text(json.dumps(proof,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'passed':True,'phases':len(phases),'reviewed_captures':8,'checks':checks},indent=2))
