#!/usr/bin/env python3
"""Verify transport hashes, frozen inputs, raw contract, and external semantic gold.
Does not execute the model. Requires the Python blake3 package.
"""
from pathlib import Path
import collections,hashlib,importlib.util,json,subprocess,sys
sys.dont_write_bytecode=True
R=Path(__file__).resolve().parent;O=R/'qualification'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 hashes=json.loads((R/'hashes.json').read_text());files={str(p.relative_to(R)):p for p in R.rglob('*') if p.is_file() and p.name!='hashes.json' and '__pycache__' not in p.parts}
 assert set(files)==set(hashes),'receipt coverage changed'
 for name,p in files.items():assert sha(p)==hashes[name],('transport hash',name)
 spec=importlib.util.spec_from_file_location('qualification',O/'verify_suite.py');v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
 freeze=json.loads((O/'freeze.json').read_text());suite=json.loads((O/'frozen/suite.json').read_text());summary=json.loads((O/'summary.json').read_text());indexed={(r['cohort'],r['case']):r for r in summary['cases']}
 assert freeze['baseline_binary_sha256']=='699c8ac6c911e3b7e0cf017cd300bb238e93102501168629d4bf9703a9130466'
 for name,h in freeze['sha256'].items():
  if name=='inference_route':assert h==freeze['baseline_binary_sha256'];continue
  assert sha(O/'frozen'/name)==h,('frozen input',name)
 harness=json.loads((O/'harness-freeze.json').read_text())
 for name,h in harness['sha256'].items():
  assert sha(O/'frozen'/name)==h,('frozen harness',name)
  if name!='verify_suite.py':assert sha(O/name)==h
 amendment=json.loads((O/'verification-amendment.json').read_text());assert amendment['original_sha256']==sha(O/'frozen/verify_suite.py') and amendment['corrected_sha256']==sha(O/'verify_suite.py')
 corrected=(O/'frozen/verify_suite.py').read_text()
 for old,new in amendment['replacements']:
  assert corrected.count(old)==1;corrected=corrected.replace(old,new)
 assert corrected==(O/'verify_suite.py').read_text()
 actual=[];completed=0;bound=0;events_total=0;semantic_passed=0;admission_passed=0
 for c in suite['cohorts']:
  d=O/c['name'];assert json.loads((d/'cases.json').read_text())==c
  proc=json.loads((d/'process-exit.json').read_text());log=(d/'run.log').read_text();events=v.health.parse(log);events_total+=len(events);remaining=collections.defaultdict(list)
  for e in events:remaining[e['prompt']].append(e)
  for case in c['asks']:
   row=indexed[(c['name'],case['id'])];p=d/(case['id']+'.json')
   if case.get('expected_admission_error'):
    assert proc['exit_code']!=0 and not proc['outer_timeout'] and case['expected_admission_error'] in log and not p.exists() and not events
    assert row['result']=='pass' and row['kind']=='admission_negative';admission_passed+=1;continue
   assert p.exists(),('incomplete',c['name'],case['id']);run=json.loads(p.read_text())
   assert proc['exit_code']==0 and not proc['outer_timeout']
   assert run['state']=='finished' and not (d/(case['id']+'.timeout.json')).exists();completed+=1
   assert run['question']==case['question']
   allowed=['capture','control','knot','note'] if case['id']=='conflict' else ['capture','knot'] if case['id']=='reduced' else ['capture','knot','note']
   assert sorted(s['source'].rsplit('/',1)[-1] for s in run['recall']['scope'])==sorted(allowed)
   for passage in run['recall']['passages']:assert v.label(passage) in allowed and passage['passage']['text']==c['corpus'][v.label(passage)]['text']
   assert run['session']=={'session_id':'blake3:0960d64d68cf80b35a5781e3daeddfcf453f30579861e02af8d93a535db58d3d','prompt_template_hash':'blake3:9c222535ed31aadd676fc2f1beb16d0dd0402c76a9ee1398bc0c200c825103da'}
   assert run['recall']['support']['semantic_review']=='unreviewed'
   b=v.binding.check(run,v.instructions);s=v.shape.check(run);bound+=1
   assert row['input_identity']=='pass' and row['structured_output']=='pass' and row['facts']==b['facts'] and row['shape']==s
   ev=remaining[run['recall']['answer_contract']['prompt']].pop(0)
   assert ev['checked_rows']>0 and ev['invalid_rows']==ev['prefix_recomputations']==ev['failed_recoveries']==0
   assert row['score_health']=={'result':'pass','event':ev}
   details=v.semantic(case,run);assert details==row['slots'],('semantic report changed',c['name'],case['id'])
   result='pass' if all(r['result']=='pass' for r in details) else 'fail';assert result==row['result']
   semantic_passed+=result=='pass';actual.append({'cohort':c['name'],'case':case['id'],'result':result})
  assert not any(remaining.values()),'unjoined numerical events'
 assert completed==bound==events_total==15 and admission_passed==1
 assert summary['semantic_cases']=={'total':15,'passed':semantic_passed,'failed':15-semantic_passed,'incomplete':0}
 assert summary['admission_negatives']=={'total':1,'passed':1}
 assert summary['result']==('pass' if semantic_passed==15 else 'gate_not_passed')
 rubric=subprocess.run([sys.executable,str(O/'test_rubric.py')],capture_output=True,text=True);assert rubric.returncode==0,rubric.stdout+rubric.stderr
 print(json.dumps({'receipt_integrity':'pass','frozen_cases':16,'complete_raw_documents':completed,'contract_bindings':bound,'semantic_passed':semantic_passed,'semantic_failed':15-semantic_passed,'admission_negative_passed':admission_passed,'score_health_requests':events_total,'qualification_gate':summary['result'],'rubric_checks':61,'boundary':'This verifies the preserved measurement, including failures. It does not turn a failed semantic gate into a pass, replay the model, or qualify a headed/cold flow.'},indent=2))
if __name__=='__main__':main()
