#!/usr/bin/env python3
"""Verify source binding, exact cross-store citations and cold native outcomes."""
import argparse, hashlib, json, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('output',type=Path);p.add_argument('--commit');a=p.parse_args();out=a.output
 inputs=json.loads((out/'inputs.json').read_text());q=json.loads((out/'qualification.json').read_text());assert q['result']=='pass'
 for name,digest in inputs['source_sha256'].items():
  assert sha(ROOT/name)==digest,name+' drifted'
  if a.commit:
   data=subprocess.check_output(['git','show',a.commit+':'+name],cwd=ROOT);assert hashlib.sha256(data).hexdigest()==digest,name+' differs from committed source'
 for run in q['runs']:
  assert run['launcher_exit']==0 and run['process']['success'] and not run['process']['timeout']
  assert run['process']['binary_sha256']==inputs['binary_sha256']
 identity=None
 for phase,name in [('seed','06_saved_passage'),('seed','09_follow_exact_citation'),('seed','11_old_A1_after_A2'),('reopen','13_cold_exact_A1'),('reopen','15_cold_old_A1_after_A2')]:
  value=json.loads((out/phase/(name+'.saved-sources.json')).read_text())
  readers=[r for r in value['readers'] if r['phase']=='Ready' and r.get('content',{}).get('text')=="The Pelican observatory's access word is café."]
  assert readers
  if identity is None:identity=readers[-1]['target']
  exact=[r for r in readers if r['target']==identity];assert exact,(phase,name)
  r=exact[0];s=r['target']['selector'];assert r['content']['text'][s['start']:s['end']]=='café'
 for phase in ['seed','reopen']:
  report=json.loads((out/(phase+'-store-inspection.json')).read_text());assert report['result']=='pass' and report['independent_after_app_exit']
  citation=report['citations'][0];assert citation['target']==identity and citation['quote']=='café' and citation['verified_against_turnstone_capture'] and citation['portable_and_transport_hashes_verified'] and citation['corrupt_bytes_refused']
 assert (out/'requests-at-stop-1.json').read_bytes()==(out/'fixture/requests.json').read_bytes()
 for phase in [0,1]:
  report=json.loads((out/f'origin-stop-{phase}.json').read_text());assert report['exited'] and report['tcp_refused']
 notes=json.loads((out/'note-identities.json').read_text());assert sha(Path(notes['other']))==notes['other_sha256']
 print(json.dumps({'result':'pass','committed_source':a.commit,'exact_target':identity,'headed_captures':q['captures'],'cross_store_cold_read':True},ensure_ascii=False,indent=2))
if __name__=='__main__':main()
