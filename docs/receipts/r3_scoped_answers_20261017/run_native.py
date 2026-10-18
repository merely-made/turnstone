#!/usr/bin/env python3
"""Freeze R3 inputs and drive two headed Turnstone processes against real Djinn.

The declared pinned supplier test owns an isolated resident, model library and
journal. Its restart handshake closes and reopens the resident composition.
The origin is terminated and TCP refusal confirmed before the first ask.
"""
import argparse
import datetime
import errno
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
LAUNCHER = ROOT / 'docs/receipts/graph_folds_20261016/run_macos_scenario.py'
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def wait_file(path, process, seconds):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if path.is_file() and path.stat().st_size: return
        if process.poll() is not None: raise RuntimeError(f'{process.args[0]} exited {process.returncode} before {path.name}')
        time.sleep(.1)
    raise TimeoutError(str(path))
def main():
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ['binary', 'resident', 'model', 'output', 'client-tests']: p.add_argument('--'+arg, type=Path, required=True)
    p.add_argument('--timeout', type=int, default=480)
    a = p.parse_args()
    out = a.output.resolve()
    if sys.platform != 'darwin' or out.exists() or out.is_relative_to(ROOT): p.error('requires macOS and a new output directory outside the repo')
    out.mkdir(parents=True)
    frozen = out/'frozen'; frozen.mkdir()
    binary = frozen/'turnstone'; shutil.copy2(a.binary, binary)
    resident_exe = frozen/'inference_route'; shutil.copy2(a.resident, resident_exe)
    client_tests=frozen/'turnstone-tests';shutil.copy2(getattr(a,'client_tests'),client_tests)
    for name in ['seed.scn', 'reopen.scn', 'fixture_server.py', 'run_native.py']: shutil.copy2(HERE/name, frozen/name)
    files = subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','scenarios','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).splitlines()
    sources = {f: sha(ROOT/f) for f in sorted(set(files)) if (ROOT/f).is_file()}
    inputs = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'head': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(), 'source_sha256': sources, 'binary_sha256': sha(binary), 'resident_sha256': sha(resident_exe), 'client_tests_sha256':sha(client_tests), 'model': json.loads((a.model/'verified.json').read_text()), 'fixture_sha256':sha(frozen/'fixture_server.py'), 'seed_sha256':sha(frozen/'seed.scn'), 'reopen_sha256':sha(frozen/'reopen.scn'), 'launcher_sha256':sha(LAUNCHER), 'os':subprocess.check_output(['sw_vers'],text=True)}
    (out/'inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
    fixture_log = (out/'fixture.log').open('w')
    fixture = subprocess.Popen([sys.executable,str(frozen/'fixture_server.py'),'--output',str(out/'fixture'),'--ready-file',str(out/'fixture-ready.json')],stdout=fixture_log,stderr=subprocess.STDOUT)
    control = out/'resident'; control.mkdir()
    (out/'control').mkdir()
    env = os.environ.copy(); env.update(ESP_SMOLLM2_DIR=str(a.model.resolve()), DJINN_HEADED_DIR=str(control), RUST_LOG='warn', TMPDIR='/tmp')
    resident_log = (out/'resident.log').open('w')
    resident = subprocess.Popen([str(resident_exe),'serve_trained_model_for_headed_turnstone','--ignored','--exact','--nocapture'],env=env,cwd=ROOT,stdout=resident_log,stderr=subprocess.STDOUT)
    awake = subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t',str(2*a.timeout+180)])
    origin_stopped = False
    runs=[]
    try:
        wait_file(out/'fixture-ready.json',fixture,10)
        origin=json.loads((out/'fixture-ready.json').read_text())
        wait_file(control/'ready-0.json',resident,180)
        endpoint=(control/'endpoint.txt').read_text()
        probe_env=env.copy();probe_env.update(GRAPHSHELL_APP_ENDPOINT=endpoint,TURNSTONE_R3_PROBE_RECEIPT=str(out/'wrong-owner-probe.json'))
        with (out/'wrong-owner-probe.log').open('w') as log:
            subprocess.run([str(client_tests),'inference_service::tests::admitted_research_route_refuses_wrong_owner','--ignored','--exact','--nocapture'],cwd=ROOT,env=probe_env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=30)
        for ordinal, phase in enumerate(['seed','reopen']):
            if ordinal:
                (control/'restart.request').touch()
                wait_file(control/'ready-1.json',resident,180)
            if any(sha(ROOT/f)!=digest for f,digest in sources.items()): raise RuntimeError('source changed during headed qualification')
            phase_dir=out/phase
            command=[sys.executable,str(LAUNCHER),'--binary',str(binary),'--prefix','TURNSTONE','--scenario',str(frozen/(phase+'.scn')),'--output',str(phase_dir),'--timeout',str(a.timeout)]
            variables={'TURNSTONE_ROOT':str(out/'profile'),'TURNSTONE_CAPTURE_DIR':str(phase_dir),'TURNSTONE_R3_FIXTURE_URL':origin['base_url'],'TURNSTONE_R3_STOP_ORIGIN':str(out/'control/stop-origin'),'TURNSTONE_R3_ORIGIN_STOPPED':str(out/'control/origin-stopped'),'TURNSTONE_OFFLINE':'1' if ordinal else '0','TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(out/'themes.json'),'GRAPHSHELL_APP_ENDPOINT':endpoint,'DJINN_HEADED_DIR':str(control),'MERE_ROOT':str(out/'mere'),'XDG_DATA_HOME':str(out/'data'),'XDG_CONFIG_HOME':str(out/'config'),'XDG_CACHE_HOME':str(out/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
            for key,value in variables.items(): command += ['--env',key+'='+value]
            (out/(phase+'-command.json')).write_text(json.dumps(command,indent=2)+'\n')
            with (out/('launch-'+phase+'.log')).open('w') as log:
                run=subprocess.Popen(command,stdout=log,stderr=subprocess.STDOUT,cwd=ROOT)
                deadline=time.monotonic()+a.timeout+30
                while run.poll() is None and time.monotonic()<deadline:
                    if not origin_stopped and (out/'control/stop-origin').exists():
                        fixture.terminate(); fixture.wait(timeout=5)
                        with socket.socket() as probe: code=probe.connect_ex((origin['host'],int(origin['port'])))
                        if code != errno.ECONNREFUSED: raise RuntimeError('origin still reachable')
                        origin_stopped=True
                        (out/'origin-stop.json').write_text(json.dumps({'exited':True,'exit_code':fixture.returncode,'tcp_refused':True,'connect_errno':code},indent=2)+'\n')
                        shutil.copy2(out/'fixture/requests.json',out/'requests-at-origin-stop.json')
                        (out/'control/origin-stopped').write_text('origin process exited; TCP refused\n')
                    time.sleep(.1)
                if run.poll() is None:
                    run.terminate(); run.wait(timeout=5); raise TimeoutError(phase)
            result=json.loads((phase_dir/'process.json').read_text()) if (phase_dir/'process.json').exists() else {}
            runs.append({'phase':phase,'launcher_exit':run.returncode,'process':result})
            (out/'runs.json').write_text(json.dumps(runs,indent=2)+'\n')
            if run.returncode or not result.get('success'): raise RuntimeError(f'{phase} headed run failed; see launch-{phase}.log')
        (control/'stop.request').touch(); resident.wait(timeout=30)
        if resident.returncode: raise RuntimeError('resident receipt host failed')
        if not origin_stopped: raise RuntimeError('origin was never stopped')
        shutil.copy2(out/'fixture/requests.json',out/'requests-final.json')
        if (out/'requests-at-origin-stop.json').read_bytes() != (out/'requests-final.json').read_bytes(): raise RuntimeError('origin requests changed after stop')
        if any(sha(ROOT/f)!=digest for f,digest in sources.items()): raise RuntimeError('source changed during qualification')
        print(json.dumps({'result':'headed-runs-complete','output':str(out)},indent=2))
    finally:
        for proc in [resident,fixture,awake]:
            if proc.poll() is None:
                proc.terminate()
                try: proc.wait(timeout=10)
                except subprocess.TimeoutExpired: proc.kill();proc.wait(timeout=5)
        fixture_log.close();resident_log.close()
if __name__ == '__main__': main()
