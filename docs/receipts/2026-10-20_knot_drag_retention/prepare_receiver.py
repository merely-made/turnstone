from pathlib import Path
import json,subprocess,hashlib
OUT=Path(__file__).parent
ROOT=OUT/'runtime'
def address(package):
    wire=json.dumps({'clip':package['clip'],'private_backing':[]},ensure_ascii=False,separators=(',',':')).encode()
    return 'mere://clip/'+subprocess.check_output(['/Users/markik/Code/output/eidetic-headed-20261019/blake3_identity'],input=wire).decode().strip()
knot=json.loads((ROOT/'selected-knot.mereclip').read_text())
voice=json.loads((ROOT/'voice-fixture.mereclip').read_text())
assert len(knot['payloads'])==1 and bytes(knot['payloads'][0]['bytes'])=='café'.encode()
assert knot['clip']['anchors'][0]['selector']['byte_span']=={'start':16,'end':21}
assert b'private prefix' not in (ROOT/'selected-knot.mereclip').read_bytes()
assert b'private suffix' not in (ROOT/'selected-knot.mereclip').read_bytes()
(OUT/'selected-knot-reference.json').write_text(json.dumps(knot,ensure_ascii=False,indent=2)+'\n')
(OUT/'selected-voice-reference.json').write_text(json.dumps(voice,indent=2)+'\n')
refs={'knot':address(knot),'voice':address(voice)}
(OUT/'content-references.json').write_text(json.dumps(refs,indent=2)+'\n')
commands=['settle 10','key escape','assert snap network-offline == true','assert snap peer-services == 0',
 'import-clip '+str(ROOT/'selected-knot.mereclip'),'settle 20','act Open node in Workbench','settle 10',
 'assert snap browser-current ~ '+refs['knot'],'capture 01_received_drag_selection',
 'import-clip '+str(ROOT/'voice-fixture.mereclip'),'settle 20','act Open node in Workbench','settle 10',
 'click-at 850 192','settle 40','assert snap retained-voice ~ Ended',
 'assert snap offline-acquisition-attempts == 0','capture 02_received_voice_played','act Save session']
(OUT/'receiver.scn').write_text('\n'.join(commands)+'\n')
commands=['settle 10','key escape','assert snap network-offline == true','assert snap peer-services == 0',
 'open '+refs['knot'],'settle 20','act Open node in Workbench','settle 10','click-at 610 193',
 'assert snap actions ~ Export retained clip','export-clip '+str(ROOT/'cold-knot.mereclip'),
 'wait-file 20 '+str(ROOT/'cold-knot.mereclip'),'assert event clip-package-exported','capture 01_cold_drag_selection_exported',
 'open '+refs['voice'],'settle 20','act Open node in Workbench','settle 10','click-at 850 192','settle 40',
 'assert snap retained-voice ~ Ended','export-clip '+str(ROOT/'cold-voice.mereclip'),
 'wait-file 20 '+str(ROOT/'cold-voice.mereclip'),'assert event clip-package-exported',
 'assert snap offline-acquisition-attempts == 0','capture 02_cold_voice_played_and_exported','act Save session']
(OUT/'cold-receiver.scn').write_text('\n'.join(commands)+'\n')
print(json.dumps(refs))
