from pathlib import Path
import hashlib
import json
import os
import resource
import subprocess
import sys
import time

out = Path('/home/markik/Code/output/cue-native-vulkan-20261023')
name, family, *args = sys.argv[1:]
assert family in ('turnstone', 'mere')
repo = Path('/home/markik/Code/worktrees') / (family + '-cue-native-20261023')
path = out / (name + '.json')
assert not path.exists(), 'preserve prior gate: ' + name
env = os.environ.copy()
env.update(CARGO_TARGET_DIR='/home/markik/Code/target', CARGO_BUILD_JOBS='2',
           CARGO_BUILD_BUILD_DIR='/home/markik/Code/repos/' + family + '/target/build',
           WGPU_BACKEND='vulkan', VK_DRIVER_FILES='/usr/share/vulkan/icd.d/radeon_icd.x86_64.json',
           XDG_RUNTIME_DIR='/run/user/1000')
limit = resource.getrlimit(resource.RLIMIT_NOFILE)
resource.setrlimit(resource.RLIMIT_NOFILE, (min(65536, limit[1]), limit[1]))
lock = repo / 'Cargo.lock'
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
command = ['nice', '-n', '10', 'cargo', '+1.99.1', *args]
record = {'command': command, 'cwd': str(repo), 'started': time.time(), 'exit': None,
          'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
          'source_status': subprocess.check_output(['git', 'status', '--porcelain=v1', '--untracked-files=all'], cwd=repo, text=True),
          'manifest_sha256': digest(repo/'Cargo.toml'), 'lock_before_sha256': digest(lock),
          'environment': {k: env[k] for k in ['CARGO_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CARGO_BUILD_JOBS', 'WGPU_BACKEND', 'VK_DRIVER_FILES', 'XDG_RUNTIME_DIR']},
          'child_nofile': list(resource.getrlimit(resource.RLIMIT_NOFILE))}
path.write_text(json.dumps(record, indent=2)+'\n')
print('START', name, flush=True)
with (out/(name+'.log')).open('w') as log:
    p = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
record.update(exit=p.returncode, elapsed_seconds=round(time.time()-record['started'],3),
              lock_after_sha256=digest(lock))
if family == 'turnstone' and args[0] == 'build' and p.returncode == 0:
    record['binary_sha256'] = digest(Path('/home/markik/Code/target/debug/turnstone'))
if args[0] == 'test' and p.returncode == 0:
    import re
    binaries = re.findall(r'Running .*?\((/[^)]+)\)', (out/(name+'.log')).read_text())
    record['test_binaries_sha256'] = {b: digest(Path(b)) for b in binaries}
path.write_text(json.dumps(record, indent=2)+'\n')
print('END', name, p.returncode, record['elapsed_seconds'], flush=True)
print('\n'.join((out/(name+'.log')).read_text().splitlines()[-25:]), flush=True)
assert record['lock_before_sha256'] == record['lock_after_sha256'], 'lock changed during receiving'
raise SystemExit(p.returncode)
