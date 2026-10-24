from pathlib import Path
import os,time,subprocess,json,shutil,hashlib
O=Path(__file__).resolve().parent;H=O/'native-harness';V='/Users/markik/Code/output/turnstone-r2-verifier-venv/bin/python';M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022');T=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022');E=Path('/Users/markik/Code/worktrees/eidetic-cross-source-answers-20261022')
def read(p):return json.loads(p.read_text())
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
limit=time.monotonic()+1800
while read(O/'order-phase.json')['phase']!='stable-tie frozen cohort passed; final product built':
 assert time.monotonic()<limit
 if 'Traceback (most recent call last):' in (O/'order-qualification-driver.log').read_text():raise RuntimeError((O/'order-qualification-driver.log').read_text()[-3000:])
 time.sleep(1)
C=O/'order-replay/candidate-01'
for name in ['wire-default-review.json','contract-input-review.json','structured-output-review.json','score-health-review.json']:assert read(C/name)['result']=='pass'
rows=[json.loads(line) for line in (O/'order-product-build.jsonl').read_text().splitlines() if line.startswith('{')];assert rows[-1]['reason']=='build-finished' and rows[-1]['success']
product=Path(next(x['executable'] for x in reversed(rows) if x.get('executable') and x.get('target',{}).get('name')=='turnstone'));resident=Path((O/'order-resident-binary-path').read_text().strip())
assert sha(resident)==read(C/'inputs.json')['binary_sha256']
baseline=read(O/'native-pair-02/frozen/product-baseline.json');paths=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','Cargo.toml','Cargo.lock'],cwd=T,text=True).splitlines();baseline['source_sha256']={n:sha(T/n) for n in sorted(set(paths)) if (T/n).is_file()};baseline['turnstone_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=T,text=True).strip();baseline.update(binary_sha256=sha(product),build='Locked product build with audit-runtime.toml: optimized hot tensor/SHA/Curve25519/Ed25519; verification enabled; debug/incremental off; RUSTC/RUSTDOC /usr/local/bin',runtime_profile_sha256=sha(O/'audit-runtime.toml'));write(H/'product-baseline.json',baseline)
shutil.copy2(O/'order-replay/measurement-contract.json',H/'measurement-contract.json')
for name in ['recall.rs','recall_embedding.rs']:shutil.copy2(M/'ports/distillery/src/inference'/name,H/name)
write(H/'auxiliary-inputs.json',{p.name:sha(p) for p in H.iterdir() if p.is_file() and p.name!='auxiliary-inputs.json'})
write(O/'native04-start.json',{'product_sha256':sha(product),'resident_sha256':sha(resident),'snapshot_shutdown_limit_seconds':300,'per_answer_timeout_seconds':900,'per_answer_tokens':1024,'frozen_cases_sha256':sha(H/'cases.json'),'wall_time':time.time()})
command=[V,str(H/'run_native.py'),'--binary',str(product),'--resident',str(resident),'--supplier',str(M),'--eidetic',str(E),'--model','/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct','--embedding','/Users/markik/Code/output/embedding-envelope-20261020/all-MiniLM-L6-v2','--embedding-artifacts','/Users/markik/Code/output/embedding-envelope-20261020/model-artifacts.json','--output',str(O/'native-pair-04'),'--qwen15','--mode','fused']
with (O/'native-pair-driver-03.log').open('w') as f:r=subprocess.run(command,cwd=T,stdout=f,stderr=subprocess.STDOUT)
write(O/'native-pair-exit-04.json',{'exit':r.returncode});assert r.returncode==0
for helper in ['verify_native_defaults.py','verify_native_facts.py']:
 with (O/('native04-'+helper+'.log')).open('w') as f:subprocess.run([V,str(H/helper),str(O/'native-pair-04')],stdout=f,stderr=subprocess.STDOUT,check=True)
subprocess.run(['python3',str(O/'check_native_health.py')],check=True)
