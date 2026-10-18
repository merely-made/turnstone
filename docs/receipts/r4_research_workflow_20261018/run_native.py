#!/usr/bin/env python3
"""One R4 task across real origin and resident exit/restart, using ordinary UI."""
import argparse
import datetime
import errno
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import socket
import subprocess
import sys
import time
import uuid

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
R3 = ROOT / 'docs/receipts/r3_scoped_answers_20261017'
shared = runpy.run_path(str(R3 / 'run_native.py'))
sha, wait_file, LAUNCHER = shared['sha'], shared['wait_file'], shared['LAUNCHER']

def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for key in ['binary', 'resident', 'client-tests', 'model', 'supplier', 'output']:
        parser.add_argument('--' + key, type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=600)
    args = parser.parse_args()
    out = args.output.resolve()
    if sys.platform != 'darwin' or out.exists() or out.is_relative_to(ROOT):
        parser.error('requires macOS and a new output directory outside the repo')
    out.mkdir(parents=True)
    frozen = out / 'frozen'; frozen.mkdir()
    for source, name in [(args.binary, 'turnstone'), (args.resident, 'inference_route'), (args.client_tests, 'turnstone-tests')]:
        shutil.copy2(source, frozen / name)
    for name in ['seed.scn', 'reopen.scn', 'fixture_server.py', 'run_native.py']:
        shutil.copy2(HERE / name, frozen / name)
    files = subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','scenarios','Cargo.toml','Cargo.lock'], cwd=ROOT, text=True).splitlines()
    sources = {f: sha(ROOT/f) for f in sorted(set(files)) if (ROOT/f).is_file()}
    supplier = args.supplier.resolve()
    supplier_files = ['ports/djinn/tests/inference_route.rs', 'ports/djinn/tests/common/mod.rs', 'Cargo.lock', 'Cargo.toml']
    write(out/'inputs.json', {'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(), 'source_sha256':sources, 'binary_sha256':sha(frozen/'turnstone'), 'client_tests_sha256':sha(frozen/'turnstone-tests'), 'resident_sha256':sha(frozen/'inference_route'), 'supplier_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=supplier,text=True).strip(), 'supplier_source_sha256':{f:sha(supplier/f) for f in supplier_files}, 'model':json.loads((args.model/'verified.json').read_text()), 'scenario_sha256':{name:sha(frozen/name) for name in ['seed.scn','reopen.scn','fixture_server.py','run_native.py']}, 'launcher_sha256':sha(LAUNCHER), 'r3_runner_sha256':sha(R3/'run_native.py'), 'os':subprocess.check_output(['sw_vers'],text=True)})
    control = out/'control'; control.mkdir()
    resident_dir = out/'resident'; resident_dir.mkdir()
    endpoint = '/tmp/turnstone-r4-'+str(uuid.uuid4())+'.sock'
    (resident_dir/'endpoint.txt').write_text(endpoint)
    logs = []
    fixture = resident = None
    origin = None
    runs = []
    awake = subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t',str(2*args.timeout+180)])
    def log(name):
        stream=(out/name).open('w');logs.append(stream);return stream
    def origin_start(phase):
        nonlocal fixture, origin
        ready=out/f'origin-ready-{phase}.json'
        command=[sys.executable,str(frozen/'fixture_server.py'),'--output',str(out/'fixture'),'--ready-file',str(ready)]
        if origin: command += ['--port',str(origin['port'])]
        fixture=subprocess.Popen(command,stdout=log(f'origin-{phase}.log'),stderr=subprocess.STDOUT)
        wait_file(ready,fixture,10)
        observed=json.loads(ready.read_text())
        if origin: assert origin==observed, 'origin must restart at the same address'
        origin=observed
        write(out/f'origin-process-{phase}.json', {'pid':fixture.pid,'origin':origin})
    def origin_stop(phase):
        fixture.terminate();fixture.wait(timeout=5)
        with socket.socket() as sock: code=sock.connect_ex((origin['host'],int(origin['port'])))
        assert code==errno.ECONNREFUSED
        write(out/f'origin-stop-{phase}.json',{'pid':fixture.pid,'exit_code':fixture.returncode,'exited':True,'tcp_refused':True,'connect_errno':code})
        shutil.copy2(out/'fixture/requests.json',out/f'requests-at-stop-{phase}.json')
        (control/f'origin-stopped-{phase}').write_text('origin exited; TCP refused\n')
    def resident_start(phase):
        nonlocal resident
        assert resident is None or resident.poll()==0
        observation=out/('seed/04_A1_search_read_no_origin_no_resident.saved-sources.json' if phase==0 else 'reopen/15_cold_reader_before_resident_restart.saved-sources.json')
        assert observation.is_file(), 'read source before starting resident'
        with socket.socket() as sock: origin_code=sock.connect_ex((origin['host'],int(origin['port'])))
        with socket.socket(socket.AF_UNIX) as sock: resident_code=sock.connect_ex(endpoint)
        assert origin_code==errno.ECONNREFUSED and resident_code in [errno.ECONNREFUSED,errno.ENOENT]
        write(out/f'resident-start-{phase}.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'prior_resident_exited':resident is None or resident.poll()==0,'origin_refused_before_start':True,'resident_endpoint_absent_before_start':True,'source_read_observation':str(observation.relative_to(out)),'source_read_sha256':sha(observation)})
        env=os.environ.copy();env.update(ESP_SMOLLM2_DIR=str(args.model.resolve()),DJINN_HEADED_DIR=str(resident_dir),DJINN_HEADED_ROOT=str(out/'private-resident'),DJINN_HEADED_PHASE=str(phase),TMPDIR='/tmp',RUST_LOG='warn')
        resident=subprocess.Popen([str(frozen/'inference_route'),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],env=env,cwd=supplier,stdout=log(f'resident-{phase}.log'),stderr=subprocess.STDOUT)
        wait_file(resident_dir/f'ready-{phase}.json',resident,180)
        if phase==0:
            probe=env.copy();probe.update(GRAPHSHELL_APP_ENDPOINT=endpoint,TURNSTONE_R3_PROBE_RECEIPT=str(out/'wrong-owner-probe.json'))
            subprocess.run([str(frozen/'turnstone-tests'),'inference_service::tests::admitted_research_route_refuses_wrong_owner','--ignored','--exact','--nocapture'],env=probe,cwd=ROOT,stdout=log('wrong-owner-probe.log'),stderr=subprocess.STDOUT,check=True,timeout=30)
        (control/f'resident-started-{phase}').write_text('resident accepting\n')
    def resident_stop(phase):
        (resident_dir/('restart.request' if phase==0 else 'stop.request')).touch()
        resident.wait(timeout=30);assert resident.returncode==0
        with socket.socket(socket.AF_UNIX) as sock: code=sock.connect_ex(endpoint)
        assert code in [errno.ECONNREFUSED,errno.ENOENT]
        write(out/f'resident-stop-{phase}.json',{'pid':resident.pid,'exit_code':resident.returncode,'exited':True,'endpoint_unreachable':True,'connect_errno':code})
    try:
        origin_start(0)
        for ordinal,phase in enumerate(['seed','reopen']):
            assert all(sha(ROOT/f)==digest for f,digest in sources.items()), 'source drift'
            assert all(sha(supplier/f)==digest for f,digest in json.loads((out/'inputs.json').read_text())['supplier_source_sha256'].items()), 'supplier fixture drift'
            directory=out/phase
            env={'TURNSTONE_ROOT':str(out/'profile'),'TURNSTONE_CAPTURE_DIR':str(directory),'TURNSTONE_OFFLINE':str(ordinal),'TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(out/'themes.json'),'R4_ORIGIN':origin['base_url'],'R4_CONTROL':str(control),'R4_RESIDENT':str(resident_dir),'GRAPHSHELL_APP_ENDPOINT':endpoint,'MERE_ROOT':str(out/'mere'),'XDG_DATA_HOME':str(out/'data'),'XDG_CONFIG_HOME':str(out/'config'),'XDG_CACHE_HOME':str(out/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
            command=[sys.executable,str(LAUNCHER),'--binary',str(frozen/'turnstone'),'--prefix','TURNSTONE','--scenario',str(frozen/(phase+'.scn')),'--output',str(directory),'--timeout',str(args.timeout)]
            for key,value in env.items():command += ['--env',key+'='+value]
            write(out/(phase+'-command.json'),command)
            with socket.socket(socket.AF_UNIX) as sock: endpoint_code=sock.connect_ex(endpoint)
            assert endpoint_code in [errno.ECONNREFUSED,errno.ENOENT]
            if ordinal:
                assert resident.poll()==0 and fixture.poll() is not None
                with socket.socket() as sock: assert sock.connect_ex((origin['host'],int(origin['port'])))==errno.ECONNREFUSED
            write(out/(phase+'-start.json'),{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'resident_endpoint_absent':True,'cold_reopen':bool(ordinal),'origin_process_exited':bool(ordinal)})
            app=subprocess.Popen(command,cwd=ROOT,stdout=log('launch-'+phase+'.log'),stderr=subprocess.STDOUT)
            seen=set();deadline=time.monotonic()+args.timeout+30
            while app.poll() is None and time.monotonic()<deadline:
                for marker,operation in [('stop-origin-0',lambda:origin_stop(0)),('start-resident-0',lambda:resident_start(0)),('start-origin-1',lambda:origin_start(1)),('stop-origin-1',lambda:origin_stop(1)),('start-resident-1',lambda:resident_start(1))]:
                    if marker not in seen and (control/marker).exists() and (ordinal==0)==(marker!='start-resident-1'):
                        seen.add(marker);operation()
                        if marker=='start-origin-1':(control/'origin-started-1').write_text('same origin restarted\n')
                time.sleep(.1)
            if app.poll() is None:app.terminate();app.wait(timeout=5);raise TimeoutError(phase)
            result=json.loads((directory/'process.json').read_text());runs.append({'phase':phase,'launcher_exit':app.returncode,'process':result});write(out/'runs.json',runs)
            assert app.returncode==0 and result['success'], phase+' headed run failed'
            if ordinal==0:
                shutil.copytree(out/'profile/sessions',out/'saved-before-cold')
            inspect_env=os.environ.copy();inspect_env.update(TURNSTONE_R4_PROFILE=str(out/'profile'),TURNSTONE_R4_OBSERVATION=str(out/'seed/14_old_A1_after_A2_and_scope_change.saved-sources.json'),TURNSTONE_R4_STORE_RECEIPT=str(out/(phase+'-store-inspection.json')))
            subprocess.run([str(frozen/'turnstone-tests'),'trail_memory::research_owner_tests::independently_inspect_exited_research_profile','--ignored','--exact','--nocapture'],env=inspect_env,cwd=ROOT,stdout=log(phase+'-store-inspection.log'),stderr=subprocess.STDOUT,check=True,timeout=30)
            resident_stop(ordinal)
        shutil.copy2(out/'fixture/requests.json',out/'requests-final.json')
        assert (out/'requests-at-stop-1.json').read_bytes()==(out/'requests-final.json').read_bytes()
        assert all(sha(ROOT/f)==digest for f,digest in sources.items())
        write(out/'completion.json',{'result':'pass','origin_and_resident_exited_before_cold_app':True,'distinct_resident_pids':json.loads((out/'resident-stop-0.json').read_text())['pid']!=json.loads((out/'resident-stop-1.json').read_text())['pid']})
    finally:
        for proc in [resident,fixture,awake]:
            if proc and proc.poll() is None:
                proc.terminate()
                try:proc.wait(timeout=10)
                except subprocess.TimeoutExpired:proc.kill();proc.wait(timeout=5)
        for stream in logs:stream.close()
if __name__=='__main__':main()
