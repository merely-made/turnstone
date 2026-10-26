#!/usr/bin/env python3
import datetime, hashlib, json, os, shutil, subprocess, time
from pathlib import Path
O=Path(__file__).resolve().parent
M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022')
R=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022/docs/receipts/answer_contract_20261022')
MODEL=Path('/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct')
import argparse
p=argparse.ArgumentParser(); p.add_argument('binary',type=Path); p.add_argument('--label',default='candidate-01'); a=p.parse_args()
out=O/a.label; out.mkdir(); frozen=out/'frozen'; frozen.mkdir()
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v): p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
shutil.copy2(a.binary,frozen/'inference_route')
for name in ['cases.json','case-freeze.json','verify_probe.py','verify_native.py']: shutil.copy2(R/name,frozen/name)
shutil.copy2(O/'optimized-runtime.toml',frozen/'optimized-runtime.toml')
shutil.copy2(Path(__file__),frozen/'run_probe.py')
shutil.copy2(M/'ports/distillery/src/inference/answer_contract.rs',frozen/'answer_contract.rs')
shutil.copy2(M/'ports/distillery/src/inference/candidate_review.rs',frozen/'candidate_review.rs')
for n in ['recall.rs','recall_embedding.rs']: shutil.copy2(M/'ports/distillery/src/inference'/n,frozen/n)
shutil.copy2(O/'verify_contract_inputs.py',frozen/'verify_contract_inputs.py')
shutil.copy2(O/'verify_wire_defaults.py',frozen/'verify_wire_defaults.py')
shutil.copy2(O/'verify_structured_output.py',frozen/'verify_structured_output.py')
shutil.copy2(O/'verify_score_health.py',frozen/'verify_score_health.py')
shutil.copy2(O/'measurement-contract.json',frozen/'measurement-contract.json')
contract=json.loads((frozen/'measurement-contract.json').read_text())
assert sha(frozen/'answer_contract.rs')==contract['prompt_module_sha256_before_measurement']
for n,digest in contract['recall_source_sha256'].items(): assert sha(frozen/n)==digest
assert sha(frozen/'candidate_review.rs')==contract['review_grammar_source_sha256']
assert sha(M/'ports/djinn/tests/inference_route.rs')==contract['probe_source_sha256']
for name,digest in contract['unchanged_frozen_sha256'].items(): assert sha(frozen/name)==digest
assert sha(frozen/'optimized-runtime.toml')==contract['runtime_profile_sha256']
assert sha(frozen/'cases.json')==json.loads((frozen/'case-freeze.json').read_text())['cases_sha256']
model=json.loads((MODEL/'verified.json').read_text())
for n,size,digest in model['artifacts']:
 assert (MODEL/n).stat().st_size==size and sha(MODEL/n)==digest
paths=subprocess.check_output(['git','diff','--name-only','HEAD'],cwd=M,text=True).splitlines()
write(out/'inputs.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'supplier_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=M,text=True).strip(),'supplier_dirty_hashes':{n:sha(M/n) for n in paths if (M/n).is_file()},'binary_sha256':sha(frozen/'inference_route'),'frozen_sha256':{f.name:sha(f) for f in frozen.iterdir()},'generation_model':model,'host_capacity_budget_bytes':3*1024**3,'device_layer_payload_budget_bytes':4*1024**3,'numerical_format':'original f32 layered execution','esp_opt_level':3,'runtime_optimization':'Explicit optimized-runtime.toml for hot tensor/backend crates, SHA-256, Curve25519 and Ed25519; all verification stays enabled','case_timeout_seconds':900,'token_limit':1024})
env=os.environ.copy()
for n in list(env):
 if n.startswith('DJINN_CROSS_SOURCE_QWEN'): env.pop(n)
env.update(TMPDIR='/tmp',DJINN_CROSS_SOURCE_QWEN_DIR=str(MODEL),DJINN_CROSS_SOURCE_QWEN15='1',CROSS_SOURCE_CASES=str(frozen/'cases.json'),CROSS_SOURCE_OUTPUT=str(out))
cmd=[str(frozen/'inference_route'),'probe_bounded_cross_source_answers','--exact','--ignored','--nocapture']
start=time.monotonic(); seen={}; baseline=json.loads((Path('/Users/markik/Code/output/streamed-inference-20261022/baseline-profile/cross.partial.json')).read_text())['session']; mismatch=False
with (out/'run.log').open('w') as log, (out/'progress.jsonl').open('w') as progress:
 child=subprocess.Popen(cmd,cwd=M,env=env,stdout=log,stderr=subprocess.STDOUT)
 write(out/'process.json',{'pid':child.pid,'command':cmd,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
 while True:
  for path in sorted(out.glob('*.partial.json')):
   try: run=json.loads(path.read_text())
   except (ValueError,OSError): continue
   key=path.name.removesuffix('.partial.json'); status=(run['tokens'],run['state'])
   if status!=seen.get(key):
    event={'elapsed_seconds':time.monotonic()-start,'case':key,'tokens':run['tokens'],'state':run['state'],'session_matches_baseline':run['session']==baseline}
    mismatch=mismatch or not event['session_matches_baseline']; progress.write(json.dumps(event)+'\n'); progress.flush(); seen[key]=status
  if child.poll() is not None: break
  time.sleep(.25)
 write(out/'process-exit.json',{'exit_code':child.returncode,'elapsed_seconds':time.monotonic()-start,'session_mismatch':mismatch})
verifier='/Users/markik/Code/output/turnstone-r2-verifier-venv/bin/python'
result=subprocess.run([verifier,str(frozen/'verify_probe.py'),str(out)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(out/'verification.log').write_text(result.stdout)
print(result.stdout,flush=True)
binding=subprocess.run([verifier,str(frozen/'verify_contract_inputs.py'),str(out)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(out/'contract-verification.log').write_text(binding.stdout)
print(binding.stdout,flush=True)
review=subprocess.run([verifier,str(frozen/'verify_wire_defaults.py'),str(out)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(out/'wire-default-verification.log').write_text(review.stdout)
print(review.stdout,flush=True)
shape=subprocess.run([verifier,str(frozen/'verify_structured_output.py'),str(out)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(out/'structured-verification.log').write_text(shape.stdout)
print(shape.stdout,flush=True)
health=subprocess.run([verifier,str(frozen/'verify_score_health.py'),str(out)],stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(out/'score-health-verification.log').write_text(health.stdout)
print(health.stdout,flush=True)
raise SystemExit(0 if health.returncode==0 and shape.returncode==0 and review.returncode==0 and child.returncode==0 and not mismatch and binding.returncode==0 else 1)
