"""macOS counterpart of web/tests/run-headed-receipt.py for this machine: the repo's own receipt sink
and the loader's `?sink=` protocol, with the real Google Chrome instead of the Linux flatpak and no
/proc/stat sampling. Usage: run_headed_macos.py WEB OUT PORT PROFILE SCENARIO QUERY"""
import json, pathlib, subprocess, sys, time, socket, signal, os
web, out, port, profile, scenario, query = sys.argv[1:7]
web = pathlib.Path(web).resolve(); out = pathlib.Path(out).resolve(); port = int(port); profile = pathlib.Path(profile).resolve()
out.mkdir(parents=True, exist_ok=False); profile.mkdir(parents=True, exist_ok=True)
for p in (port, port + 1):
    with socket.socket() as s: s.bind(('127.0.0.1', p))
origin = f'http://127.0.0.1:{port}'
sink = subprocess.Popen(['python3', str(web / 'tests/receipt_sink.py'), '--web', str(web), '--out', str(out), '--port', str(port), '--allowed-origin', origin], stdout=open(out / 'sink.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
time.sleep(1.5)
url = f'{origin}/tree.html?scenario=scenarios/{scenario}.scn&sink={origin}/scenario-receipt&{query}'
chrome = subprocess.Popen(['/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', f'--user-data-dir={profile}', f'--remote-debugging-port={port+1}', '--no-first-run', '--no-default-browser-check', '--new-window', '--window-size=1400,900', '--enable-unsafe-webgpu', url], stdout=open(out / 'chrome.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
started = time.monotonic(); status = 'timeout'
try:
    while time.monotonic() - started < 120:
        if (out / 'scenario.done').exists(): status = 'done'; break
        if chrome.poll() is not None: status = 'chrome-exited'; break
        time.sleep(1)
    print(status, (out / 'scenario.done').read_text() if (out / 'scenario.done').exists() else '')
    if (out / 'result.json').exists():
        r = json.loads((out / 'result.json').read_text())['scenario']
        print('result', r['result']['result'], 'errors', r.get('errors'), 'gate', r.get('gate_failures'))
        print(json.dumps(r['result'], indent=1)[:1800])
finally:
    for proc in (chrome, sink):
        try: os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError: pass
