#!/usr/bin/env python3
"""Native retained Knot/Redshank imports, fused recall and independent cold pairs.

Uses selected packages from qualified embedded producer flows, including the
fresh sentence-length Knot drag recorded in this packet.
Every new product profile starts empty. No source owner/model is copied in.
"""
import argparse, datetime, hashlib, json, os, shutil, signal, socket, subprocess, sys, time, uuid
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path, value): path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def wait_file(path, process, seconds):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if path.is_file() and path.stat().st_size: return
  if process.poll() is not None: raise RuntimeError(f'{process.args[0]} exited {process.returncode} before {path}')
  time.sleep(.1)
 raise TimeoutError(str(path))
def source_paths(package):
 from urllib.parse import urlparse, unquote
 values=[package['clip']['source']['entity']]+[a['source'] for a in package['clip']['anchors']]
 return [Path(unquote(urlparse(v).path)) for v in values if v.startswith('file:')]
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['binary','resident','supplier','model','embedding','embedding-artifacts','output']: p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();out=a.output.resolve()
 if sys.platform!='darwin' or out.exists() or out.is_relative_to(ROOT):p.error('requires a new macOS output directory outside the repository')
 out.mkdir(parents=True);frozen=out/'frozen';frozen.mkdir();streams=[];resident=app=None;launch_record=None
 for source,name in [(a.binary,'turnstone'),(a.resident,'inference_route')]:shutil.copy2(source,frozen/name)
 for name in ['run_native.py','run_macos_scenario.py','warm.scn','cold.scn','knot.mereclip','redshank.mereclip']:shutil.copy2(HERE/name,frozen/name)
 model=json.loads((a.model/'verified.json').read_text());embedding=json.loads(a.embedding_artifacts.read_text())
 for name,size,digest in model['artifacts']:assert (a.model/name).stat().st_size==size and sha(a.model/name)==digest
 for name,meta in embedding['artifacts'].items():assert (a.embedding/name).stat().st_size==meta['bytes'] and sha(a.embedding/name)==meta['sha256']
 sources={name:sha(ROOT/name) for name in subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).splitlines() if (ROOT/name).is_file()}
 supplier_paths=['Cargo.toml','Cargo.lock','ports/djinn/tests/inference_route.rs','ports/djinn/tests/common/mod.rs','ports/djinn/tests/common/inference_door.rs','ports/distillery/src/inference/recall.rs']
 inputs={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'turnstone_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_sha256':sources,'supplier_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.supplier,text=True).strip(),'supplier_source_sha256':{n:sha(a.supplier/n) for n in supplier_paths},'binary_sha256':sha(frozen/'turnstone'),'resident_sha256':sha(frozen/'inference_route'),'generation_model':model,'embedding_model':embedding,'os':subprocess.check_output(['sw_vers'],text=True),'frozen_sha256':{f.name:sha(f) for f in frozen.iterdir()}}
 write(out/'inputs.json',inputs)
 def log(name):
  stream=(out/name).open('w');streams.append(stream);return stream
 awake=subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t','2400'])
 try:
  for kind in ['knot','redshank']:
   lane=out/kind;lane.mkdir();control=lane/'resident';control.mkdir();wallet=lane/'wallet';wallet.mkdir();(wallet/'fixture-marker').write_text('retained-embedding-20261020\n')
   package=json.loads((frozen/(kind+'.mereclip')).read_text());assert len(package['payloads'])==1
   absent=source_paths(package);assert absent and all(not x.exists() for x in absent)
   transfer=lane/'selected.mereclip';shutil.copy2(frozen/(kind+'.mereclip'),transfer)
   # Import without private backing: the retained reference is the canonical selected record.
   wire=json.dumps({'clip':package['clip'],'private_backing':[]},ensure_ascii=False,separators=(',',':')).encode()
   import blake3
   address='mere://clip/'+blake3.blake3(wire).hexdigest()
   endpoint='/tmp/rc-'+uuid.uuid4().hex+'.sock';(control/'endpoint.txt').write_text(endpoint)
   phases=[]
   for phase,name in enumerate(['warm','cold']):
    assert all(not x.exists() for x in absent)
    if phase:assert not transfer.exists() and resident.poll()==0
    env=os.environ.copy();env.update(ESP_SMOLLM2_DIR=str(a.model.resolve()),DJINN_HEADED_EMBEDDING_DIR=str(a.embedding.resolve()),DJINN_HEADED_DIR=str(control),DJINN_HEADED_ROOT=str(lane/'private-resident'),DJINN_HEADED_PHASE=str(phase),DJINN_HEADED_CLIP_CUSTODY=str(wallet),TMPDIR='/tmp',RUST_LOG='warn')
    resident=subprocess.Popen([str(frozen/'inference_route'),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],cwd=a.supplier,env=env,stdout=log(f'{kind}-resident-{phase}.log'),stderr=subprocess.STDOUT)
    wait_file(control/f'ready-{phase}.json',resident,180)
    env={'TURNSTONE_ROOT':str(lane/'profile'),'TURNSTONE_OFFLINE':'1','TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(lane/'themes.json'),'TURNSTONE_WIDTH':'1200','TURNSTONE_HEIGHT':'800','GRAPHSHELL_APP_ENDPOINT':endpoint,'RC_PACKAGE':str(transfer),'RC_ADDRESS':address,'RC_TITLE':package['clip']['title'],'RC_QUESTION':('Repeat the retained sentence about the Pelican observatory exactly.' if kind=='knot' else 'Repeat both lines of the retained note about the embedded listener exactly, preserving the line break.'),'RC_RESIDENT':str(control),'RC_EXPORT':str(lane/'cold-export.mereclip'),'MERE_ROOT':str(lane/'mere'),'XDG_DATA_HOME':str(lane/'data'),'XDG_CONFIG_HOME':str(lane/'config'),'XDG_CACHE_HOME':str(lane/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
    command=[sys.executable,str(frozen/'run_macos_scenario.py'),'--binary',str(frozen/'turnstone'),'--prefix','TURNSTONE','--scenario',str(frozen/(name+'.scn')),'--output',str(lane/name),'--timeout','600']
    for key,value in env.items():command+=['--env',key+'='+value]
    write(lane/(name+'-command.json'),command)
    launch_record=lane/name/'launch.json'
    app=subprocess.Popen(command,cwd=ROOT,stdout=log(f'{kind}-{name}-launcher.log'),stderr=subprocess.STDOUT)
    deadline=time.monotonic()+630
    while app.poll() is None:
     if time.monotonic()>deadline:raise TimeoutError(kind+' '+name)
     try:live=json.loads((control/f'live-runs-{phase}.json').read_text())
     except (FileNotFoundError,json.JSONDecodeError):live=None
     failed=[run for run in (live or {}).get('runs',[]) if run['state']=='failed']
     if failed:write(lane/(name+'-resident-failure.json'),failed);raise RuntimeError('Resident refused the ask: '+json.dumps(failed))
     time.sleep(.1)
    process=json.loads((lane/name/'process.json').read_text());assert app.returncode==0 and process['success'],kind+' '+name+' failed'
    (control/('restart.request' if phase==0 else 'stop.request')).touch();resident.wait(timeout=60);assert resident.returncode==0
    with socket.socket(socket.AF_UNIX) as sock:assert sock.connect_ex(endpoint)!=0
    phases.append({'phase':phase,'resident_pid':resident.pid,'product_pids':json.loads((lane/name/'launch.json').read_text())['product_pids'],'clean_shutdown':True,'endpoint_unreachable':True,'transfer_absent':not transfer.exists(),'producer_paths_absent':[str(x) for x in absent]})
    write(lane/'phases.json',phases)
    if phase==0:transfer.unlink()
    assert all(sha(ROOT/n)==h for n,h in sources.items()),'compiled source drift'
   assert json.loads((lane/'cold-export.mereclip').read_text())==package
   assert len({x['resident_pid'] for x in phases})==2
   write(lane/'completion.json',{'result':'pass','source_and_transfer_absent_before_cold':True,'export_equal_original':True,'address':address})
  assert all(sha(a.supplier/n)==h for n,h in inputs['supplier_source_sha256'].items())
  assert all(sha(frozen/n)==h for n,h in inputs['frozen_sha256'].items())
  write(out/'completion.json',{'result':'pass','lanes':['knot','redshank']})
 finally:
  # LaunchServices products are not children of this driver. Clean only the
  # exact unique product recorded by this owned launch, including on refusal.
  if launch_record and launch_record.exists():
   launch=json.loads(launch_record.read_text());paths={launch['executable'],str(Path(launch['executable']).resolve())}
   for pid in launch['product_pids']:
    current=subprocess.run(['ps','-p',str(pid),'-o','command='],capture_output=True,text=True)
    if current.returncode==0 and any(current.stdout.strip()==p or current.stdout.strip().startswith(p+' ') for p in paths):os.kill(pid,signal.SIGTERM)
  if app and app.poll() is None:
   try:app.wait(timeout=5)
   except subprocess.TimeoutExpired:pass
  for process in [app,resident,awake]:
   if process and process.poll() is None:
    process.terminate()
    try:process.wait(timeout=10)
    except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
  for stream in streams:stream.close()
if __name__=='__main__':main()
