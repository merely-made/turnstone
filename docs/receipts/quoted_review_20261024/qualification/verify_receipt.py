#!/usr/bin/env python3
"""Verify public evidence without inference, UI launch or raw-output mutation."""
from pathlib import Path
import hashlib,json,subprocess,sys,tempfile,shutil,re,importlib.util
sys.dont_write_bytecode=True
R=Path(__file__).resolve().parent
def read(p):return json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def invoke(helper,run,expected=0):
 v=subprocess.run([sys.executable,str(helper),str(run)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 assert v.returncode==expected,(helper,v.returncode,v.stdout[-5000:])
manifest=read(R/'hashes.json')
actual={str(p.relative_to(R)):sha(p) for p in R.rglob('*') if p.is_file() and p.name!='hashes.json' and '__pycache__' not in p.parts}
assert actual==manifest,'published file set or hash changed'
for dst,entry in read(R/'copy-ledger.json').items():assert sha(R/dst)==entry['sha256']
assert read(R/'summary.json')['result']=='pass'
for lane in ['candidate-01','candidate-02','candidate-03','native-pair-01','native-pair-02','native-pair-03','native-pair-04','failed-v6/candidate-01']:
 for name,value in read(R/lane/'inputs.json')['frozen_sha256'].items():
  if name in ['inference_route','turnstone']:continue
  assert sha(R/lane/'frozen'/name)==value,(lane,name)
for name,value in read(R/'native-harness/auxiliary-inputs.json').items():assert sha(R/'native-harness'/name)==value

assert read(R/'candidate-01/gate.json')['result']=='gate_not_passed' # original direct indexing of omitted citations
assert read(R/'failed-v6/candidate-01/wire-default-review.json')['result']=='gate_not_passed'
assert all(v['result']=='fail' for v in read(R/'failed-v6/candidate-01/wire-default-review.json')['cases'].values())
assert (R/'native-pair-01/fused/warm/scenario.done').read_text().startswith('RESULT fail')
assert read(R/'native-pair-01/warm-cross-review.json')['semantic']['status']==['supported']*3
assert read(R/'native-pair-04/completion.json')['result']=='pass'
assert read(R/'native-pair-03/warm-failure-review.json')['result']=='gate_not_passed'
assert read(R/'native-pair-03/warm-failure-review.json')['cases']['conflict']['result']=='fail'
assert not (R/'native-pair-03/fused/cold/scenario.done').exists()
assert read(R/'native-pair-02/warm-only-review.json')['warm_result']=='pass'
assert read(R/'native-pair-02/warm-only-review.json')['native_pair_result']=='incomplete'
head_inputs=read(R/'candidate-03/inputs.json');native_inputs=read(R/'native-pair-04/inputs.json')
assert head_inputs['binary_sha256']==native_inputs['resident_sha256']
assert head_inputs['generation_model']==native_inputs['generation_model']
assert head_inputs['case_timeout_seconds']==900 and head_inputs['token_limit']==1024
assert head_inputs['supplier_commit']==native_inputs['supplier_base']
for phase in read(R/'native-pair-04/fused/phases.json'):
 assert phase['snapshot_shutdown_limit_seconds']==300 and 0<phase['snapshot_shutdown_seconds']<=300

# Whole original semantic assertion function is unchanged in the binding fix.
a=(R/'native-harness/verify_native.py').read_text();b=(R/'native-harness/verify_native_bindings.py').read_text()
def semantic(s):return s[s.index('def verify_answer('):s.index('\ndef ',s.index('def verify_answer(')+1)]
assert semantic(a)==semantic(b)
assert sha(R/'native-harness/cases.json')=='b91de4627aee873389066f4a8ed814fe4faff85cf1c8a3ae1d8e74f988d27ea2'
assert sha(R/'native-harness/verify_native.py')=='f3a0e939cef9ac3bf85f80cb9d1e984ee40d94a0757cf6d4c6c6d55e23b88594'
# All checkers write only into throwaway copies. Their shipped code is hash-bound above.
with tempfile.TemporaryDirectory(prefix='quoted-review-recheck-') as td:
 tmp=Path(td);head=tmp/'candidate-01';native=tmp/'native-pair-04';helpers=tmp/'native-harness'
 for src,dst in [(R/'candidate-03',head),(R/'native-pair-04',native),(R/'native-harness',helpers)]:shutil.copytree(src,dst)
 for name in ['verify_contract_inputs.py','verify_structured_output.py','verify_wire_defaults.py','verify_score_health.py']:
  invoke(head/'frozen'/name,head)
 invoke(helpers/'verify_native_defaults.py',native);invoke(helpers/'verify_native_facts.py',native)
 for name in ['contract-input-review.json','structured-output-review.json','wire-default-review.json','score-health-review.json']:
  assert read(head/name)['result']=='pass'
 for name in ['wire-default-native-review.json','quoted-review-native-review.json']:assert read(native/name)['result']=='pass'
# Health observations require all five headed generations, distinct from four reopened histories.
spec=importlib.util.spec_from_file_location('score_health',R/'qualification/verify_score_health.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
for phase,count in [(0,4),(1,1)]:
 events=m.parse((R/f'native-pair-04/fused-resident-{phase}.log').read_text());assert len(events)==count
 for event in events:assert event['checked_rows']>0 and event['invalid_rows']==event['prefix_recomputations']==event['failed_recoveries']==0
assert read(R/'native-pair-04/visual-review.json')['result']=='pass'
logs={'eidetic-tests.log':[20],'resident-tests-final.log':[45,8],'order-resident-tests.log':[46,8],'order-route-tests.log':[4],'order-turnstone-tests.log':[9],'resident-route-tests.log':[4],'turnstone-tests-after-probe.log':[9],'saved-reader-probe-tests-final.log':[1]}
for name,counts in logs.items():
 text=(R/'qualification'/name).read_text()
 for count in counts:assert re.search(r'test result: ok\. '+str(count)+r' passed; 0 failed;',text)
print(json.dumps({'result':'pass','files':len(manifest),'software_checks':88,'headless_semantic':4,'headed_warm_semantic':4,'reopened_histories':4,'fresh_cold_generations':1,'note':'Checks recorded evidence consistency; does not rerun inference or native windows.'},indent=2))
