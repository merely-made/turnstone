from pathlib import Path
import json
import os
import resource
import subprocess
import time

out=Path('/home/markik/Code/output/cue-vulkan-thinkpad-20261023')
turnstone=Path('/home/markik/Code/worktrees/turnstone-cue-vulkan-20261023')
mere=Path('/home/markik/Code/worktrees/mere-cue-vulkan-20261023')
env=os.environ.copy()
env.update(CARGO_TARGET_DIR='/home/markik/Code/target', CARGO_BUILD_JOBS='2',
           WGPU_BACKEND='vulkan', VK_DRIVER_FILES='/usr/share/vulkan/icd.d/radeon_icd.x86_64.json',
           XDG_RUNTIME_DIR='/run/user/1000')
limit=resource.getrlimit(resource.RLIMIT_NOFILE)
resource.setrlimit(resource.RLIMIT_NOFILE,(min(65536,limit[1]),limit[1]))
gates=[
    ('turnstone-pending-vulkan',turnstone,['test','--lib','--locked','--offline','-j2','cue_content::tests::','--','--include-ignored','--nocapture','--test-threads=1']),
    ('turnstone-history',turnstone,['test','--lib','--locked','--offline','-j2','app::surface_control_tests::','--','--test-threads=1']),
    ('turnstone-inspector',turnstone,['test','--lib','--locked','--offline','-j2','inspector','--','--test-threads=1']),
    ('turnstone-all-targets',turnstone,['check','--all-targets','--locked','--offline','-j2']),
]
for name,repo,args in gates:
    name="cohort-"+name
    env['CARGO_BUILD_BUILD_DIR']='/home/markik/Code/repos/'+('turnstone' if repo==turnstone else 'mere')+'/target/build'
    command=['nice','-n','10','cargo','+1.99.1',*args]
    record={'command':command,'cwd':str(repo),'started':time.time(),'exit':None,
            'environment':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_BUILD_JOBS','WGPU_BACKEND','VK_DRIVER_FILES','XDG_RUNTIME_DIR']},
            'child_nofile':list(resource.getrlimit(resource.RLIMIT_NOFILE))}
    path=out/(name+'.json')
    assert not path.exists(), 'preserve prior gate: '+name
    path.write_text(json.dumps(record,indent=2)+'\n')
    print('START',name,flush=True)
    with (out/(name+'.log')).open('w') as log:
        p=subprocess.run(command,cwd=repo,env=env,stdout=log,stderr=subprocess.STDOUT)
    record.update(exit=p.returncode,elapsed_seconds=round(time.time()-record['started'],3))
    path.write_text(json.dumps(record,indent=2)+'\n')
    print('END',name,p.returncode,record['elapsed_seconds'],flush=True)
    print('\n'.join((out/(name+'.log')).read_text().splitlines()[-12:]),flush=True)
    if p.returncode:
        raise SystemExit(p.returncode)
