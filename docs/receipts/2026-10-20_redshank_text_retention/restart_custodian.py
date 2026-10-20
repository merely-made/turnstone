from pathlib import Path
import os,signal,subprocess,time,json,hashlib,sys
OUT=Path(__file__).parent;ROOT=OUT/'runtime'
assert (ROOT/'fixture-marker').read_text()=='redshank-text-retention-20261020\n'
old=json.loads((OUT/'custodian-process.json').read_text())
cmd=subprocess.check_output(['ps','-p',str(old['pid']),'-o','command='],text=True)
assert str(OUT/'custodian-host') in cmd
os.kill(old['pid'],signal.SIGTERM)
for _ in range(40):
    if subprocess.run(['kill','-0',str(old['pid'])],capture_output=True).returncode:break
    time.sleep(.1)
else:raise RuntimeError('custodian did not stop')
sock=ROOT/'custody.sock'
if sock.exists():sock.unlink()
env=os.environ.copy();env.update(EIDETIC_CUSTODY_FIXTURE_ROOT=str(ROOT/'custodian'),EIDETIC_CUSTODY_FIXTURE_ENDPOINT=str(sock))
p=subprocess.Popen([str(OUT/'custodian-host'),'--ignored','--exact','retained_custody_receipt_host','--nocapture'],env=env,stdout=(OUT/('custodian-'+sys.argv[1]+'.log')).open('w'),stderr=subprocess.STDOUT,start_new_session=True)
for _ in range(100):
    if sock.exists():break
    if p.poll() is not None:raise RuntimeError('custodian exited')
    time.sleep(.1)
else:raise RuntimeError('no socket')
now={**old,'pid':p.pid,'previous_pid':old['pid'],'same_persisted_fixture_wallet':True}
(OUT/('custodian-'+sys.argv[1]+'-process.json')).write_text(json.dumps(now,indent=2)+'\n')
(OUT/'custodian-process.json').write_text(json.dumps(now,indent=2)+'\n')
print(p.pid)
