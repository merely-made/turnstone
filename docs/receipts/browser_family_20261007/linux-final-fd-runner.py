from pathlib import Path
import subprocess,os,json,hashlib,datetime,time,sys,tomllib,re,resource,threading
r=Path('/home/markik/Code/repos/turnstone');out=r/'docs/receipts/browser_family_20261007';target=Path('/home/markik/Code/target');sha='8d1f907f4bd31b256737a27d3e63fe8cd8672efc'
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def git(*a):return subprocess.check_output(['git','-C',str(r),*a],text=True).strip()
def save(n,d):(out/n).write_bytes((json.dumps(d,indent=2)+'\n').encode())
assert git('branch','--show-current')=='main' and git('rev-parse','HEAD')==sha and not git('diff','HEAD','--name-only')
assert not list(out.glob('linux-final-fd*')),'Preserving existing remote current receipt'
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
save('linux-final-fd-inputs.json',inputs)
for n in ['Cargo.toml','Cargo.lock']:(out/('linux-final-fd-'+n)).write_bytes((r/n).read_bytes())
env=os.environ.copy();env['CARGO_TARGET_DIR']=str(target);env['CARGO_BUILD_JOBS']='1'
host_nofile=resource.getrlimit(resource.RLIMIT_NOFILE);child_soft=min(65536,host_nofile[1]);assert host_nofile[0]==1024 and child_soft>host_nofile[0]
def child_limits():resource.setrlimit(resource.RLIMIT_NOFILE,(child_soft,host_nofile[1]))
def proc_identity(pid):
 try:
  p=Path('/proc')/str(pid);fields=(p/'stat').read_text().rsplit(')',1)[1].split();return {'pid':pid,'parent_pid':int(fields[1]),'start_ticks':int(fields[19]),'comm':(p/'comm').read_text().strip()}
 except (OSError,ValueError):return None
def observe_descriptors(root_pid,stop,record):
 root_identity=proc_identity(root_pid)
 if root_identity is None:record['root_identity_unavailable']=True;return
 record['root_identity']=root_identity;record['sampling_interval_seconds']=0.5;record['processes']={};record['limitations']='Accessible /proc snapshots at 0.5s; peaks are observed lower bounds, not exhaustive maxima. Only verified launched-root lineage with child creation at/after root is sampled; exited/inaccessible processes may be missed.'
 while not stop.is_set():
  current=proc_identity(root_pid)
  if current is None or current['start_ticks']!=root_identity['start_ticks']:break
  all_processes={}
  for pp in Path('/proc').iterdir():
   if pp.name.isdigit():
    ident=proc_identity(int(pp.name))
    if ident:all_processes[ident['pid']]=ident
  owned={root_pid}
  while True:
   new={pid for pid,x in all_processes.items() if x['parent_pid'] in owned and x['start_ticks']>=root_identity['start_ticks']}-owned
   if not new:break
   owned.update(new)
  for pid in owned:
   ident=all_processes.get(pid)
   if not ident:continue
   try:
    count=sum(1 for _ in (Path('/proc')/str(pid)/'fd').iterdir());line=next(x for x in (Path('/proc')/str(pid)/'limits').read_text().splitlines() if x.startswith('Max open files'))
    key=str(pid)+':'+str(ident['start_ticks']);row=record['processes'].setdefault(key,{**ident,'first_observed_utc':stamp(),'peak_open_descriptors':0,'samples':0,'observed_limit_line':line});row['peak_open_descriptors']=max(row['peak_open_descriptors'],count);row['samples']+=1;row['last_observed_utc']=stamp();row['last_open_descriptors']=count
   except (OSError,StopIteration):pass
  stop.wait(0.5)

