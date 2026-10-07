from pathlib import Path
import subprocess,os,json,hashlib,datetime,time,sys,tomllib
r=Path('/home/markik/Code/repos/turnstone');out=r/'docs/receipts/browser_family_20261007';target=Path('/home/markik/Code/target');sha='aafa78df71170c83df9838c6fc207117bedc45ec'
out.mkdir(parents=True,exist_ok=True)
def stamp():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def git(*a):return subprocess.check_output(['git','-C',str(r),*a],text=True).strip()
files=['linux-baseline-inputs.json','linux-baseline-Cargo.toml','linux-baseline-Cargo.lock','linux-baseline.stdout.log','linux-baseline.stderr.log','linux-baseline-result.json']
for f in files:assert not (out/f).exists(),'Preserving '+f
assert git('branch','--show-current')=='main' and git('rev-parse','HEAD')==sha
assert not git('diff','HEAD','--name-only')
assert (target/'CACHEDIR.TAG').read_text().startswith('Signature: 8a477f597d28d172789f06886806bc55')
owners=[];names={'cargo','rustc','rustdoc','turnstone','cc','c++','clang','clang++','gcc','g++','ld','ld.lld','lld','cmake','ninja','make'}
for p in Path('/proc').iterdir():
 if p.name.isdigit():
  try:
   if (p/'comm').read_text().strip() in names:owners.append(int(p.name))
  except OSError:pass
assert not owners,owners
lock=subprocess.run(['fuser',str(target/'debug/.cargo-lock')],capture_output=True,text=True)
assert lock.returncode==1 and not lock.stdout and not lock.stderr
inventory=[]
for name in git('ls-files').splitlines():
 p=r/name
 if p.is_file():inventory.append({'path':name,'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
lockdata=tomllib.loads((r/'Cargo.lock').read_text())
selected=[{'name':p['name'],'version':p['version'],'source':p.get('source')} for p in lockdata['package'] if p.get('source','').startswith('git+')]
inputs={'recorded_utc':stamp(),'host':'thinkpad-l14-f','repo':str(r),'source_sha':sha,'source_tree':git('rev-parse','HEAD^{tree}'),'tracked_clean':True,'tracked_file_inventory':inventory,'selected_git_packages':selected,'toolchain':subprocess.check_output(['rustup','run','1.98.1','rustc','-Vv'],text=True),'target':str(target),'target_marker_sha256':hashlib.sha256((target/'CACHEDIR.TAG').read_bytes()).hexdigest(),'prelaunch_owners':owners,'prelaunch_lock_observer_exit':lock.returncode,'scope':'Pre-repin baseline default Linux workspace all-targets check, not browser native or AT proof.'}
(out/'linux-baseline-inputs.json').write_bytes((json.dumps(inputs,indent=2)+'\n').encode())
for n in ['Cargo.toml','Cargo.lock']:(out/('linux-baseline-'+n)).write_bytes((r/n).read_bytes())
args=['nice','-n','10','cargo','+1.98.1','check','--workspace','--all-targets','--locked','-j1'];env=os.environ.copy();env['CARGO_TARGET_DIR']=str(target);env['CARGO_BUILD_JOBS']='1'
result={'command':args,'cwd':str(r),'environment':{'CARGO_TARGET_DIR':str(target),'CARGO_BUILD_JOBS':'1'},'source_sha':sha,'started_utc':stamp(),'scope':inputs['scope'],'actual_exit_code':None}
start=time.monotonic()
with (out/'linux-baseline.stdout.log').open('wb') as stdout,(out/'linux-baseline.stderr.log').open('wb') as stderr:
 p=subprocess.Popen(args,cwd=r,env=env,stdout=stdout,stderr=stderr)
 result['owned_pid']=p.pid
 (out/'linux-baseline-result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode())
 print('Launched owned baseline PID '+str(p.pid)+'; '+json.dumps(args),flush=True)
 result['actual_exit_code']=p.wait()
result['finished_utc']=stamp();result['elapsed_seconds']=round(time.monotonic()-start,3);result['final_source_sha']=git('rev-parse','HEAD');result['tracked_diff_after']=git('diff','HEAD','--name-only').splitlines()
result['input_hash_changes']=[]
for row in inventory:
 p=r/row['path'];actual=hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None
 if actual!=row['sha256']:result['input_hash_changes'].append({'path':row['path'],'before':row['sha256'],'after':actual})
result['output_hashes']={n:hashlib.sha256((out/n).read_bytes()).hexdigest() for n in files if n!='linux-baseline-result.json'}
(out/'linux-baseline-result.json').write_bytes((json.dumps(result,indent=2)+'\n').encode())
print(json.dumps(result,indent=2),flush=True)
sys.exit(result['actual_exit_code'])
