#!/usr/bin/env python3
"""Source-bound native document-format and research-note qualification with real origin shutdown."""
import argparse, datetime, errno, hashlib, json, os
from pathlib import Path
import shutil, socket, subprocess, sys, time
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
LAUNCHER=HERE/'run_macos_scenario.py'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path,value):path.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for key in ['binary','client-tests','output']:p.add_argument('--'+key,type=Path,required=True)
    p.add_argument('--timeout',type=int,default=240)
    a=p.parse_args();out=a.output.resolve()
    if sys.platform!='darwin' or out.exists() or out.is_relative_to(ROOT):p.error('requires macOS and new external output directory')
    out.mkdir(parents=True);frozen=out/'frozen';frozen.mkdir();control=out/'control';control.mkdir()
    for path,name in [(a.binary,'turnstone'),(a.client_tests,'turnstone-tests')]:shutil.copy2(path,frozen/name)
    for name in ['create.scn','seed.scn','reopen.scn','fixture_server.py','run_native.py']:shutil.copy2(HERE/name,frozen/name)
    files=subprocess.check_output(['git','ls-files','-c','-o','--exclude-standard','src','scenarios','Cargo.toml','Cargo.lock'],cwd=ROOT,text=True).splitlines()
    sources={f:sha(ROOT/f) for f in sorted(set(files)) if (ROOT/f).is_file()}
    write(out/'inputs.json',{'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_sha256':sources,'binary_sha256':sha(frozen/'turnstone'),'client_tests_sha256':sha(frozen/'turnstone-tests'),'harness_sha256':{n:sha(frozen/n) for n in ['create.scn','seed.scn','reopen.scn','fixture_server.py','run_native.py']},'launcher_sha256':sha(LAUNCHER),'os':subprocess.check_output(['sw_vers'],text=True)})
    streams=[];fixture=None;origin=None;runs=[];first=None;other=None;other_hash=None;seen=set()
    def log(name):
        stream=(out/name).open('w');streams.append(stream);return stream
    def origin_start(phase):
        nonlocal fixture,origin
        ready=out/f'origin-ready-{phase}.json'
        command=[sys.executable,str(frozen/'fixture_server.py'),'--output',str(out/'fixture'),'--ready-file',str(ready)]
        if origin:command+=['--port',str(origin['port'])]
        fixture=subprocess.Popen(command,stdout=log(f'origin-{phase}.log'),stderr=subprocess.STDOUT)
        deadline=time.monotonic()+10
        while not ready.exists() and time.monotonic()<deadline:
            assert fixture.poll() is None;time.sleep(.1)
        observed=json.loads(ready.read_text());assert origin is None or origin==observed;origin=observed
    def origin_stop(phase):
        fixture.terminate();fixture.wait(timeout=5)
        with socket.socket() as sock:code=sock.connect_ex((origin['host'],origin['port']))
        assert code==errno.ECONNREFUSED
        write(out/f'origin-stop-{phase}.json',{'pid':fixture.pid,'exit_code':fixture.returncode,'exited':True,'tcp_refused':True})
        shutil.copy2(out/'fixture/requests.json',out/f'requests-at-stop-{phase}.json')
        (control/f'origin-stopped-{phase}').write_text('origin exited; TCP refused\n')
    awake=subprocess.Popen(['/usr/bin/caffeinate','-d','-i','-t',str(3*a.timeout+90)])
    try:
        origin_start(0)
        for phase in ['create','seed','reopen']:
            assert all(sha(ROOT/f)==digest for f,digest in sources.items()),'source drift'
            if phase=='reopen':
                assert fixture.poll() is not None
                with socket.socket() as sock:assert sock.connect_ex((origin['host'],origin['port']))==errno.ECONNREFUSED
            directory=out/phase
            env={'TURNSTONE_ROOT':str(out/'profile'),'TURNSTONE_CAPTURE_DIR':str(directory),'TURNSTONE_OFFLINE':str(int(phase=='reopen')),'TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT':'1','TURNSTONE_THEME_LIBRARY':str(out/'themes.json'),'NOTES_ORIGIN':origin['base_url'],'NOTES_CONTROL':str(control),'FIRST_NOTE':first.as_uri() if first else 'file:///unavailable','MERE_ROOT':str(out/'mere'),'XDG_DATA_HOME':str(out/'data'),'XDG_CONFIG_HOME':str(out/'config'),'XDG_CACHE_HOME':str(out/'cache'),'TMPDIR':'/tmp','RUST_LOG':'warn,turnstone=debug,taproot=info,swarm_discovery=error'}
            command=[sys.executable,str(LAUNCHER),'--binary',str(frozen/'turnstone'),'--prefix','TURNSTONE','--scenario',str(frozen/(phase+'.scn')),'--output',str(directory),'--timeout',str(a.timeout)]
            for key,value in env.items():command+=['--env',key+'='+value]
            write(out/(phase+'-command.json'),command)
            app=subprocess.Popen(command,cwd=ROOT,stdout=log('launch-'+phase+'.log'),stderr=subprocess.STDOUT)
            deadline=time.monotonic()+a.timeout+20
            while app.poll() is None and time.monotonic()<deadline:
                for marker,operation in [('stop-origin-0',lambda:origin_stop(0)),('start-origin-1',lambda:origin_start(1)),('stop-origin-1',lambda:origin_stop(1))]:
                    if marker not in seen and (control/marker).exists():
                        seen.add(marker);operation()
                        if marker=='start-origin-1':(control/'origin-started-1').write_text('same origin restarted\n')
                time.sleep(.1)
            if app.poll() is None:app.terminate();app.wait(timeout=5);raise TimeoutError(phase)
            result=json.loads((directory/'process.json').read_text());runs.append({'phase':phase,'launcher_exit':app.returncode,'process':result});write(out/'runs.json',runs)
            assert app.returncode==0 and result['success'],phase+' headed failure'
            if phase=='create':
                paths=list((out/'profile/notes').glob('*.djot'));assert len(paths)==2
                first=next(p for p in paths if 'My authored A1 commentary.' in p.read_text())
                other=next(p for p in paths if 'Other note stays independent.' in p.read_text());other_hash=sha(other)
                write(out/'note-identities.json',{'first':str(first),'other':str(other),'other_sha256':other_hash})
            else:
                assert sha(other)==other_hash,'unselected note changed'
                inspect_env=os.environ.copy();inspect_env.update(TURNSTONE_NOTES_PROFILE=str(out/'profile'),TURNSTONE_NOTES_OBSERVATION=str(out/'seed/11_old_A1_after_A2.saved-sources.json'),TURNSTONE_NOTES_STORE_RECEIPT=str(out/(phase+'-store-inspection.json')))
                subprocess.run([str(frozen/'turnstone-tests'),'trail_memory::research_owner_tests::independently_inspect_exited_research_notes','--ignored','--exact','--nocapture'],cwd=ROOT,env=inspect_env,stdout=log(phase+'-store-inspection.log'),stderr=subprocess.STDOUT,check=True,timeout=30)
        assert (out/'requests-at-stop-1.json').read_bytes()==(out/'fixture/requests.json').read_bytes()
        write(out/'qualification.json',{'result':'pass','source_bound':True,'origin_gone_before_clip_and_cold_reopen':True,'new_app_and_hosted_knot_endpoint_processes':True,'unselected_note_unchanged':True,'other_note_sha256':other_hash,'runs':runs,'captures':len(list(out.glob('*/*.png')))})
    finally:
        if fixture and fixture.poll() is None:fixture.terminate();fixture.wait(timeout=5)
        awake.terminate();awake.wait(timeout=5)
        for stream in streams:stream.close()
if __name__=='__main__':main()
