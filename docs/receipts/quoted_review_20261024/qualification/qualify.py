from pathlib import Path
import os,subprocess,json,re,time,hashlib,datetime
O=Path(__file__).resolve().parent;W=Path('/Users/markik/Code/worktrees');M=W/'mere-cross-source-answers-20261022';T=W/'turnstone-cross-source-answers-20261022'
env={**os.environ,'TMPDIR':'/tmp','RUSTC':'/usr/local/bin/rustc','RUSTDOC':'/usr/local/bin/rustdoc','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_INCREMENTAL':'0'}
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def phase(name):write(O/'phase.json',{'phase':name,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
def run(cmd,cwd,log):
 with (O/log).open('w') as f:
  if log.endswith('source-graph.json'):
   with (O/(log+'.stderr.log')).open('w') as err:subprocess.run(cmd,cwd=cwd,env=env,stdout=f,stderr=err,check=True)
  else:subprocess.run(cmd,cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
def git(cwd,*args):return subprocess.check_output(['git',*args],cwd=cwd,text=True).strip()
def commit(d,paths,message):
 subprocess.run(['git','diff','--check'],cwd=d,check=True)
 subprocess.run(['git','add','--',*paths],cwd=d,check=True)
 subprocess.run(['git','commit','-m',message],cwd=d,check=True)
 subprocess.run(['git','push','origin','HEAD:codex/cross-source-answers-20261022'],cwd=d,check=True)
base=['/usr/local/bin/cargo','--config',str(O/'optimized-runtime.toml'),'--config','build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"','--config','build.build-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-build"','test','--locked','-j4']
phase('awaiting final resident tests')
deadline=time.monotonic()+900
while True:
 s=(O/'resident-tests-final.log').read_text()
 if '45 passed; 0 failed' in s and '8 passed; 0 failed' in s:break
 assert not re.search(r'^error(?:\[|:)|FAILED',s,re.M),s[-5000:]
 assert time.monotonic()<deadline
 time.sleep(.5)
phase('publishing tested resident')
commit(M,['Cargo.toml','Cargo.lock','ports/distillery/src/inference/answer_contract.rs','ports/distillery/src/inference/candidate_review.rs','ports/distillery/src/inference/recall.rs','ports/distillery/src/inference/durable.rs'],'Bind quoted candidate reviews to extractive answers and exact claim citations')
pins=json.loads((O/'supplier-pins.json').read_text());old=pins['mere'];pins['mere']=git(M,'rev-parse','HEAD');p=T/'Cargo.lock';p.write_text(p.read_text().replace(old,pins['mere']));write(O/'supplier-pins.json',pins)
phase('building current route binary')
run(base+['-p','djinn','--features','inference-embedding,inference-gpu','--test','inference_route','--no-run'],M,'resident-build.log')
binary=Path(re.search(r'Executable tests/inference_route.rs \((.*?)\)',(O/'resident-build.log').read_text()).group(1));(O/'resident-binary-path').write_text(str(binary)+'\n')
phase('qualifying current Turnstone and ordinary routes')
tbase=base.copy();tbase[tbase.index('build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"')]='build.target-dir="/Users/markik/Code/output/turnstone-embedding-headed-20261020/target"';tbase[-1]='-j2'
with (O/'turnstone-tests.log').open('w') as log:
 t=subprocess.Popen(tbase+['--lib','inference'],cwd=T,env=env,stdout=log,stderr=subprocess.STDOUT)
 try:
  run([str(binary)],M,'resident-route-tests.log');assert t.wait(timeout=1800)==0
 finally:
  if t.poll() is None:t.terminate();t.wait(timeout=30)
assert '9 passed; 0 failed' in (O/'turnstone-tests.log').read_text()
assert '4 passed; 0 failed' in (O/'resident-route-tests.log').read_text()
phase('publishing current Turnstone pins')
commit(T,['Cargo.toml','Cargo.lock'],'Refresh quoted-review resident and evidence suppliers')
pins['turnstone']=git(T,'rev-parse','HEAD');write(O/'supplier-pins.json',pins)
canonical={}
for name,d in [('resident',M),('turnstone',T)]:
 run(['/usr/local/bin/cargo','metadata','--locked','--offline','--format-version','1'],d,name+'-source-graph.json')
 g=json.loads((O/(name+'-source-graph.json')).read_text());ps=[p for p in g['packages'] if p.get('source') and 'github.com/merely-made/eidetic.git' in p['source']];sources=sorted({p['source'] for p in ps});assert len(sources)==1 and pins['eidetic'] in sources[0],sources
 canonical[name]={'source':sources[0],'packages':sorted(p['name'] for p in ps)}
write(O/'canonical-sources.json',canonical)
for name,head in pins.items():
 d=W/(name+'-cross-source-answers-20261022');assert git(d,'status','--short')=='';assert git(d,'rev-parse','HEAD')==head
v=json.loads((O/'measurement-contract.json').read_text());v['supplier_pins']=pins;v['supplier_commit']=pins['mere'];v['prompt_module_sha256_before_measurement']=hashlib.sha256((M/'ports/distillery/src/inference/answer_contract.rs').read_bytes()).hexdigest();v['review_grammar_source_sha256']=hashlib.sha256((M/'ports/distillery/src/inference/candidate_review.rs').read_bytes()).hexdigest();v['helper_sha256']={n:hashlib.sha256((O/n).read_bytes()).hexdigest() for n in ['run_probe.py','verify_contract_inputs.py','verify_wire_defaults.py','verify_structured_output.py','verify_score_health.py']};v['citation_projection']='Structured citations use only checked claim-evidence links. Reviewed quotes and marker-like strings are data, and answer selectors address the actual evidence quotation inside the particular claim.';write(O/'measurement-contract.json',v)
phase('running unchanged frozen cohort')
result=subprocess.run(['python3',str(O/'run_probe.py'),str(binary)],cwd=M,env=env)
write(O/'probe-exit.json',{'exit_code':result.returncode});phase('frozen cohort passed' if result.returncode==0 else 'frozen gate did not pass')
raise SystemExit(result.returncode)
