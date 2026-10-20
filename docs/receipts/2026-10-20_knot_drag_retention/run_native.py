"""Run the shipping native products with isolated synthetic profiles only."""
from pathlib import Path
import json, subprocess, sys
OUT=Path(__file__).parent
RUNTIME=OUT/'runtime'
assert (RUNTIME/'fixture-marker').read_text()=='eidetic-headed-20261020\n'
def run(product, phase, scenario, profile='receiver', extra=None):
    env={
        'MERE_ROOT': str(RUNTIME/'shared'),
        'XDG_CONFIG_HOME': str(RUNTIME/'xdg-config'),
        'XDG_CACHE_HOME': str(RUNTIME/'xdg-cache'),
        'XDG_DATA_HOME': str(RUNTIME/'xdg-data'),
        'TMPDIR': str(RUNTIME/'tmp')+'/',
    }
    if product=='turnstone':
        prefix='TURNSTONE';binary='turnstone'
        env.update(TURNSTONE_ROOT=str(RUNTIME/profile), TURNSTONE_OFFLINE='1',
            GRAPHSHELL_APP_ENDPOINT=str(RUNTIME/'custody.sock'), TURNSTONE_WIDTH='1024', TURNSTONE_HEIGHT='600')
    else:
        prefix='REDSHANK';binary='redshank-desktop'
        env.update(REDSHANK_DATA_DIR=str(RUNTIME/'producer'),REDSHANK_WIDTH='960',REDSHANK_HEIGHT='720')
    env.update(extra or {})
    command=['python3','/Users/markik/Code/repos/mere/scripts/run_macos_scenario.py',
        '--binary',str(OUT/'target/debug'/binary),
        '--prefix',prefix,'--scenario',str(OUT/scenario),'--output',str(OUT/phase),'--timeout','200']
    for key,value in env.items(): command+=['--env',key+'='+value]
    (OUT/(phase+'-environment.json')).write_text(json.dumps(env,indent=2)+'\n')
    with (OUT/(phase+'-launch.log')).open('w') as log:
        subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=220)
    result=json.loads((OUT/phase/'process.json').read_text());assert result['success'];return result
if __name__=='__main__':run(*sys.argv[1:])
