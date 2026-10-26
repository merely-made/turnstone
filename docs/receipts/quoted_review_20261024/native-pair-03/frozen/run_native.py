#!/usr/bin/env python3
"""Actual mixed published corpora: lexical/fused native pairs and cold stores."""
import argparse, datetime, errno, hashlib, json, os, shutil, signal, socket, subprocess, sys, time, uuid
from pathlib import Path
import blake3
HERE=Path(__file__).resolve().parent
ROOT=Path('/Users/markik/Code/worktrees/turnstone-cross-source-answers-20261022')
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def read(path):return json.loads(path.read_text())
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def wait_file(path,process,seconds):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  if path.is_file() and path.stat().st_size:return
  if process.poll() is not None:raise RuntimeError(f'{process.args[0]} exited {process.returncode}: {path}')
  time.sleep(.1)
 raise TimeoutError(str(path))
def objects(value):
 if isinstance(value,dict):
  yield value
  for item in value.values():yield from objects(item)
 elif isinstance(value,list):
  for item in value:yield from objects(item)
def source_paths(package):
 from urllib.parse import urlparse,unquote
 values=[package['clip']['source']['entity']]+[v['source'] for v in package['clip']['anchors']]
 return [Path(unquote(urlparse(v).path)) for v in values if v.startswith('file:')]
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['binary','resident','supplier','eidetic','model','embedding','embedding-artifacts','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--qwen',action='store_true')
 p.add_argument('--qwen3',action='store_true')
 p.add_argument('--qwen15',action='store_true')
 p.add_argument('--mode',choices=['both','lexical','fused'],default='both')
 a=p.parse_args();out=a.output.resolve();modes=['lexical','fused'] if a.mode=='both' else [a.mode]
 for name,digest in read(HERE/'auxiliary-inputs.json').items():assert sha(HERE/name)==digest,('auxiliary input changed',name)
 if sys.platform!='darwin' or out.exists() or out.is_relative_to(ROOT):p.error('new macOS output directory outside repository required')
 out.mkdir(parents=True);frozen=out/'frozen';frozen.mkdir();streams=[];app=resident=origin=None;launch_record=None
 for source,name in [(a.binary,'turnstone'),(a.resident,'inference_route')]:shutil.copy2(source,frozen/name)
 for name in ['run_native.py','run_macos_scenario.py','fixture_server.py','warm.scn','cold.scn','cases.json','case-freeze.json','verify_native.py','verify_probe.py','product-baseline.json','knot.mereclip','redshank.mereclip','answer_contract.rs','candidate_review.rs','verify_contract_inputs.py','verify_structured_output.py','verify_wire_defaults.py','verify_native_defaults.py','verify_native_defaults_original.py','verify_native_bindings.py','verify_native_facts.py','measurement-contract.json','optimized-runtime.toml','auxiliary-inputs.json']:shutil.copy2(HERE/name,frozen/name)
 cases=read(frozen/'cases.json');questions={k:cases[k] for k in ['asks','rankings']};questions['claim_support']=True;write(frozen/'questions.json',questions)
 model=read(a.model/'verified.json');embedding=read(a.embedding_artifacts)
 for n,size,digest in model['artifacts']:assert (a.model/n).stat().st_size==size and sha(a.model/n)==digest
 for n,meta in embedding['artifacts'].items():assert (a.embedding/n).stat().st_size==meta['bytes'] and sha(a.embedding/n)==meta['sha256']
 paths=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).splitlines()
 source_hashes={n:sha(ROOT/n) for n in sorted(set(paths)) if (ROOT/n).is_file()}
 baseline=read(frozen/'product-baseline.json');assert baseline['binary_sha256']==sha(frozen/'turnstone') and baseline['source_sha256']==source_hashes,'shipping product/source differs from its qualified build'
 supplier_paths=['Cargo.toml','Cargo.lock','ports/djinn/tests/inference_route.rs','ports/djinn/tests/common/mod.rs','ports/djinn/tests/common/inference_door.rs','ports/distillery/src/inference.rs','ports/distillery/src/inference/answer_contract.rs','ports/distillery/src/inference/candidate_review.rs','ports/distillery/src/inference/recall.rs','ports/distillery/src/inference/recall_embedding.rs','ports/distillery/src/inference/durable.rs','ports/distillery/src/inference_protocol/recall.rs']
 inputs={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'turnstone_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'supplier_base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.supplier,text=True).strip(),'source_sha256':source_hashes,'supplier_source_sha256':{n:sha(a.supplier/n) for n in supplier_paths},'binary_sha256':sha(frozen/'turnstone'),'resident_sha256':sha(frozen/'inference_route'),'generation_model':model,'embedding_model':embedding,'os':subprocess.check_output(['sw_vers'],text=True),'frozen_sha256':{f.name:sha(f) for f in frozen.iterdir()}}
 inputs['eidetic_base']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.eidetic,text=True).strip()
 eidetic_paths=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','crates/eidetic-core/src','crates/eidetic-core/Cargo.toml','Cargo.toml','Cargo.lock'],cwd=a.eidetic,text=True).splitlines()
 inputs['eidetic_source_sha256']={n:sha(a.eidetic/n) for n in sorted(set(eidetic_paths)) if (a.eidetic/n).is_file()}
 write(out/'inputs.json',inputs)
 packages={key:read(frozen/cases['corpus'][key]['package']) for key in ['knot','note']};absent=[x for package in packages.values() for x in source_paths(package)];assert absent and all(not x.exists() for x in absent)
 def log(name):
  f=(out/name).open('w');streams.append(f);return f
 awake=subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t','3600'])
 try:
  for mode in modes:
   lane=out/mode;lane.mkdir();control=lane/'control';control.mkdir();rdir=lane/'resident';rdir.mkdir();wallet=lane/'wallet';wallet.mkdir();(wallet/'fixture-marker').write_text('retained-embedding-20261020\n')
   transfers={};addresses={}
   for key,package in packages.items():
    transfers[key]=lane/(key+'.mereclip');shutil.copy2(frozen/cases['corpus'][key]['package'],transfers[key])
    wire=json.dumps({'clip':package['clip'],'private_backing':[]},ensure_ascii=False,separators=(',',':')).encode();addresses[key]='mere://clip/'+blake3.blake3(wire).hexdigest()
   ready=lane/'origin-ready.json';origin=subprocess.Popen([sys.executable,str(frozen/'fixture_server.py'),'--cases',str(frozen/'cases.json'),'--output',str(lane/'origin'),'--ready-file',str(ready)],stdout=log(mode+'-origin.log'),stderr=subprocess.STDOUT);wait_file(ready,origin,10);address=read(ready)
   def refused():
    with socket.socket() as sock:return sock.connect_ex((address['host'],address['port']))==errno.ECONNREFUSED
   endpoint='/tmp/mr-'+uuid.uuid4().hex+'.sock';(rdir/'endpoint.txt').write_text(endpoint);phases=[]
   for phase,name in enumerate(['warm','cold']):
    assert all(not x.exists() for x in absent)
    if phase:assert origin.poll() is not None and refused() and all(not x.exists() for x in transfers.values())
    env=os.environ.copy();env.pop('DJINN_HEADED_EMBEDDING_DIR',None);env.pop('DJINN_CROSS_SOURCE_QWEN_DIR',None);env.pop('DJINN_CROSS_SOURCE_QWEN3',None);env.pop('DJINN_CROSS_SOURCE_QWEN15',None)
    if a.qwen or a.qwen3 or a.qwen15:env['DJINN_CROSS_SOURCE_QWEN_DIR']=str(a.model.resolve())
    if a.qwen3:env['DJINN_CROSS_SOURCE_QWEN3']='1'
    if a.qwen15:env['DJINN_CROSS_SOURCE_QWEN15']='1'
    env.update(ESP_SMOLLM2_DIR=str(a.model.resolve()),DJINN_HEADED_DIR=str(rdir),DJINN_HEADED_ROOT=str(lane/'private-resident'),DJINN_HEADED_PHASE=str(phase),DJINN_HEADED_CLIP_CUSTODY=str(wallet),DJINN_HEADED_MIXED_QUERIES=str(frozen/'questions.json'),TMPDIR='/tmp',RUST_LOG='warn,distillery::inference::gpu=debug')
    if mode=='fused':env['DJINN_HEADED_EMBEDDING_DIR']=str(a.embedding.resolve())
    resident=subprocess.Popen([str(frozen/'inference_route'),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],cwd=a.supplier,env=env,stdout=log(f'{mode}-resident-{phase}.log'),stderr=subprocess.STDOUT);wait_file(rdir/f'ready-{phase}.json',resident,2400 if a.qwen15 else 240)
    env={'TURNSTONE_ROOT':str(lane/'profile'),'TURNSTONE_OFFLINE':str(phase),'TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(lane/'themes.json'),'GRAPHSHELL_APP_ENDPOINT':endpoint,'MR_ORIGIN':address['base_url'],'MR_CONTROL':str(control),'MR_RESIDENT':str(rdir),'MR_CAPTURE_TEXT':cases['corpus']['capture']['text'],'MERE_ROOT':str(lane/'mere'),'XDG_DATA_HOME':str(lane/'data'),'XDG_CONFIG_HOME':str(lane/'config'),'XDG_CACHE_HOME':str(lane/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
    for key in ['knot','note']:env.update({f'MR_{key.upper()}':str(transfers[key]),f'MR_{key.upper()}_ADDRESS':addresses[key],f'MR_{key.upper()}_EXPORT':str(lane/(key+'-cold-export.mereclip'))})
    for case in cases['asks']:
     env['MR_'+case['id'].upper()+'_QUESTION']=case['question']
     env['MR_'+case['id'].upper()+'_REOPEN']=case['question'][:70]
    command=[sys.executable,str(frozen/'run_macos_scenario.py'),'--binary',str(frozen/'turnstone'),'--prefix','TURNSTONE','--scenario',str(frozen/(name+'.scn')),'--output',str(lane/name),'--timeout',str(4200 if a.qwen15 else 1200)]
    for key,value in env.items():command+=['--env',key+'='+value]
    write(lane/(name+'-command.json'),command);launch_record=lane/name/'launch.json';app=subprocess.Popen(command,cwd=ROOT,stdout=log(f'{mode}-{name}-launcher.log'),stderr=subprocess.STDOUT);deadline=time.monotonic()+(4230 if a.qwen15 else 1230)
    while app.poll() is None:
     if time.monotonic()>deadline:raise TimeoutError(mode+' '+name)
     if resident.poll() is not None:raise RuntimeError(f'{mode} resident exited {resident.returncode}; inspect its log')
     if not phase and (control/'selecting-sources').exists():
      expected=[]
      for key in ['capture','knot','note','control']:
       expected.append('blake3:'+blake3.blake3(cases['corpus'][key]['text'].encode()).hexdigest())
      for workspace in (lane/'profile/sessions').glob('*/workspace.json'):
       try:values=list(objects(read(workspace)))
       except (FileNotFoundError,json.JSONDecodeError):continue
       for value in values:
        if 'chosen' not in value or 'view' not in value or 'session' not in value:continue
        hashes={target['text_hash'] for target in value['chosen']}
        for count in range(1,5):
         ack=control/f'selected-{count}.json'
         if not ack.exists() and len(value['chosen'])==count and hashes==set(expected[:count]):write(ack,{'condition':'native selection committed by the app; exact expected text hashes','source':value})
     for workspace in (lane/'profile/sessions').glob('*/workspace.json'):
      try:values=list(objects(read(workspace)))
      except (FileNotFoundError,json.JSONDecodeError):continue
      for value in values:
       if 'chosen' not in value or 'view' not in value or 'session' not in value:continue
       hashes={target['text_hash'] for target in value['chosen']}
       def expected(keys):return {'blake3:'+blake3.blake3(cases['corpus'][key]['text'].encode()).hexdigest() for key in keys}
       ack=control/('cold-scope.json' if phase else 'reduced-scope.json')
       if (phase or (control/'reducing-scope').exists()) and not ack.exists() and hashes==expected(['capture','knot','note'] if phase else ['capture','knot']):write(ack,value)
     if not phase and (control/'stop-origin').exists() and not (control/'origin-stopped').exists():
      origin.terminate();origin.wait(timeout=5);assert refused();shutil.copy2(lane/'origin/requests.json',lane/'requests-at-stop.json');write(lane/'origin-stop.json',{'pid':origin.pid,'tcp_refused':True,'exited':True});(control/'origin-stopped').write_text('origin exited and TCP refused\n')
     try:live=read(rdir/f'live-runs-{phase}.json')
     except (FileNotFoundError,json.JSONDecodeError):live=None
     failed=[r for r in (live or {}).get('runs',[]) if r['state']=='failed']
     if failed:write(lane/(name+'-failure.json'),failed);raise RuntimeError(json.dumps(failed))
     time.sleep(.1)
    process=read(lane/name/'process.json');assert app.returncode==0 and process['success'],mode+' '+name
    audit_started=time.monotonic();(rdir/('restart.request' if phase==0 else 'stop.request')).touch();resident.wait(timeout=300);assert resident.returncode==0
    with socket.socket(socket.AF_UNIX) as sock:assert sock.connect_ex(endpoint)!=0
    phases.append({'snapshot_shutdown_seconds':time.monotonic()-audit_started,'snapshot_shutdown_limit_seconds':300,'phase':phase,'resident_pid':resident.pid,'product_pids':read(lane/name/'launch.json')['product_pids'],'clean_shutdown':True,'endpoint_unreachable':True,'origin_refused':refused(),'transfer_absent':all(not x.exists() for x in transfers.values()),'producer_paths_absent':[str(x) for x in absent]});write(lane/'phases.json',phases)
    if phase==0:
     for f in transfers.values():f.unlink()
    assert all(sha(ROOT/n)==h for n,h in source_hashes.items()),'compiled source drift'
   # Onward export was qualified in the prior slice; this gate checks retained answers.
   assert (lane/'requests-at-stop.json').read_bytes()==(lane/'origin/requests.json').read_bytes();shutil.copy2(lane/'origin/requests.json',lane/'requests-final.json')
   write(lane/'completion.json',{'result':'pass','addresses':addresses,'origin_and_transfer_absent_before_cold':True,'retained_answers':True})
  assert all(sha(a.supplier/n)==h for n,h in inputs['supplier_source_sha256'].items())
  assert all(sha(a.eidetic/n)==h for n,h in inputs['eidetic_source_sha256'].items())
  assert all(sha(frozen/n)==h for n,h in inputs['frozen_sha256'].items())
  write(out/'completion.json',{'result':'pass','modes':modes})
 finally:
  if launch_record and launch_record.exists():
   launch=read(launch_record);paths={launch['executable'],str(Path(launch['executable']).resolve())}
   for pid in launch['product_pids']:
    r=subprocess.run(['ps','-p',str(pid),'-o','command='],capture_output=True,text=True)
    if r.returncode==0 and any(r.stdout.strip()==v or r.stdout.strip().startswith(v+' ') for v in paths):os.kill(pid,signal.SIGTERM)
  for process in [app,resident,origin,awake]:
   if process and process.poll() is None:
    process.terminate()
    try:process.wait(timeout=10)
    except subprocess.TimeoutExpired:process.kill();process.wait(timeout=5)
  for f in streams:f.close()
if __name__=='__main__':main()
