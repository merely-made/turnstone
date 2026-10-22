#!/usr/bin/env python3
"""Verify and retain public synthetic mixed-source evidence only."""
import argparse, gzip, hashlib, json, re, runpy, shutil, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for n in ['run','supplier','logs']:p.add_argument('--'+n,type=Path,required=True)
 a=p.parse_args();raw=a.run.resolve();inputs=json.loads((raw/'inputs.json').read_text())
 assert all(sha(ROOT/n)==h for n,h in inputs['source_sha256'].items())
 assert all(sha(a.supplier/n)==h for n,h in inputs['supplier_source_sha256'].items())
 for name,digest in inputs['frozen_sha256'].items():
  if (HERE/name).is_file():assert sha(HERE/name)==digest,'public recipe differs from the executed frozen input: '+name
 sys.argv=['verify_native.py',str(raw)];runpy.run_path(str(HERE/'verify_native.py'),run_name='__main__')
 dest=HERE/'native';dest.mkdir(exist_ok=False)
 def copy(source,relative):
  target=dest/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
 def zipped(source,target):
  target.parent.mkdir(parents=True,exist_ok=True)
  with target.open('wb') as stream:
   with gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0) as z:z.write(source.read_bytes())
 for name in ['inputs.json','completion.json','verification.json']:copy(raw/name,name)
 for mode in ['lexical','fused']:
  copy(raw/mode/'inputs.json',Path(mode)/'inputs.json')
  for file in (raw/mode/'recipe').iterdir():copy(file,Path(mode)/'recipe'/file.name)
  for file in (raw/mode/'control').glob('selected-*.json'):copy(file,Path(mode)/'selection-acknowledgements'/file.name)
  for name in ['completion.json','phases.json','receiver-workspace.json','private-store-scan.json','knot-cold-export.mereclip','note-cold-export.mereclip','origin-stop.json','origin-ready.json','requests-at-stop.json','requests-final.json']:copy(raw/mode/name,Path(mode)/name)
  for phase in ['warm','cold']:
   for file in (raw/mode/phase).iterdir():
    if file.suffix=='.png' or file.name.endswith(('.saved-sources.json','.source-captures.json')) or file.name in ['process.json','launch.json','scenario.done','launcher.log']:copy(file,Path(mode)/phase/file.name)
    elif file.name=='native.log':zipped(file,dest/mode/phase/'native.log.gz')
  for file in (raw/mode/'resident').glob('*.json'):copy(file,Path(mode)/'resident'/file.name)
  for phase in [0,1]:zipped(raw/f'{mode}-resident-{phase}.log',dest/mode/f'resident-{phase}.log.gz')
 checks={};software=HERE/'software';software.mkdir(exist_ok=False)
 for name in ['mixed-publication-tests.log','mixed-recall-tests.log','mixed-lexical-route.log','mixed-library-route.log']:
  body=(a.logs/name).read_text();m=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',body);assert m
  checks[name]={'passed':int(m[1]),'failed':int(m[2]),'ignored':int(m[3]),'log':name+'.gz'}
  zipped(a.logs/name,software/(name+'.gz'))
 zipped(a.logs/'mixed-resident-build.log',software/'mixed-resident-build.log.gz')
 assert sum(v['passed'] for v in checks.values())==19 and not any(v['failed'] for v in checks.values())
 write(HERE/'software-checks.json',checks)
 sys.argv=['verify_native.py',str(dest)];runpy.run_path(str(HERE/'verify_native.py'),run_name='__main__')
 write(HERE/'qualification.json',{'result':'pass','compiled_inputs':inputs,'software_checks':checks,'native':json.loads((dest/'verification.json').read_text()),'public_evidence_sha256':{str(f.relative_to(HERE)):sha(f) for f in sorted(dest.rglob('*')) if f.is_file()},'private_material_excluded':['wallets','profiles and encrypted databases','model weights','keys','sockets','executables'],'scope':'Bounded macOS native mixed-source recall, lexical and real MiniLM fused modes, three provenance-specific generation asks and eight frozen ranking questions; independent product/resident cold pairs; previously qualified selected producer packages.'})
 print('Mixed-source public receipt finalized')
if __name__=='__main__':main()
