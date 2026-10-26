#!/usr/bin/env python3
"""Apply the declared optional-array wire default before the unchanged rubric.
The original checker and results stay frozen. No output text or support changes.
"""
from pathlib import Path
import argparse,copy,json,sys,hashlib

def decoded(run):
 result=copy.deepcopy(run);r=result['recall'];applied=[]
 if 'citations' not in r:r['citations']=[];applied.append('recall.citations=[]')
 assert isinstance(r['citations'],list),'citations must be an array, never null'
 return result,applied

def main():
 p=argparse.ArgumentParser();p.add_argument('probe',type=Path);p.add_argument('--case');a=p.parse_args();f=a.probe/'frozen';sys.path.insert(0,str(f));from verify_native import verify_answer
 cases=json.loads((f/'cases.json').read_text());checks={};controls={}
 for case in cases['asks']:
  key=case['id']
  if a.case and key!=a.case:continue
  path=a.probe/(key+'.json')
  if not path.exists():checks[key]={'result':'not_run'};continue
  run=json.loads(path.read_text())
  if run['state']!='finished' or (a.probe/(key+'.timeout.json')).exists():checks[key]={'result':'incomplete','state':run['state'],'tokens':run['tokens']};continue
  try:
   view,defaults=decoded(run);detail=verify_answer(view,case,cases);checks[key]={'result':'pass','protocol_defaults_applied':defaults,**detail}
   if case['expected_status']==['insufficient']:
    invalid=copy.deepcopy(view);invalid['recall']['citations']=[{'handle':999,'passage':'invented','basis':'model_marker'}]
    try:verify_answer(invalid,case,cases)
    except (AssertionError,KeyError,TypeError,ValueError):controls[key+'_nonempty_citation']='rejected'
    else:raise AssertionError('Negative citation control unexpectedly passed')
    invalid=copy.deepcopy(view);invalid['recall']['citations']=None
    try:decoded(invalid)
    except AssertionError:controls[key+'_null_citations']='rejected'
    else:raise AssertionError('Null-array control unexpectedly passed')
  except (AssertionError,KeyError,TypeError,ValueError) as e:checks[key]={'result':'fail','error':repr(e)}
 result={'result':'pass' if len(checks)==4 and all(c['result']=='pass' for c in checks.values()) else 'gate_not_passed','cases':checks,'negative_controls':controls,'unchanged_checker_sha256':hashlib.sha256((f/'verify_native.py').read_bytes()).hexdigest(),'cases_sha256':hashlib.sha256((f/'cases.json').read_bytes()).hexdigest(),'boundary':'Same frozen rubric; decode omitted citations as [] per RecallEvidence serde(default,skip_serializing_if=Vec::is_empty). Raw files, model output and support remain unchanged. Any incomplete case still blocks headed checks.'}
 name='wire-default-review.json' if not a.case else a.case+'-wire-default-review.json';(a.probe/name).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n');print(json.dumps(result,ensure_ascii=False,indent=2))
 raise SystemExit(0 if all(c['result']=='pass' for c in checks.values()) else 1)
if __name__=='__main__':main()