gates=[('lib',['cargo','+1.98.1','test','--workspace','--lib','--locked','-j1']),('check',['cargo','+1.98.1','check','--workspace','--all-targets','--locked','-j1']),('cargo-mode',['python3','scripts/cargo_mode.py','verify','+1.98.1']),('tree',['cargo','+1.98.1','tree','--locked']),('metadata',['cargo','+1.98.1','metadata','--locked','--format-version','1','--filter-platform','x86_64-unknown-linux-gnu'])]
results=[]
for name,cmd in gates:
 args=['nice','-n','10',*cmd];prefix='linux-final-fd-'+name;result={'command':args,'cwd':str(r),'environment':{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1'},'source_sha':sha,'started_utc':stamp(),'scope':inputs['scope'],'actual_exit_code':None};start=time.monotonic()
 with (out/(prefix+'.stdout.log')).open('wb') as stdout,(out/(prefix+'.stderr.log')).open('wb') as stderr:
  p=subprocess.Popen(args,cwd=r,env=env,stdout=stdout,stderr=stderr,preexec_fn=child_limits);result['owned_pid']=p.pid;result['host_nofile_before']=list(host_nofile);result['child_nofile_configured']=[child_soft,host_nofile[1]];result['resource_scope']='Only launched process and descendants inherit raised soft limit; hard limit/system/user configuration unchanged.';descriptor_record={};observer_stop=threading.Event();observer=threading.Thread(target=observe_descriptors,args=(p.pid,observer_stop,descriptor_record),daemon=True);observer.start();save(prefix+'.result.json',result);print('START '+name+' PID '+str(p.pid)+' '+result['started_utc'],flush=True);result['actual_exit_code']=p.wait();observer_stop.set();observer.join(timeout=3);result['descriptor_observation']=descriptor_record
 result['finished_utc']=stamp();result['elapsed_seconds']=round(time.monotonic()-start,3);result['final_source_sha']=git('rev-parse','HEAD');result['tracked_diff_after']=git('diff','HEAD','--name-only').splitlines();result['stdout_sha256']=hashlib.sha256((out/(prefix+'.stdout.log')).read_bytes()).hexdigest();result['stderr_sha256']=hashlib.sha256((out/(prefix+'.stderr.log')).read_bytes()).hexdigest();save(prefix+'.result.json',result);results.append(result);print('FINISH '+name+' exit '+str(result['actual_exit_code'])+' elapsed '+str(result['elapsed_seconds']),flush=True)
 if result['actual_exit_code']!=0 or result['final_source_sha']!=sha or result['tracked_diff_after']:break
changes=[]
for row in inventory:
 p=r/row['path'];h=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
 if h!=row['sha256']:changes.append({'path':row['path'],'before':row['sha256'],'after':h})
summary={'recorded_utc':stamp(),'source_sha':sha,'final_source_sha':git('rev-parse','HEAD'),'gates':results,'all_requested_gates_completed':len(results)==len(gates),'all_completed_exit_zero':all(x['actual_exit_code']==0 for x in results),'tracked_diff_after':git('diff','HEAD','--name-only').splitlines(),'input_hash_changes':changes,'scope':inputs['scope']}
if len(results)==len(gates) and results[-1]['actual_exit_code']==0:
 m=json.loads((out/'linux-final-fd-metadata.stdout.log').read_text());active=set(x['id'] for x in m['resolve']['nodes']);families={};paths=[]
 for p in m['packages']:
  s=p.get('source')
  if not s:paths.append({'name':p['name'],'version':p['version'],'manifest_path':p['manifest_path'],'active_in_default_resolve':p['id'] in active})
  else:
   match=re.search(r'https://github.com/(?:merely-made|servo)/(mere|genet|knot-editor|woodshed|wgpu-graft|wgpu-weld|wgpu-scry|servo)(?:[.?/#]|$)',s)
   if match:families.setdefault(match.group(1),{}).setdefault(s,[]).append({'name':p['name'],'version':p['version'],'active_in_default_resolve':p['id'] in active})
 save('linux-final-fd-family-provenance.json',{'metadata_sha256':results[-1]['stdout_sha256'],'metadata_packages':len(m['packages']),'active_resolve_nodes':len(active),'families':families,'path_packages':paths,'scope':'Filtered Linux default-feature metadata. Source membership/compile proof does not establish foreign native producer support.'})
save('linux-final-fd-summary.json',summary);print(json.dumps(summary,indent=2),flush=True)
sys.exit(0 if summary['all_requested_gates_completed'] and summary['all_completed_exit_zero'] and not changes and not summary['tracked_diff_after'] else 1)
