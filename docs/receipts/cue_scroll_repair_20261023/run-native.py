from pathlib import Path
import functools
import http.server
import json
import os
import subprocess
import sys
import threading

repo = Path('/home/markik/Code/worktrees/turnstone-cue-scroll-20261023')
output = Path('/home/markik/Code/output/genet-scroll-background-20261023')
case = sys.argv[1]
scenario = sys.argv[2] if len(sys.argv) > 2 else 'cue_ui_retained.scn'
env = os.environ.copy()
env.update(XDG_RUNTIME_DIR='/run/user/1000', WAYLAND_DISPLAY='wayland-0',
           XDG_SESSION_TYPE='wayland', DBUS_SESSION_BUS_ADDRESS='unix:path=/run/user/1000/bus',
           VK_DRIVER_FILES='/usr/share/vulkan/icd.d/radeon_icd.x86_64.json')
env.pop('DISPLAY', None)
state = subprocess.check_output(['loginctl', 'show-session', '8', '-p', 'Type', '-p', 'Active', '-p', 'LockedHint'], text=True)
assert 'Type=wayland' in state and 'Active=yes' in state and 'LockedHint=no' in state, state
assert Path('/run/user/1000/wayland-0').is_socket()
with (output / (case + '-server.log')).open('w') as log:
    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, format, *args):
            log.write((format % args) + '\n')
            log.flush()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(repo/'scenarios/fixtures')))
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    url = 'http://127.0.0.1:' + str(server.server_port)
    (output / (case + '-environment.json')).write_text(json.dumps({'session':state,'fixture_base':url,'environment':{k:env[k] for k in ['XDG_RUNTIME_DIR','WAYLAND_DISPLAY','XDG_SESSION_TYPE','DBUS_SESSION_BUS_ADDRESS','VK_DRIVER_FILES']}}, indent=2)+'\n')
    try:
        args=['python3', str(repo/'scripts/receive-cue-ui-linux.py'), '/home/markik/Code/target/debug/turnstone', str(repo/'scenarios'/scenario), str(output/case), url]
        with (output/(case+'-driver.log')).open('w') as driver_log:
            p=subprocess.run(args, env=env, cwd=repo, stdout=driver_log, stderr=subprocess.STDOUT)
        print('driver exit:',p.returncode,flush=True)
        print((output/(case+'-driver.log')).read_text()[-5000:],flush=True)
    finally:
        server.shutdown()
        server.server_close()
    raise SystemExit(p.returncode)
