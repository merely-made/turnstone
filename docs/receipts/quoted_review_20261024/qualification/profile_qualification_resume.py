from pathlib import Path
import os,subprocess,json,re,time,hashlib,shutil
O=Path(__file__).resolve().parent;M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022');T=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022');V='/Users/markik/Code/output/turnstone-r2-verifier-venv/bin/python'
env={**os.environ,'TMPDIR':'/tmp','RUSTC':'/usr/local/bin/rustc','RUSTDOC':'/usr/local/bin/rustdoc','CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_INCREMENTAL':'0'}
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def phase(v):write(O/'profile-phase.json',{'phase':v,'unix':time.time()})
binary=Path((O/'audit-resident-binary-path').read_text().strip());(O/'audit-resident-binary-path').write_text(str(binary)+'\n')
base=['/usr/local/bin/cargo','--config',str(O/'audit-runtime.toml'),'--config','build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"','--config','build.build-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-build"']
phase('rerunning frozen cases and building native product')
tbase=base.copy();tbase[tbase.index('build.target-dir="/Users/markik/Code/output/embedding-envelope-20261020/resident-target"')]='build.target-dir="/Users/markik/Code/output/turnstone-embedding-headed-20261020/target"'
with (O/'audit-product-build-02.jsonl').open('w') as out,(O/'audit-product-build-02.log').open('w') as err,(O/'audit-probe-driver-02.log').open('w') as log:
 product=subprocess.Popen(tbase+['build','--locked','-j2','--bin','turnstone','--message-format=json-render-diagnostics'],cwd=T,env=env,stdout=out,stderr=err)
 try:
  probe=subprocess.run(['python3',str(O/'audit-replay/run_probe.py'),str(binary)],cwd=M,env=env,stdout=log,stderr=subprocess.STDOUT)
  write(O/'audit-probe-exit-02.json',{'exit':probe.returncode});assert probe.returncode==0
  assert product.wait(timeout=1800)==0
 finally:
  if product.poll() is None:product.terminate();product.wait(timeout=30)
phase('profile frozen cohort passed; native product built')
