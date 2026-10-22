from pathlib import Path
import hashlib,json,re,shutil,subprocess
OUT=Path(__file__).parent
ROOT=OUT/'runtime'
WORK=Path('/Users/markik/Code/worktrees/turnstone-knot-drag-20261020')
PACKET=WORK/'docs/receipts/2026-10-20_knot_drag_retention'
phases=['final-producer','final-receiver','final-cold-receiver']
assert (ROOT/'fixture-marker').read_text()=='eidetic-headed-20261020\n'
results={p:json.loads((OUT/p/'process.json').read_text()) for p in phases}
assert all(p['success'] and not p['timeout'] for p in results.values())
assert len({p['binary_sha256'] for p in results.values()})==1
for p in phases: assert (OUT/p/'scenario.done').read_text().startswith('RESULT ok\n')
knot=json.loads((ROOT/'cold-knot.mereclip').read_text());voice=json.loads((ROOT/'cold-voice.mereclip').read_text())
assert knot==json.loads((OUT/'selected-knot-reference.json').read_text())
assert voice==json.loads((OUT/'selected-voice-reference.json').read_text())
assert len(knot['payloads'])==1 and bytes(knot['payloads'][0]['bytes'])=='café'.encode()
assert knot['clip']['anchors'][0]['selector']['byte_span']=={'start':16,'end':21}
assert voice['clip']['anchors'][0]['selector']['pressed_offset_ms']==1400
assert not (ROOT/'final-producer/notes/selected.djot').exists()
assert not (ROOT/'selected-knot.mereclip').exists() and not (ROOT/'voice-fixture.mereclip').exists()
source='private prefix;\ncafé\nprivate suffix'.encode();body=bytes(voice['payloads'][0]['bytes'])
privacy={}
for profile in ['final-producer','final-receiver']:
 files=[p for p in (ROOT/profile/'retained-clips').rglob('*') if p.is_file()];assert files
 hits=[str(p.relative_to(ROOT/profile)) for p in files if source in p.read_bytes() or body in p.read_bytes()]
 wav=[str(p.relative_to(ROOT/profile)) for p in (ROOT/profile).rglob('*.wav')]
 assert not hits and not wav
 privacy[profile]={'store_files':len(files),'clear_full_source_or_voice_hits':hits,'wav_cache_files':wav}
sources=json.loads((OUT/'source-inputs.json').read_text())
assert all(hashlib.sha256((WORK/p).read_bytes()).hexdigest()==h for p,h in sources['files'].items())
checks={}
for name,filename in [('knot_authoring','final-knot-tests.log'),('retained_clip','final-retained-tests.log'),('producer_retention','final-producer-tests.log'),('window_input','final-window-input-tests.log')]:
 text=(OUT/filename).read_text();m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text);assert m and int(m[1])>0,filename
 checks[name]={'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3])}
PACKET.mkdir(parents=True,exist_ok=True)
for phase in phases+['before-observation']:
 dest=PACKET/phase;dest.mkdir(exist_ok=True)
 for p in (OUT/phase).iterdir():
  if p.suffix=='.png' or p.name in ['process.json','scenario.done','native.log']:
   shutil.copy2(p,dest/p.name)
 shutil.copy2(OUT/(phase+'.scn'),dest/'scenario.scn')
for name in ['source-inputs.json','test-artifact.json','final-knot-tests.log','final-retained-tests.log','final-producer-tests.log','final-window-input-tests.log','content-references.json','cold-prerequisites.json','voice-fixture-provenance.json','custodian-process.json','custodian-reopened-process.json','final-build.log','run_native.py','prepare_receiver.py','finalize_receipt.py']:
 shutil.copy2(OUT/name,PACKET/name)
assert json.loads((OUT/'before-observation/process.json').read_text())['success'] is False
for log in PACKET.rglob('*.log'):
 log.write_text(log.read_text().rstrip()+'\n')
proof={
 'date':'2026-10-20','passed':True,
 'scope':'Bounded shipping macOS primary mouse drag in embedded Knot; immediate retention action; sealed selected export, source-absent independent receiver, custodian and receiver restart, cold re-export and previously qualified selected-voice playback',
 'source_base':sources['base_commit'],'sources':sources,'phases':results,'checks':checks,
 'reviewed_captures':sum(p['captures'] for p in results.values()),
 'knot_selection':{'text':'café','utf8_start':16,'utf8_end':21,'selected_body_bytes':5,'source_unchanged_before_removal':True},
 'cold_packages_equal_selected_originals':True,'source_and_transfer_inputs_absent':True,'raw_store_privacy_scan':privacy,
 'custodian_restarted':True,'offline_acquisition_attempts':0,
 'negative_control':{'phase':'before-observation','expected':'fail','reason':'Pointer trace holds bytes 16..21 while the app retention command facts remain stale before the synchronous observation update'},
 'limits':['Synthetic wallet-backed custodian; no personal vault migration','Explicit scenario destination uses the ordinary export action/effect; OS save dialog not automated','Primary mouse and this bounded text fixture; no touch, bidi, autoscroll, or broad native editor acceptance claim','Voice body reused from the October 19 owner-export receipt; no renewed recording or owner export claim','Five configured external Knot endpoint checks and the custody process helper remain ignored in the ordinary focused sweep; not a full workspace gate']}
(PACKET/'qualification.json').write_text(json.dumps(proof,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({'passed':True,'phases':len(phases),'captures':proof['reviewed_captures'],'checks':checks},indent=2))
