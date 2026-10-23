#!/usr/bin/env python3
"""Check preserved evidence, including expected failures; never run inference."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,sys
HERE=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text())
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--hashes',action='store_true');args=parser.parse_args()
 def hashes():
  expected=read(HERE/'hashes.json')
  actual={str(p.relative_to(HERE)) for p in HERE.rglob('*') if p.is_file() and p.name!='hashes.json'}
  assert actual==set(expected),(actual-set(expected),set(expected)-actual)
  for name,value in expected.items():assert sha(HERE/name)==value,name
 if args.hashes:hashes()
 summary=read(HERE/'summary.json');assert summary['result']=='gate_not_passed'
 assert sum(v for k,v in summary['software_checks'].items() if k!='total')==summary['software_checks']['total']==180
 contract=read(HERE/'measurement-contract.json');P=HERE/'candidate-01';F=P/'frozen'
 assert read(F/'measurement-contract.json')==contract
 for name,value in contract['unchanged_frozen_sha256'].items():assert sha(F/name)==value,name
 assert sha(F/'answer_contract.rs')==contract['prompt_module_sha256_before_measurement']
 assert sha(F/'verify_score_health.py')==contract['score_health_verifier_sha256']
 expected_returns={'verify_probe.py':1,'verify_contract_inputs.py':0,'verify_wire_defaults.py':1,'verify_structured_output.py':0,'verify_score_health.py':0}
 assert summary['probe']['gate_returns']==expected_returns
 env={**os.environ,'PYTHONDONTWRITEBYTECODE':'1'}
 for script,expected in expected_returns.items():
  run=subprocess.run([sys.executable,str(F/script),str(P)],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  assert run.returncode==expected,(script,run.returncode,run.stdout)
 review=read(P/'wire-default-review.json')
 assert [key for key,value in review['cases'].items() if value['result']=='pass']==['cross']
 assert all(review['cases'][key]['result']=='fail' for key in ['missing','conflict','reduced'])
 assert read(P/'gate.json')['headed_allowed'] is False
 for name in ['contract-input-review.json','structured-output-review.json','score-health-review.json']:
  assert read(P/name)['result']=='pass',name
 health=read(P/'score-health-review.json')
 assert health['total_observed_invalid_rows']==health['total_prefix_recomputations']==0
 assert sum(row['checked_rows'] for row in health['cases'].values())==91
 for key,expected in summary['probe']['cases'].items():
  run=read(P/(key+'.json'));assert run['state']==expected['state']=='finished'
  assert run['tokens']==expected['tokens'] and run['error']==expected['error'] is None
  assert (P/(key+'.json')).read_bytes()==(P/(key+'.partial.json')).read_bytes()
 assert read(P/'process-exit.json')==summary['probe']['process']
 assert summary['probe']['semantic_passes']==1 and summary['probe']['completed_answers']==4
 assert summary['headed_answer']==summary['source_absent_cold_reopen']==summary['new_native_binary_build']=='not_run_gate_not_passed'
 for name in ['esp-tests-qualification.log','resident-tests-qualification.log','resident-route-tests.log','turnstone-tests.log']:
  assert 'test result: ok.' in (HERE/'checks'/name).read_text()
 assert 'selected=16 finite=false score=NaN' in (HERE/'diagnostics/nan-pick-baseline.log').read_text()
 assert 'selected=17 finite=true score=2' in (HERE/'diagnostics/nan-pick-fixed.log').read_text()
 if args.hashes:hashes()
 print(json.dumps({'receipt_verification':'pass','recorded_qualification':'gate_not_passed','completed_answers':4,'semantic_passes':1,'software_checks':180,'score_rows_checked':91,'observed_invalid_rows':0,'headed_and_cold_reopen':'not_run_gate_not_passed'},indent=2))
if __name__=='__main__':main()
