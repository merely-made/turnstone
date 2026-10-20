from pathlib import Path
import json,subprocess,hashlib,shutil
OUT=Path(__file__).parent; ROOT=OUT/'runtime'
assert (ROOT/'fixture-marker').read_text()=='redshank-text-retention-20261020\n'
p=ROOT/'selected-text.mereclip'; pkg=json.loads(p.read_text())
print(json.dumps(pkg,ensure_ascii=False,indent=2))
assert len(pkg['payloads'])==1
text='Saved from the embedded listener: café → harbor.\nKeep this timed note.'
assert bytes(pkg['payloads'][0]['bytes'])==text.encode()
assert all(x not in p.read_text() for x in ['PRIVATE CONTROL','UNSELECTED CONTROL'])
(OUT/'selected-text-reference.json').write_text(json.dumps(pkg,ensure_ascii=False,indent=2)+'\n')
wire=json.dumps({'clip':pkg['clip'],'private_backing':[]},ensure_ascii=False,separators=(',',':')).encode()
address='mere://clip/'+subprocess.check_output(['/Users/markik/Code/output/eidetic-headed-20261019/blake3_identity'],input=wire).decode().strip()
(OUT/'content-reference.json').write_text(json.dumps({'text':address},indent=2)+'\n')
base=['settle 12','key escape','assert snap network-offline == true','assert snap peer-services == 0']
def save(name,lines):(OUT/name).write_text('\n'.join(lines)+'\n')
save('producer-cold.scn',base+['assert snap producer-retention == Unavailable','open '+address,'settle 20','act Open node in Workbench','settle 10','assert content-live','click-at 610 193','assert snap actions ~ Export retained clip','capture 01_source_absent_cold_text','export-clip '+str(ROOT/'onward-text.mereclip'),'wait-file 20 '+str(ROOT/'onward-text.mereclip'),'assert event clip-package-exported','assert snap offline-acquisition-attempts == 0','capture 02_source_absent_onward_export','act Save session'])
save('receiver.scn',base+['import-clip '+str(ROOT/'onward-text.mereclip'),'settle 20','act Open node in Workbench','settle 10','assert content-live','assert snap browser-current ~ '+address,'assert snap producer-retention == Unavailable','assert snap offline-acquisition-attempts == 0','capture 01_independent_received_text','act Save session'])
save('receiver-cold.scn',base+['open '+address,'settle 20','act Open node in Workbench','settle 10','assert content-live','click-at 610 193','assert snap actions ~ Export retained clip','export-clip '+str(ROOT/'cold-text.mereclip'),'wait-file 20 '+str(ROOT/'cold-text.mereclip'),'assert event clip-package-exported','assert snap offline-acquisition-attempts == 0','capture 01_independent_cold_export','act Save session'])
original=(ROOT/'source-session').read_text().strip(); active=(ROOT/'producer/current_session').read_text().strip()
assert original!=active
session=ROOT/'producer/sessions'/original
assert (session/'redshank').is_dir()
recording=ROOT/'origin-recording.wav'
receipt={'original_source_session':original,'new_active_session':active,'removed_source_session':str(session),'removed_recording':str(recording),'recording_sha256':hashlib.sha256(recording.read_bytes()).hexdigest(),'source_session_files':[str(x.relative_to(session)) for x in session.rglob('*') if x.is_file()]}
shutil.rmtree(session);recording.unlink();p.unlink()
receipt.update(source_session_absent=not session.exists(),recording_absent=not recording.exists(),selected_transfer_absent=not p.exists(),active_session_preserved=(ROOT/'producer/sessions'/active).is_dir())
assert all(receipt[k] for k in ['source_session_absent','recording_absent','selected_transfer_absent','active_session_preserved'])
for name in ['seed-model.json','expected-note-export.json','source-session']:
    (ROOT/name).rename(OUT/name)
receipt['fixture_diagnostics_outside_application_runtime']=True
(OUT/'source-removal.json').write_text(json.dumps(receipt,indent=2)+'\n')
