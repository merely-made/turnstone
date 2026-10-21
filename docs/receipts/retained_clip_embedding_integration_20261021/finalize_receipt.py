#!/usr/bin/env python3
"""Verify and copy only public synthetic acceptance evidence into this packet."""
import argparse, gzip, hashlib, json, runpy, shutil, sys
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--run',type=Path,required=True);p.add_argument('--supplier',type=Path,required=True);p.add_argument('--logs',type=Path,required=True);a=p.parse_args();raw=a.run.resolve()
 inputs=json.loads((raw/'inputs.json').read_text())
 assert all(sha(ROOT/n)==h for n,h in inputs['source_sha256'].items())
 assert all(sha(a.supplier/n)==h for n,h in inputs['supplier_source_sha256'].items())
 shutil.copy2(HERE/'embedding-artifact-blake3.json',raw/'embedding-artifact-blake3.json')
 sys.argv=['verify_native.py',str(raw)];runpy.run_path(str(HERE/'verify_native.py'),run_name='__main__')
 dest=HERE/'native';dest.mkdir(exist_ok=False)
 def copy(source,relative):
  target=dest/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
 for name in ['inputs.json','completion.json','verification.json','embedding-artifact-blake3.json']:copy(raw/name,name)
 for kind in ['knot','redshank']:
  for name in ['completion.json','phases.json','receiver-workspace.json','private-store-scan.json','cold-export.mereclip']:copy(raw/kind/name,Path(kind)/name)
  for phase in ['warm','cold']:
   for file in (raw/kind/phase).iterdir():
    if file.suffix=='.png' or file.name.endswith('.saved-sources.json') or file.name in ['process.json','launch.json','scenario.done','launcher.log']:copy(file,Path(kind)/phase/file.name)
    elif file.name=='native.log':
     target=dest/kind/phase/'native.log.gz'
     with target.open('wb') as stream:
      with gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0) as zipped:zipped.write(file.read_bytes())
  for file in (raw/kind/'resident').glob('*.json'):copy(file,Path(kind)/'resident'/file.name)
  for phase in [0,1]:
   target=dest/kind/f'resident-{phase}.log.gz'
   with target.open('wb') as stream:
    with gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0) as zipped:zipped.write((raw/f'{kind}-resident-{phase}.log').read_bytes())
 checks={};software=HERE/'software';software.mkdir(exist_ok=False)
 patterns=['research-teststests','inference_tests','saved_sources-tests','retained_clip-tests','app-contributed_pane_arms-tests']
 for name in patterns:
  log=a.logs/f'clip-sdk-{name}.log';body=log.read_text();assert 'test result: ok.' in body
  import re
  match=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',body);assert match
  checks[name]={'passed':int(match[1]),'failed':int(match[2]),'ignored':int(match[3]),'log':log.name+'.gz'}
 for name in ['clip-context-tests-passed.log','clip-sdk-lexical-route.log','clip-sdk-library-route.log']:
  body=(a.logs/name).read_text();assert 'test result: ok.' in body
  import re
  match=re.search(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',body);assert match
  checks[name]={'passed':int(match[1]),'failed':int(match[2]),'ignored':int(match[3]),'log':name+'.gz'}
 names=[f'clip-sdk-{name}.log' for name in patterns]+['clip-context-tests-passed.log','clip-sdk-lexical-route.log','clip-sdk-library-route.log','clip-sdk-native-build.log','clip-sdk-test-build.log','clip-resident-sdk-build.log','clip-resident-observation-build.log','clip-sdk-metadata.log','clip-pin-refresh.log']
 for name in names:
  with (software/(name+'.gz')).open('wb') as stream:
   with gzip.GzipFile(filename='',mode='wb',fileobj=stream,mtime=0) as zipped:zipped.write((a.logs/name).read_bytes())
 write(HERE/'software-checks.json',checks)
 assert sum(v['passed'] for v in checks.values())==59 and sum(v['failed'] for v in checks.values())==0
 sys.argv=['verify_native.py',str(dest)];runpy.run_path(str(HERE/'verify_native.py'),run_name='__main__')
 write(HERE/'qualification.json',{'result':'pass','compiled_inputs':inputs,'software_checks':checks,'native':json.loads((dest/'verification.json').read_text()),'public_evidence_sha256':{str(f.relative_to(HERE)):sha(f) for f in sorted(dest.rglob('*')) if f.is_file()},'private_material_excluded':['wallets','profiles and retained databases','model weights','keys','sockets','executables'],'scope':'Bounded macOS native one-source receiving cases for previously qualified embedded producer packages; real MiniLM fused recall and GPU generation, independent product/resident cold pairs; model prompt diagnostics retained separately.'})
 print('retained clip embedding public packet finalized')
if __name__=='__main__':main()
