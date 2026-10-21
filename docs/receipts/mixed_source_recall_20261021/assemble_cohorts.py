#!/usr/bin/env python3
"""Join completed lexical and fused pairs without rerunning accepted phases."""
import argparse, json, shutil
from pathlib import Path
def read(p):return json.loads(p.read_text())
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for n in ['lexical','fused','output']:p.add_argument('--'+n,type=Path,required=True)
 a=p.parse_args();roots={'lexical':a.lexical.resolve(),'fused':a.fused.resolve()};inputs={k:read(v/'inputs.json') for k,v in roots.items()}
 for field in ['binary_sha256','resident_sha256','source_sha256','supplier_source_sha256','generation_model','embedding_model']:
  assert inputs['lexical'][field]==inputs['fused'][field],field
 for name in ['cases.json','product-baseline.json','knot.mereclip','redshank.mereclip','fixture_server.py','run_macos_scenario.py']:
  assert inputs['lexical']['frozen_sha256'][name]==inputs['fused']['frozen_sha256'][name],name
 a.output.mkdir(parents=True,exist_ok=False)
 for mode,root in roots.items():
  lane=root/mode;assert read(lane/'completion.json')['result']=='pass'
  phases=read(lane/'phases.json');assert len(phases)==2 and all(v['clean_shutdown'] for v in phases)
  write(lane/'inputs.json',inputs[mode]);recipe=lane/'recipe';recipe.mkdir(exist_ok=False)
  for source in (root/'frozen').iterdir():
   if source.name not in ['turnstone','inference_route']:shutil.copy2(source,recipe/source.name)
  (a.output/mode).symlink_to(lane,target_is_directory=True)
  for phase in [0,1]:(a.output/f'{mode}-resident-{phase}.log').symlink_to(root/f'{mode}-resident-{phase}.log')
 common=inputs['fused'].copy();common['cohorts']={k:{'run':str(v),'recipe':k+'/recipe','inputs':k+'/inputs.json'} for k,v in roots.items()}
 write(a.output/'inputs.json',common);write(a.output/'completion.json',{'result':'pass','modes':['lexical','fused'],'boundary':'Both independent warm/cold pairs completed. Identical product, resident, supplier source and frozen corpus; per-mode executed UI recipes retained.'})
 print('Completed cohorts assembled; independent verification is still required')
if __name__=='__main__':main()
