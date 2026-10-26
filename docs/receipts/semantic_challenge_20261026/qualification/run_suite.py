from pathlib import Path
import datetime,hashlib,json,os,subprocess,time
O=Path(__file__).resolve().parent
M=Path('/Users/markik/Code/worktrees/mere-cross-source-answers-20261022')
MODEL='/Users/markik/Code/output/cross-source-answers-20261022/qwen2.5-1.5b-instruct'
def write(p,v):p.write_text(json.dumps(v,ensure_ascii=False,indent=2)+'\n')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
freeze=json.loads((O/'freeze.json').read_text())
for name,digest in freeze['sha256'].items():assert sha(O/'frozen'/name)==digest,name
suite=json.loads((O/'frozen/suite.json').read_text())
write(O/'runner-freeze.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'sha256':sha(Path(__file__))})
start=time.monotonic();results=[]
for c in suite['cohorts']:
 d=O/c['name']; assert not (d/'run.log').exists()
 env=os.environ.copy()
 for n in list(env):
  if n.startswith('DJINN_CROSS_SOURCE_QWEN'):env.pop(n)
 env.update(TMPDIR='/tmp',DJINN_CROSS_SOURCE_QWEN_DIR=MODEL,DJINN_CROSS_SOURCE_QWEN15='1',CROSS_SOURCE_CASES=str(d/'cases.json'),CROSS_SOURCE_OUTPUT=str(d))
 cmd=[str(O/'frozen/inference_route'),'probe_bounded_cross_source_answers','--exact','--ignored','--nocapture']
 started=time.monotonic();seen={};timedout=False
 with (d/'run.log').open('w') as log,(d/'progress.jsonl').open('w') as progress:
  child=subprocess.Popen(cmd,cwd=M,env=env,stdout=log,stderr=subprocess.STDOUT)
  write(d/'process.json',{'pid':child.pid,'command':cmd,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat()})
  while True:
   for p in sorted(d.glob('*.partial.json')):
    try:r=json.loads(p.read_text())
    except (ValueError,OSError):continue
    state=(r['state'],r['tokens']);key=p.stem
    if state!=seen.get(key):
     ev={'elapsed_seconds':time.monotonic()-started,'case':key,'state':r['state'],'tokens':r['tokens']};progress.write(json.dumps(ev)+'\n');progress.flush()
     if not key in seen or state[0]!='running':print(c['name'],ev,flush=True)
     seen[key]=state
   if child.poll() is not None:break
   if time.monotonic()-started>len(c['asks'])*940+90:
    timedout=True;child.terminate()
    try:child.wait(timeout=30)
    except subprocess.TimeoutExpired:child.kill();child.wait()
    break
   time.sleep(.5)
 rc={'cohort':c['name'],'exit_code':child.returncode,'elapsed_seconds':time.monotonic()-started,'outer_timeout':timedout}
 write(d/'process-exit.json',rc);results.append(rc);print('COHORT_EXIT',json.dumps(rc),flush=True)
write(O/'process-summary.json',{'elapsed_seconds':time.monotonic()-start,'cohorts':results})
print('SUITE_FINISHED',flush=True)
