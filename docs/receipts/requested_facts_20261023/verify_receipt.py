#!/usr/bin/env python3
"""Recheck the retained result, including its failed gates; never rerun models."""
from pathlib import Path
import argparse,hashlib,json,subprocess,sys
HERE=Path(__file__).resolve().parent

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def read(p):return json.loads(p.read_text())
def main():
 p=argparse.ArgumentParser();p.add_argument('--hashes',action='store_true');a=p.parse_args()
 if a.hashes:
  expected=read(HERE/'hashes.json');actual={str(f.relative_to(HERE)) for f in HERE.rglob('*') if f.is_file() and f.name!='hashes.json'};assert actual==set(expected)
  for n,h in expected.items():assert sha(HERE/n)==h,n
 contract=read(HERE/'measurement-contract.json');frozen=HERE/'candidate-01/frozen'
 for n,h in contract['unchanged_frozen_sha256'].items():assert sha(frozen/n)==h,n
 assert sha(frozen/'answer_contract.rs')==contract['prompt_module_sha256_before_measurement']
 summary=read(HERE/'summary.json');assert summary['software_checks']['total']==81
 actual={}
 for script,expected in summary['probe']['gate_returns'].items():
  result=subprocess.run([sys.executable,str(frozen/script),str(HERE/'candidate-01')],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);assert result.returncode==expected,(script,result.returncode,result.stdout);actual[script]=result.returncode
 assert read(HERE/'candidate-01/gate.json')['headed_allowed'] is False
 assert read(HERE/'candidate-01/contract-input-review.json')['result']=='pass'
 review=read(HERE/'candidate-01/wire-default-review.json');assert review['cases']['reduced']['result']=='pass';assert all(review['cases'][k]['result']=='incomplete' for k in ['cross','missing','conflict'])
 for key,expected in summary['probe']['cases'].items():
  run=read(HERE/'candidate-01'/f'{key}.json');assert run['state']==expected['state'] and run['tokens']==expected['tokens'] and run['error']==expected['error']
 assert summary['headed_answer']==summary['source_absent_cold_reopen']=='not_run_gate_not_passed'
 for n in ['eidetic-model-tests.log','resident-unit-tests-final.log','resident-route-tests.log','turnstone-tests.log']:assert 'test result: ok.' in (HERE/'checks'/n).read_text()
 d=read(HERE/'constraint-diagnostic.json');assert d['eligible_pieces']==['1','2','3'];assert sha(HERE/'diagnostics/constraint.rs')==d['source_sha256'];assert sha(HERE/'diagnostics/debug-prefix.txt')==d['prefix_sha256'];assert sha(HERE/'diagnostics/debug-grammar.json')==d['grammar_sha256']
 if a.hashes:
  for n,h in read(HERE/'hashes.json').items():assert sha(HERE/n)==h,n
 print(json.dumps({'receipt_verification':'pass','recorded_qualification':'gate_not_passed','recorded_semantic_passes':1,'source_plan_bindings':4,'software_checks':81,'gate_returns':actual,'headed_and_cold_reopen':'not_run_gate_not_passed'},indent=2))
if __name__=='__main__':main()
