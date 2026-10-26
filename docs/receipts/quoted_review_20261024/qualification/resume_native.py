from pathlib import Path
import os,time,subprocess,json,shutil,hashlib
O=Path(__file__).resolve().parent;H=O/'native-harness';V='/Users/markik/Code/output/turnstone-r2-verifier-venv/bin/python';M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022');T=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022');E=Path('/Users/markik/Code/worktrees/eidetic-cross-source-answers-20261022')
def read(p):return json.loads(p.read_text())
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
limit=time.monotonic()+1800
while read(O/'profile-phase.json')['phase']!='profile frozen cohort passed; native product built':
 assert time.monotonic()<limit
 if (O/'profile-qualification-resume.log').read_text():raise RuntimeError((O/'profile-qualification-resume.log').read_text()[-3000:])
 time.sleep(1)
C=O/'audit-replay/candidate-01'
for name in ['wire-default-review.json','contract-input-review.json','structured-output-review.json','score-health-review.json']:assert read(C/name)['result']=='pass'
rows=[json.loads(line) for line in (O/'audit-product-build-02.jsonl').read_text().splitlines() if line.startswith('{')];assert rows[-1]['reason']=='build-finished' and rows[-1]['success']
product=Path(next(x['executable'] for x in reversed(rows) if x.get('executable') and x.get('target',{}).get('name')=='turnstone'));resident=Path((O/'audit-resident-binary-path').read_text().strip())
assert sha(resident)==read(C/'inputs.json')['binary_sha256']
baseline=read(O/'native-pair-02/frozen/product-baseline.json');baseline.update(binary_sha256=sha(product),build='Locked product build with audit-runtime.toml: optimized hot tensor/SHA/Curve25519/Ed25519; verification enabled; debug/incremental off; RUSTC/RUSTDOC /usr/local/bin',runtime_profile_sha256=sha(O/'audit-runtime.toml'));write(H/'product-baseline.json',baseline)
shutil.copy2(O/'audit-replay/measurement-contract.json',H/'measurement-contract.json')
write(H/'auxiliary-inputs.json',{p.name:sha(p) for p in H.iterdir() if p.is_file() and p.name!='auxiliary-inputs.json'})
write(O/'native03-start.json',{'product_sha256':sha(product),'resident_sha256':sha(resident),'snapshot_shutdown_limit_seconds':300,'per_answer_timeout_seconds':900,'per_answer_tokens':1024,'frozen_cases_sha256':sha(H/'cases.json'),'wall_time':time.time()})
command=[V,str(H/'run_native.py'),'--binary',str(product),'--resident',str(resident),'--supplier',str(M),'--eidetic',str(E),'--model','/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct','--embedding','/Users/markik/Code/output/embedding-envelope-20261020/all-MiniLM-L6-v2','--embedding-artifacts','/Users/markik/Code/output/embedding-envelope-20261020/model-artifacts.json','--output',str(O/'native-pair-03'),'--qwen15','--mode','fused']
with (O/'native-pair-driver-03.log').open('w') as f:r=subprocess.run(command,cwd=T,stdout=f,stderr=subprocess.STDOUT)
write(O/'native-pair-exit-03.json',{'exit':r.returncode});assert r.returncode==0
for helper in ['verify_native_defaults.py','verify_native_facts.py']:
 with (O/(helper+'.log')).open('w') as f:subprocess.run([V,str(H/helper),str(O/'native-pair-03')],stdout=f,stderr=subprocess.STDOUT,check=True)
subprocess.run(['python3',str(O/'check_native_health.py')],check=True)
