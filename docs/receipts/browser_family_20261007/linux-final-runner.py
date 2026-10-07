from pathlib import Path
import subprocess,os,json,hashlib,datetime,time,sys,tomllib,re
r=Path('/home/markik/Code/repos/turnstone');out=r/'docs/receipts/browser_family_20261007';target=Path('/home/markik/Code/target');sha='8d1f907f4bd31b256737a27d3e63fe8cd8672efc'
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def git(*a):return subprocess.check_output(['git','-C',str(r),*a],text=True).strip()
def save(n,d):(out/n).write_bytes((json.dumps(d,indent=2)+'\n').encode())
assert git('branch','--show-current')=='main' and git('rev-parse','HEAD')==sha and not git('diff','HEAD','--name-only')
assert not list(out.glob('linux-final*')),'Preserving existing remote current receipt'
assert (target/'CACHEDIR.TAG').read_text().startswith('Signature: 8a477f597d28d172789f06886806bc55')
names={'cargo','rustc','rustdoc','turnstone','cc','c++','clang','clang++','gcc','g++','ld','ld.lld','lld','cmake','ninja','make'};owners=[]
for p in Path('/proc').iterdir():
 if p.name.isdigit():
  try:
   if (p/'comm').read_text().strip() in names:owners.append(int(p.name))
  except OSError:pass
assert not owners,owners
lock=subprocess.run(['fuser',str(target/'debug/.cargo-lock')],capture_output=True,text=True);assert lock.returncode==1 and not lock.stdout and not lock.stderr
inventory=[]
for n in git('ls-files').splitlines():
 p=r/n
 if p.is_file():inventory.append({'path':n,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
lockdata=tomllib.loads((r/'Cargo.lock').read_text());selected=[{'name':p['name'],'version':p['version'],'source':p.get('source')} for p in lockdata['package'] if p.get('source','').startswith('git+')]
inputs={'recorded_utc':stamp(),'host':'thinkpad-l14-f','source_sha':sha,'source_tree':git('rev-parse','HEAD^{tree}'),'tracked_clean':True,'tracked_file_inventory':inventory,'selected_git_lock_packages':selected,'toolchain':subprocess.check_output(['rustup','run','1.98.1','rustc','-Vv'],text=True),'target':str(target),'target_marker_sha256':hashlib.sha256((target/'CACHEDIR.TAG').read_bytes()).hexdigest(),'prelaunch_named_owners':owners,'prelaunch_cargo_lock_observer_exit':lock.returncode,'scope':'Current published-family default Linux compile/tests/portable-lock verification; no foreign Windows backend features, headed browser or AT acceptance.'}
save('linux-final-inputs.json',inputs)
for n in ['Cargo.toml','Cargo.lock']:(out/('linux-final-'+n)).write_bytes((r/n).read_bytes())
env=os.environ.copy();env['CARGO_TARGET_DIR']=str(target);env['CARGO_BUILD_JOBS']='1'
gates=[('lib',['cargo','+1.98.1','test','--workspace','--lib','--locked','-j1']),('check',['cargo','+1.98.1','check','--workspace','--all-targets','--locked','-j1']),('cargo-mode',['python3','scripts/cargo_mode.py','verify','+1.98.1']),('tree',['cargo','+1.98.1','tree','--locked']),('metadata',['cargo','+1.98.1','metadata','--locked','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu'])]
results=[]
for name,cmd in gates:
 args=['nice','-n','10',*cmd];prefix='linux-final-'+name;result={'command':args,'cwd':str(r),'environment':{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1'},'source_sha':sha,'started_utc':stamp(),'scope':inputs['scope'],'actual_exit_code':None};start=time.monotonic()
 with (out/(prefix+'.stdout.log')).open('wb') as stdout,(out/(prefix+'.stderr.log')).open('wb') as stderr:
  p=subprocess.Popen(args,cwd=r,env=env,stdout=stdout,stderr=stderr);result['owned_pid']=p.pid;save(prefix+'.result.json',result);print('START '+name+' PID '+str(p.pid)+' '+result['started_utc'],flush=True);result['actual_exit_code']=p.wait()
 result['finished_utc']=stamp();result['elapsed_seconds']=round(time.monotonic()-start,3);result['final_source_sha']=git('rev-parse','HEAD');result['tracked_diff_after']=git('diff','HEAD','--name-only').splitlines();result['stdout_sha256']=hashlib.sha256((out/(prefix+'.stdout.log')).read_bytes()).hexdigest();result['stderr_sha256']=hashlib.sha256((out/(prefix+'.stderr.log')).read_bytes()).hexdigest();save(prefix+'.result.json',result);results.append(result);print('FINISH '+name+' exit '+str(result['actual_exit_code'])+' elapsed '+str(result['elapsed_seconds']),flush=True)
 if result['actual_exit_code']!=0 or result['final_source_sha']!=sha or result['tracked_diff_after']:break
changes=[]
for row in inventory:
 p=r/row['path'];h=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
 if h!=row['sha256']:changes.append({'path':row['path'],'before':row['sha256'],'after':h})
summary={'recorded_utc':stamp(),'source_sha':sha,'final_source_sha':git('rev-parse','HEAD'),'gates':results,'all_requested_gates_completed':len(results)==len(gates),'all_completed_exit_zero':all(x['actual_exit_code']==0 for x in results),'tracked_diff_after':git('diff','HEAD','--name-only').splitlines(),'input_hash_changes':changes,'scope':inputs['scope']}
if len(results)==len(gates) and results[-1]['actual_exit_code']==0:
 m=json.loads((out/'linux-final-metadata.stdout.log').read_text());active=set(x['id'] for x in m['resolve']['nodes']);families={};paths=[]
 for p in m['packages']:
  s=p.get('source')
  if not s:paths.append({'name':p['name'],'version':p['version'],'manifest_path':p['manifest_path'],'active_in_default_resolve':p['id'] in active})
  else:
   match=re.search(r'https://github.com/(?:merely-made|servo)/(mere|genet|knot-editor|woodshed|wgpu-graft|wgpu-weld|wgpu-scry|servo)(?:[.?/#]|$)',s)
   if match:families.setdefault(match.group(1),{}).setdefault(s,[]).append({'name':p['name'],'version':p['version'],'active_in_default_resolve':p['id'] in active})
 save('linux-final-family-provenance.json',{'metadata_sha256':results[-1]['stdout_sha256'],'metadata_packages':len(m['packages']),'active_resolve_nodes':len(active),'families':families,'path_packages':paths,'scope':'Filtered Linux default-feature metadata. Source membership/compile proof does not establish foreign native producer support.'})
save('linux-final-summary.json',summary);print(json.dumps(summary,indent=2),flush=True)
sys.exit(0 if summary['all_requested_gates_completed'] and summary['all_completed_exit_zero'] and not changes and not summary['tracked_diff_after'] else 1)
