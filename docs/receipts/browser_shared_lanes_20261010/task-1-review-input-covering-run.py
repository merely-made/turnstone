from pathlib import Path
import hashlib, json, os, re, shutil, signal, subprocess, time

receipt = Path(__file__).resolve().parent
baseline = (receipt / 'task-1-review-input-green-after.sha256').read_text().splitlines()
roots = [line.split(' HEAD ')[0] for line in baseline if ' HEAD ' in line]
paths = [line.split('  ', 1)[1] for line in baseline if ' HEAD ' not in line]
for extra in ('README.md', 'scripts/cargo_mode.py'):
    full = str(Path.cwd() / extra)
    if full not in paths:
        paths.append(full)

def snapshot(name):
    lines = []
    for root in roots:
        head = subprocess.check_output(['git', '-C', root, 'rev-parse', 'HEAD'], text=True).strip()
        lines.append(f'{root} HEAD {head}')
    for path in paths:
        lines.append(f'{hashlib.sha256(Path(path).read_bytes()).hexdigest()}  {path}')
    out = receipt / f'task-1-review-input-covering-{name}.sha256'
    out.write_text('\n'.join(lines) + '\n')
    return lines

def memory():
    values = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemAvailable', 'MemTotal', 'SwapFree', 'SwapTotal'):
            values[key] = int(value.split()[0]) * 1024
    return values

before = snapshot('before')
if not set(baseline).issubset(before):
    raise SystemExit('Current source/config/lock/HEAD differs from retained GREEN-after boundary; no test started.')
local = Path('.cargo/local/Cargo.lock')
shutil.copy2(local, receipt / 'task-1-review-input-green-effective-local-lock-recovered.txt')
shutil.copy2(local, receipt / 'task-1-review-input-covering-before-local-lock.txt')
jobs = [
    ('content-pool', 'content_pool', None, 8),
    ('native-lens', 'foreign_lens_retains_native_forest', None, 1),
]
results = []
try:
    for label, test_filter, exact, expected in jobs:
        mem = memory()
        print(label, 'MemAvailable bytes:', mem['MemAvailable'], flush=True)
        if mem['MemAvailable'] <= 3 * 1024**3:
            raise RuntimeError('3 GiB memory buffer unavailable; test deferred.')
        cmd = ['ionice', '-c3', 'nice', '-n', '10', 'python3', 'scripts/cargo_mode.py', 'local', '+1.98.1', 'test', '--lib', test_filter, '-j1', '--', '--nocapture']
        if exact:
            cmd.append(exact)
        log = receipt / f'task-1-review-input-covering-{label}.log'
        started = time.monotonic()
        low = mem['MemAvailable']
        with log.open('w') as output:
            output.write('Command: ' + ' '.join(cmd) + '\n')
            output.flush()
            child = subprocess.Popen(cmd, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            stopped = False
            while child.poll() is None:
                now = memory()['MemAvailable']
                low = min(low, now)
                if now <= 3 * 1024**3:
                    os.killpg(child.pid, signal.SIGTERM)
                    stopped = True
                    break
                time.sleep(1)
            status = child.wait()
        text = log.read_text()
        matches = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out; finished in ([^\n]+)', text)
        result = {'label': label, 'command': cmd, 'exit': status, 'expected_count': expected, 'summaries': matches, 'memory_before': mem, 'lowest_mem_available_bytes': low, 'stopped_for_memory': stopped, 'wall_seconds': round(time.monotonic() - started, 3)}
        results.append(result)
        print(json.dumps(result), flush=True)
        if label == 'content-pool':
            for control in ('content_pool_contributed_cancel_preserves_move_and_retires_capture', 'content_pool_host_contributed_mouse_waits_for_release_and_cancel_never_activates'):
                if not re.search(r'test contributed_surface::tests::' + control + r' \.\.\. ok', text):
                    raise RuntimeError('Missing passing contributed control: ' + control)
        if status != 0 or stopped or len(matches) != 1 or matches[0][0] != 'ok' or int(matches[0][1]) != expected or int(matches[0][2]) != 0:
            raise RuntimeError('Covering result did not meet its explicit count/exit gate; production remains held.')
finally:
    after = snapshot('after')
    shutil.copy2(local, receipt / 'task-1-review-input-covering-after-local-lock.txt')
    changes = [{'before': a, 'after': b} for a, b in zip(before, after) if a != b]
    (receipt / 'task-1-review-input-covering-results.json').write_text(json.dumps({'results': results, 'records': len(before), 'changes': changes}, indent=2) + '\n')
    print('After-proof records:', len(after), 'changed:', len(changes), flush=True)
    for change in changes:
        print(json.dumps(change), flush=True)
