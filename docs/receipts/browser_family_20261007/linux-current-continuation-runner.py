from pathlib import Path
import subprocess,os,json,hashlib,datetime,time,re,sys
r=Path('/home/markik/Code/repos/turnstone');out=r/'docs/receipts/browser_family_20261007';sha='abb349cf7957e2b509f4cb7f492e3e0db4821964';target=Path('/home/markik/Code/target')
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def git(*a):return subprocess.check_output(['git','-C',str(r),*a],text=True).strip()
def save(n,d):
 p=out/n;p.write_bytes((json.dumps(d,indent=2)+'\n').encode())
assert git('rev-parse','HEAD')==sha and git('branch','--show-current')=='main' and not git('diff','HEAD','--name-only')
original=json.loads((out/'linux-current-inputs.json').read_text());assert original['source_sha']==sha
for row in original['tracked_file_inventory']:assert hashlib.sha256((r/row['path']).read_bytes()).hexdigest()==row['sha256'],row['path']
assert not (out/'linux-current-continuation-summary.json').exists()
for prefix in ['linux-current-cargo-mode','linux-current-tree','linux-current-metadata']:
 for suffix in ['.result.json','.stdout.log','.stderr.log']:assert not (out/(prefix+suffix)).exists()
lock=subprocess.run(['fuser',str(target/'debug/.cargo-lock')],capture_output=True,text=True);assert lock.returncode==1 and not lock.stdout and not lock.stderr
owners=[];names={'cargo','rustc','rustdoc','cc','c++','clang','clang++','gcc','g++','ld','ld.lld','lld','cmake','ninja','make'}
for p in Path('/proc').iterdir():
 if p.name.isdigit():
  try:
   c=(p/'comm').read_text().strip()
   if c in names or c.startswith('turnstone'):owners.append({'pid':int(p.name),'comm':c})
  except OSError:pass
assert not owners,owners
env=os.environ.copy();env['CARGO_TARGET_DIR']=str(target);env['CARGO_BUILD_JOBS']='1'
gates=[('cargo-mode',['python3','scripts/cargo_mode.py','verify','+1.98.1']),('tree',['cargo','+1.98.1','tree','--locked']),('metadata',['cargo','+1.98.1','metadata','--locked','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu'])];results=[]
for name,cmd in gates:
 args=['nice','-n','10',*cmd];prefix='linux-current-'+name;result={'command':args,'cwd':str(r),'environment':{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1'},'source_sha':sha,'started_utc':stamp(),'actual_exit_code':None,'scope':'Continuation at original abb after preserving failed library gate; does not override that failure.'};start=time.monotonic()
 with (out/(prefix+'.stdout.log')).open('wb') as stdout,(out/(prefix+'.stderr.log')).open('wb') as stderr:
  p=subprocess.Popen(args,cwd=r,env=env,stdout=stdout,stderr=stderr);result['owned_pid']=p.pid;save(prefix+'.result.json',result);print('START '+name+' PID '+str(p.pid),flush=True);result['actual_exit_code']=p.wait()
 result['finished_utc']=stamp();result['elapsed_seconds']=round(time.monotonic()-start,3);result['final_source_sha']=git('rev-parse','HEAD');result['tracked_diff_after']=git('diff','HEAD','--name-only').splitlines();result['stdout_sha256']=hashlib.sha256((out/(prefix+'.stdout.log')).read_bytes()).hexdigest();result['stderr_sha256']=hashlib.sha256((out/(prefix+'.stderr.log')).read_bytes()).hexdigest();save(prefix+'.result.json',result);results.append(result);print('FINISH '+name+' exit '+str(result['actual_exit_code'])+' elapsed '+str(result['elapsed_seconds']),flush=True)
 if result['actual_exit_code']!=0 or result['final_source_sha']!=sha or result['tracked_diff_after']:break
changes=[]
for row in original['tracked_file_inventory']:
 p=r/row['path'];h=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
 if h!=row['sha256']:changes.append({'path':row['path'],'before':row['sha256'],'after':h})
summary={'recorded_utc':stamp(),'source_sha':sha,'final_source_sha':git('rev-parse','HEAD'),'gates':results,'all_continuation_gates_completed':len(results)==len(gates),'all_continuation_exit_zero':all(x['actual_exit_code']==0 for x in results),'tracked_diff_after':git('diff','HEAD','--name-only').splitlines(),'input_hash_changes':changes,'original_library_gate':'FAILED exit101,644passed/5failed/9ignored; retained separately.','prelaunch_known_owners':owners}
if len(results)==len(gates) and results[-1]['actual_exit_code']==0:
 m=json.loads((out/'linux-current-metadata.stdout.log').read_text());active=set(x['id'] for x in m['resolve']['nodes']);families={};paths=[]
 for p in m['packages']:
  s=p.get('source')
  if not s:paths.append({'name':p['name'],'version':p['version'],'manifest_path':p['manifest_path'],'active_in_default_resolve':p['id'] in active})
  else:
   match=re.search(r'https://github.com/(?:merely-made|servo)/(mere|genet|knot-editor|woodshed|wgpu-graft|wgpu-weld|wgpu-scry|servo)(?:[.?/#]|$)',s)
   if match:families.setdefault(match.group(1),{}).setdefault(s,[]).append({'name':p['name'],'version':p['version'],'active_in_default_resolve':p['id'] in active})
 save('linux-current-family-provenance.json',{'metadata_sha256':results[-1]['stdout_sha256'],'metadata_packages':len(m['packages']),'active_resolve_nodes':len(active),'families':families,'path_packages':paths,'scope':'Filtered Linux default-feature metadata. Metadata membership does not establish foreign native support.'})
save('linux-current-continuation-summary.json',summary);print(json.dumps(summary,indent=2),flush=True)
sys.exit(0 if summary['all_continuation_gates_completed'] and summary['all_continuation_exit_zero'] and not changes and not summary['tracked_diff_after'] else 1)
