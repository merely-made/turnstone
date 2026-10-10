from pathlib import Path
import hashlib, json, os, re, shutil, signal, subprocess, sys, time
receipt = Path(__file__).resolve().parent
stage = sys.argv[1]
if stage not in ('red', 'green'): raise SystemExit('Expected red or green')
baseline = (receipt/'task-1-review-input-covering-after.sha256').read_text().splitlines()
roots = [line.split(' HEAD ')[0] for line in baseline if ' HEAD ' in line]
paths = [line.split('  ', 1)[1] for line in baseline if ' HEAD ' not in line]
prefix = f'task-1-review-mouse-boundary-{stage}'
def snapshot(which):
    rows = []
    for root in roots:
        head = subprocess.check_output(['git','-C',root,'rev-parse','HEAD'], text=True).strip()
        rows.append(f'{root} HEAD {head}')
    for path in paths:
        rows.append(f'{hashlib.sha256(Path(path).read_bytes()).hexdigest()}  {path}')
    (receipt/f'{prefix}-{which}.sha256').write_text('\n'.join(rows)+'\n')
    return rows
def memory():
    wanted = ('MemAvailable','MemTotal','SwapFree','SwapTotal')
    return {line.split(':',1)[0]:int(line.split(':',1)[1].split()[0])*1024 for line in Path('/proc/meminfo').read_text().splitlines() if line.split(':',1)[0] in wanted}
before = snapshot('before')
mem = memory()
print(stage, 'MemAvailable bytes:', mem['MemAvailable'], flush=True)
if mem['MemAvailable'] <= 3*1024**3: raise SystemExit('3 GiB memory buffer unavailable; deferred before Cargo.')
local = Path('.cargo/local/Cargo.lock')
shutil.copy2(local,receipt/f'{prefix}-before-local-lock.txt')
for name in ('window_input.rs','window_input_tests.rs','input.rs','lens.rs'):
    shutil.copy2(Path('src/shell')/name,receipt/f'{prefix}-{name}.txt')
cmd=['ionice','-c3','nice','-n','10','python3','scripts/cargo_mode.py','local','+1.98.1','test','--lib','review_mouse_boundary_','-j1','--','--nocapture']
started=time.monotonic()
low=mem['MemAvailable']
stopped=False
log=receipt/f'{prefix}.log'
try:
    with log.open('w') as output:
        output.write('Command: '+' '.join(cmd)+'\n'); output.flush()
        child=subprocess.Popen(cmd,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
        while child.poll() is None:
            now=memory()['MemAvailable'];low=min(low,now)
            if now <= 3*1024**3:
                os.killpg(child.pid,signal.SIGTERM);stopped=True
                try: child.wait(timeout=2)
                except subprocess.TimeoutExpired: os.killpg(child.pid,signal.SIGKILL)
                break
            time.sleep(.5)
        status=child.wait()
finally:
    after=snapshot('after')
    shutil.copy2(local,receipt/f'{prefix}-after-local-lock.txt')
text=log.read_text()
rows=re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in ([^\n]+)',text)
changes=[{'before':a,'after':b} for a,b in zip(before,after) if a!=b]
result={'stage':stage,'command':cmd,'exit':status,'summaries':rows,'memory_before':mem,'lowest_mem_available_bytes':low,'stopped_for_memory':stopped,'wall_seconds':round(time.monotonic()-started,3),'records':len(before),'changes':changes}
(receipt/f'{prefix}-results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result),flush=True)
for line in text.splitlines():
    if line.startswith(('running ','test ','thread ','failures:','error[E')) or 'Finished `test`' in line: print(line,flush=True)
if changes and any('.cargo/local/Cargo.lock' not in c['before'] for c in changes): raise SystemExit('Source/config/portable-lock drift during job; source remains held.')
if stopped: raise SystemExit('Own test stopped for memory floor; no passing receipt.')
expected=('FAILED','1','4') if stage=='red' else ('ok','5','0')
if len(rows)!=1 or tuple(rows[0][:3])!=expected or status!=(1 if stage=='red' else 0): raise SystemExit('Actual result differs from expected five-test behavioral gate.')
control='test shell::window_input::tests::review_mouse_boundary_unrelated_release_keeps_middle_custody'
if control not in text or (stage=='green' and not re.search(re.escape(control)+r' \.\.\. ok',text)): raise SystemExit('Unrelated-release control did not execute/pass.')
