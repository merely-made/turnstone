from pathlib import Path
import os,subprocess,json,re,hashlib,shutil,time
O=Path(__file__).resolve().parent;W=Path('/Users/markik/Code/worktrees');M=W/'mere-cross-source-answers-20261022';T=W/'turnstone-cross-source-answers-20261022';D=O/'order-replay';D.mkdir(exist_ok=True)
env={**os.environ,'TMPDIR':'/tmp','RUSTC':'/usr/local/bin/rustc','RUSTDOC':'/usr/local/bin/rustdoc','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_INCREMENTAL':'0'}
def git(d,*args):return subprocess.check_output(['git',*args],cwd=d,text=True).strip()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def phase(s):write(O/'order-phase.json',{'phase':s,'unix':time.time()})
def run(cmd,d,log):
 with (O/log).open('w') as f:subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
def commit(d,paths,message):
 assert git(d,'diff','--cached','--name-only')==''
 subprocess.run(['git','diff','--check'],cwd=d,check=True);subprocess.run(['git','add','--',*paths],cwd=d,check=True);subprocess.run(['git','commit','-m',message],cwd=d,check=True);subprocess.run(['git','push','origin','HEAD:codex/cross-source-answers-20261022'],cwd=d,check=True)
s=(O/'order-resident-tests.log').read_text();assert '46 passed; 0 failed' in s and '8 passed; 0 failed' in s
phase('publishing tested tie-order fix');pins=json.loads((O/'supplier-pins.json').read_text());assert git(M,'rev-parse','HEAD')==pins['mere'];commit(M,['ports/distillery/src/inference/recall.rs','ports/distillery/src/inference/recall_embedding.rs'],'Stabilize recall score ties before source identities');old=pins['mere'];pins['mere']=git(M,'rev-parse','HEAD');p=T/'Cargo.lock';p.write_text(p.read_text().replace(old,pins['mere']));write(O/'supplier-pins.json',pins)
base=['/usr/local/bin/cargo','--config',str(O/'audit-runtime.toml'),'--config','build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"','--config','build.build-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-build"']
phase('building final route and qualifying pin refresh');run(base+['test','--locked','-j4','-p','djinn','--features','inference-embedding,inference-gpu','--test','inference_route','--no-run'],M,'order-resident-build.log');binary=Path(re.search(r'Executable tests/inference_route.rs \((.*?)\)',(O/'order-resident-build.log').read_text()).group(1));(O/'order-resident-binary-path').write_text(str(binary)+'\n')
tbase=base.copy();tbase[tbase.index('build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"')]='build.target-dir="/Users/markik/Code/output/turnstone-embedding-headed-20261020/target"'
with (O/'order-turnstone-tests.log').open('w') as log:
 t=subprocess.Popen(tbase+['test','--locked','-j2','--lib','inference'],cwd=T,env=env,stdout=log,stderr=subprocess.STDOUT)
 try:run([str(binary)],M,'order-route-tests.log');assert t.wait(timeout=1800)==0
 finally:
  if t.poll() is None:t.terminate();t.wait(timeout=30)
assert '9 passed; 0 failed' in (O/'order-turnstone-tests.log').read_text();assert '4 passed; 0 failed' in (O/'order-route-tests.log').read_text();commit(T,['Cargo.lock'],'Refresh resident with stable recall tie ordering');pins['turnstone']=git(T,'rev-parse','HEAD');write(O/'supplier-pins.json',pins)
for name,head in pins.items():
 d=W/(name+'-cross-source-answers-20261022');assert git(d,'status','--short')=='' and git(d,'rev-parse','HEAD')==head
for n in ['run_probe.py','verify_contract_inputs.py','verify_wire_defaults.py','verify_structured_output.py','verify_score_health.py','optimized-runtime.toml']:shutil.copy2(O/'audit-replay'/n,D/n)
p=D/'run_probe.py';s=p.read_text().replace("shutil.copy2(M/'ports/distillery/src/inference/candidate_review.rs',frozen/'candidate_review.rs')", "shutil.copy2(M/'ports/distillery/src/inference/candidate_review.rs',frozen/'candidate_review.rs')\nfor n in ['recall.rs','recall_embedding.rs']: shutil.copy2(M/'ports/distillery/src/inference'/n,frozen/n)")
s=s.replace("assert sha(frozen/'candidate_review.rs')", "for n,digest in contract['recall_source_sha256'].items(): assert sha(frozen/n)==digest\nassert sha(frozen/'candidate_review.rs')");p.write_text(s)
v=json.loads((O/'audit-replay/measurement-contract.json').read_text());v['supplier_pins']=pins;v['supplier_commit']=pins['mere'];v['recall_source_sha256']={n:sha(M/'ports/distillery/src/inference'/n) for n in ['recall.rs','recall_embedding.rs']};v['source_identity_tie_policy']='Preserve scores; exact passage text descending breaks ties before source-dependent manifest ID, in lexical/vector/fused lanes. Identical text keeps distinct provenance. No candidate/output repair.';v['source_identity_regression']='24 identity permutations; lexical/vector/fused order and exact answer prompt/context/grammar stable; provenance follows actual source. Before-fix regression fails.';v['helper_sha256']={n:sha(D/n) for n in ['run_probe.py','verify_contract_inputs.py','verify_wire_defaults.py','verify_structured_output.py','verify_score_health.py']};write(D/'measurement-contract.json',v)
phase('running frozen cohort with stable ties and building final product')
with (O/'order-product-build.jsonl').open('w') as out,(O/'order-product-build.log').open('w') as err,(O/'order-probe-driver.log').open('w') as log:
 product=subprocess.Popen(tbase+['build','--locked','-j2','--bin','turnstone','--message-format=json-render-diagnostics'],cwd=T,env=env,stdout=out,stderr=err)
 try:
  probe=subprocess.run(['python3',str(D/'run_probe.py'),str(binary)],cwd=M,env=env,stdout=log,stderr=subprocess.STDOUT);write(O/'order-probe-exit.json',{'exit':probe.returncode});assert probe.returncode==0;assert product.wait(timeout=1800)==0
 finally:
  if product.poll() is None:product.terminate();product.wait(timeout=30)
phase('stable-tie frozen cohort passed; final product built')
