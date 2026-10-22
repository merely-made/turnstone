#!/usr/bin/env python3
"""Headed retained-source embedding recall through an independent cold pair.

Run outside the repository with final binaries. Model files are local admitted
inputs; neither this driver nor the resident downloads them.
"""
import argparse, datetime, errno, hashlib, json, os, runpy, shutil, socket, subprocess, sys, time, uuid
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
LAUNCHER=HERE/'run_macos_scenario.py'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def wait_file(path,process,seconds):
 deadline=time.monotonic()+seconds
 while time.monotonic()<deadline:
  if path.is_file() and path.stat().st_size:return
  if process.poll() is not None:raise RuntimeError(f'{process.args[0]} exited {process.returncode} before {path}')
  time.sleep(.1)
 raise TimeoutError(str(path))
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['binary','resident','supplier','model','embedding','embedding-artifacts','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--timeout',type=int,default=600);args=p.parse_args();out=args.output.resolve()
 if sys.platform!='darwin' or out.exists() or out.is_relative_to(ROOT):p.error('new macOS output directory outside the repository required')
 out.mkdir(parents=True);frozen=out/'frozen';frozen.mkdir();control=out/'control';control.mkdir();resident_dir=out/'resident';resident_dir.mkdir()
 for source,name in [(args.binary,'turnstone'),(args.resident,'inference_route')]:shutil.copy2(source,frozen/name)
 for name in ['seed.scn','reopen.scn','run_native.py','run_macos_scenario.py']:shutil.copy2(HERE/name,frozen/name)
 shutil.copy2(ROOT/'docs/receipts/r4_research_workflow_20261018/fixture_server.py',frozen/'fixture_server.py')
 model=json.loads((args.model/'verified.json').read_text());embedding=json.loads(args.embedding_artifacts.read_text())
 for name,size,digest in model['artifacts']:
  path=args.model/name;assert path.stat().st_size==size and sha(path)==digest,name
 for name,expected in embedding['artifacts'].items():
  path=args.embedding/name;assert path.stat().st_size==expected['bytes'] and sha(path)==expected['sha256'],name
 tracked=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','scenarios','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).splitlines()
 sources={name:sha(ROOT/name) for name in sorted(set(tracked)) if (ROOT/name).is_file()}
 supplier=args.supplier.resolve();supplier_paths=['Cargo.toml','Cargo.lock','ports/djinn/tests/inference_route.rs','ports/djinn/tests/common/mod.rs']
 write(out/'inputs.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'turnstone_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_sha256':sources,'supplier_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=supplier,text=True).strip(),'supplier_source_sha256':{name:sha(supplier/name) for name in supplier_paths},'binary_sha256':sha(frozen/'turnstone'),'resident_sha256':sha(frozen/'inference_route'),'generation_model':model,'embedding_model':embedding,'launcher_sha256':sha(LAUNCHER),'frozen_sha256':{f.name:sha(f) for f in frozen.iterdir()},'os':subprocess.check_output(['sw_vers'],text=True)})
 endpoint='/tmp/er-'+uuid.uuid4().hex+'.sock';(resident_dir/'endpoint.txt').write_text(endpoint)
 origin=resident=None;streams=[];runs=[]
 def log(name):
  stream=(out/name).open('w');streams.append(stream);return stream
 def refused():
  with socket.socket() as sock:return sock.connect_ex((address['host'],address['port']))==errno.ECONNREFUSED
 def resident_start(phase):
  nonlocal resident
  assert refused() and (resident is None or resident.poll()==0)
  expected=out/('seed/03_read_origin_and_resident_absent.saved-sources.json' if phase==0 else 'reopen/08_cold_reader_before_resident.saved-sources.json');assert expected.is_file()
  env=os.environ.copy();env.update(ESP_SMOLLM2_DIR=str(args.model.resolve()),DJINN_HEADED_EMBEDDING_DIR=str(args.embedding.resolve()),DJINN_HEADED_DIR=str(resident_dir),DJINN_HEADED_ROOT=str(out/'private-resident'),DJINN_HEADED_PHASE=str(phase),TMPDIR='/tmp',RUST_LOG='warn')
  resident=subprocess.Popen([str(frozen/'inference_route'),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],cwd=supplier,env=env,stdout=log(f'resident-{phase}.log'),stderr=subprocess.STDOUT)
  wait_file(resident_dir/f'ready-{phase}.json',resident,180)
  write(out/f'resident-start-{phase}.json',{'pid':resident.pid,'source_read_sha256':sha(expected),'origin_refused':True})
  (control/f'resident-started-{phase}').write_text('ready\n')
 def resident_stop(phase):
  (resident_dir/('restart.request' if phase==0 else 'stop.request')).touch();resident.wait(timeout=60);assert resident.returncode==0
  with socket.socket(socket.AF_UNIX) as sock:assert sock.connect_ex(endpoint) in [errno.ECONNREFUSED,errno.ENOENT]
  write(out/f'resident-stop-{phase}.json',{'pid':resident.pid,'exit_code':resident.returncode,'endpoint_unreachable':True})
 awake=subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t',str(2*args.timeout+180)])
 try:
  ready=out/'origin-ready.json';origin=subprocess.Popen([sys.executable,str(frozen/'fixture_server.py'),'--output',str(out/'origin'),'--ready-file',str(ready)],stdout=log('origin.log'),stderr=subprocess.STDOUT);wait_file(ready,origin,10);address=json.loads(ready.read_text())
  for phase,name in enumerate(['seed','reopen']):
   assert all(sha(ROOT/f)==h for f,h in sources.items()),'compiled source drift'
   env={'TURNSTONE_ROOT':str(out/'profile'),'TURNSTONE_OFFLINE':str(phase),'TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(out/'themes.json'),'ER_ORIGIN':address['base_url'],'ER_CONTROL':str(control),'ER_RESIDENT':str(resident_dir),'GRAPHSHELL_APP_ENDPOINT':endpoint,'MERE_ROOT':str(out/'mere'),'XDG_DATA_HOME':str(out/'data'),'XDG_CONFIG_HOME':str(out/'config'),'XDG_CACHE_HOME':str(out/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
   if phase:assert origin.poll() is not None and resident.poll()==0 and refused()
   command=[sys.executable,str(LAUNCHER),'--binary',str(frozen/'turnstone'),'--prefix','TURNSTONE','--scenario',str(frozen/(name+'.scn')),'--output',str(out/name),'--timeout',str(args.timeout)]
   for key,value in env.items():command+=['--env',key+'='+value]
   write(out/(name+'-command.json'),command)
   app=subprocess.Popen(command,cwd=ROOT,stdout=log(name+'-launcher.log'),stderr=subprocess.STDOUT)
   deadline=time.monotonic()+args.timeout+30;seen=set()
   while app.poll() is None and time.monotonic()<deadline:
    if not phase and 'stop-origin' not in seen and (control/'stop-origin').exists():
     seen.add('stop-origin');origin.terminate();origin.wait(timeout=5);assert refused();shutil.copy2(out/'origin/requests.json',out/'requests-at-stop.json');write(out/'origin-stop.json',{'pid':origin.pid,'exited':True,'tcp_refused':True});(control/'origin-stopped').write_text('origin exited\n')
    marker=f'start-resident-{phase}'
    if marker not in seen and (control/marker).exists():seen.add(marker);resident_start(phase)
    time.sleep(.1)
   if app.poll() is None:app.terminate();app.wait(timeout=5);raise TimeoutError(name)
   result=json.loads((out/name/'process.json').read_text());runs.append({'phase':phase,'launcher_pid':app.pid,'exit_code':app.returncode,'process':result});write(out/'runs.json',runs)
   assert app.returncode==0 and result['success'],name+' headed scenario failed'
   assert json.loads((out/name/'launch.json').read_text())['product_pids']
   resident_stop(phase)
  assert (out/'requests-at-stop.json').read_bytes()==(out/'origin/requests.json').read_bytes()
  shutil.copy2(out/'origin/requests.json',out/'requests-final.json')
  assert all(sha(ROOT/f)==h for f,h in sources.items())
  assert sha(LAUNCHER)==json.loads((out/'inputs.json').read_text())['launcher_sha256']
  assert all(sha(frozen/f)==h for f,h in json.loads((out/'inputs.json').read_text())['frozen_sha256'].items())
  assert all(sha(supplier/f)==h for f,h in json.loads((out/'inputs.json').read_text())['supplier_source_sha256'].items())
  write(out/'completion.json',{'result':'pass','both_processes_exit_before_cold':True,'origin_requests_unchanged_after_stop':True,'resident_pids':[json.loads((out/f'resident-stop-{p}.json').read_text())['pid'] for p in [0,1]],'new_cold_ask_used_reopened_projection':True})
 finally:
  for process in [resident,origin,awake]:
   if process and process.poll() is None:
    process.terminate()
    try:process.wait(timeout=10)
    except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
  for stream in streams:stream.close()
if __name__=='__main__':main()
