"""Run one headed receipt using the established Linux receipt harness.

Derived from testing/mere/viewer-cone/run-linux-receipt.py, SHA256
d408973608a57e54de5f03bce8cb9530e7bfec082e07a875bea21ff236ce76f5.
Adds explicitly shared managed profiles for reopen chains and outer-page captures.
No builds. Explicit external-fixture input uses the allowlisted CDP bridge. Output must be new. The sink and
Chrome process group belong to this run; unrelated processes are untouched.
Linux /proc/stat samples record total CPU use, including the receipt itself.
"""

import argparse
import hashlib
import json
import os
import pathlib
import platform
import signal
import socket
import subprocess
import time
import urllib.request


def cpu_ticks():
    ticks = list(map(int, pathlib.Path('/proc/stat').read_text().splitlines()[0].split()[1:9]))
    return sum(ticks), ticks[3] + ticks[4]


DEFAULT_SINK = pathlib.Path(__file__).resolve().with_name('receipt_sink.py')


def sink_command(args, origin):
    command = ['python3', str(args.sink), '--web', str(args.web), '--out', str(args.out), '--port', str(args.port)]
    if args.sink.resolve() == DEFAULT_SINK:
        command.extend(['--allowed-origin', origin])
    return command


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--web', required=True, type=pathlib.Path)
    parser.add_argument('--out', required=True, type=pathlib.Path)
    parser.add_argument('--sink', type=pathlib.Path, default=DEFAULT_SINK)
    parser.add_argument('--scenario', required=True)
    parser.add_argument('--page', default='tree.html')
    parser.add_argument('--query', required=True)
    parser.add_argument('--origin', help='Existing fixture gateway origin for pages such as co_op.html; receipts still use this run\'s isolated sink')
    parser.add_argument('--port', type=int, default=8960)
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--unsafe-webgpu', action='store_true')
    parser.add_argument('--chrome-arg', action='append', default=[])
    parser.add_argument('--profile', type=pathlib.Path,
                        help='Managed profile shared only by an explicit same-origin scenario chain')
    parser.add_argument('--node', required=True, type=pathlib.Path,
                        help='Node 22+ executable for read-only headed page captures')
    args = parser.parse_args()
    args.web = args.web.resolve()
    args.out = args.out.resolve()
    origin = (args.origin or 'http://127.0.0.1:' + str(args.port)).rstrip('/')
    for port in (args.port, args.port + 1):
        with socket.socket() as port_check:
            port_check.bind(('127.0.0.1', port))
    args.out.mkdir(parents=True, exist_ok=False)
    profile = (args.profile or args.out / 'chrome-profile').resolve()
    marker = profile / '.graphshell-receipt-profile'
    if profile.exists() and not marker.exists():
        raise RuntimeError('Refusing an existing profile not created by this receipt harness')
    profile.mkdir(parents=True, exist_ok=True)
    marker.write_text('Managed Graphshell headed receipt profile\n')
    wasm = args.web / 'pkg/graphshell_web_bg.wasm'
    metadata = {
        'machine': platform.node(), 'platform': platform.platform(),
        'bundle_sha256': hashlib.sha256(wasm.read_bytes()).hexdigest(),
        'bundle_bytes': wasm.stat().st_size, 'scenario': args.scenario,
        'scenario_sha256': hashlib.sha256((args.web / 'scenarios' / (args.scenario + '.scn')).read_bytes()).hexdigest(),
        'query': args.query, 'load_before': os.getloadavg(),
        'unsafe_webgpu': args.unsafe_webgpu, 'cpu_samples': [],
        'page': args.page, 'profile': str(profile),
        'origin': origin,
        'profile_shared': args.profile is not None,
    }
    before = cpu_ticks()
    time.sleep(1)
    after = cpu_ticks()
    metadata['cpu_before_percent'] = 100 * (1 - (after[1] - before[1]) / max(1, after[0] - before[0]))
    env = dict(os.environ)
    env.setdefault('XDG_RUNTIME_DIR', '/run/user/' + str(os.getuid()))
    env.setdefault('WAYLAND_DISPLAY', 'wayland-0')
    env.setdefault('DBUS_SESSION_BUS_ADDRESS', 'unix:path=' + env['XDG_RUNTIME_DIR'] + '/bus')
    chrome = None
    host_input = None
    sink = None
    started = time.monotonic()
    try:
        with (args.out / 'sink.log').open('w') as sink_log, (args.out / 'chrome.log').open('w') as chrome_log:
            sink = subprocess.Popen(sink_command(args, origin), stdout=sink_log, stderr=sink_log, start_new_session=True)
            deadline = time.monotonic() + 10
            while True:
                if sink.poll() is not None:
                    raise RuntimeError('sink exited; see sink.log')
                try:
                    with urllib.request.urlopen('http://127.0.0.1:' + str(args.port) + '/tree.html', timeout=1) as response:
                        if response.status == 200:
                            break
                except OSError:
                    if time.monotonic() > deadline:
                        raise RuntimeError('sink did not start')
                    time.sleep(.25)
            url = (origin + '/' + args.page + '?scenario=scenarios/' + args.scenario + '.scn&sink=http://127.0.0.1:' + str(args.port) + '/scenario-receipt&' + args.query)
            command = ['flatpak', 'run', '--filesystem=' + str(args.out), '--filesystem=' + str(profile), 'com.google.Chrome', '--user-data-dir=' + str(profile), '--remote-debugging-port=' + str(args.port + 1), '--no-first-run', '--no-default-browser-check', '--enable-logging=stderr', '--ozone-platform=wayland', '--disable-features=CalculateNativeWinOcclusion', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding', '--new-window', '--window-size=1400,900']
            if args.unsafe_webgpu:
                command.append('--enable-unsafe-webgpu')
            command.extend(args.chrome_arg)
            command.append(url)
            metadata['command'] = command
            chrome = subprocess.Popen(command, stdout=chrome_log, stderr=chrome_log, env=env, start_new_session=True)
            host_input = subprocess.Popen([
                str(args.node), str(pathlib.Path(__file__).with_name('host-input.mjs')),
                str(args.port + 1), origin + '/', str(args.out),
            ], stdout=chrome_log, stderr=chrome_log, start_new_session=True)
            previous = cpu_ticks()
            while not (args.out / 'scenario.done').exists():
                if host_input.poll() not in (None, 0):
                    raise RuntimeError('External fixture input bridge failed; see host-input.json and chrome.log')
                if chrome.poll() is not None:
                    raise RuntimeError('Chrome exited; see chrome.log')
                if time.monotonic() - started > args.timeout:
                    raise RuntimeError('receipt timed out')
                time.sleep(1)
                current = cpu_ticks()
                metadata['cpu_samples'].append({'seconds': round(time.monotonic() - started, 2), 'percent': round(100 * (1 - (current[1] - previous[1]) / max(1, current[0] - previous[0])), 2)})
                previous = current
            result = json.loads((args.out / 'result.json').read_text())
            scenario = result['scenario']
            metadata['result'] = scenario['result']['result']
            metadata['page_errors'] = scenario.get('errors', [])
            metadata['gate_failures'] = scenario.get('gate_failures', [])
            subprocess.run([
                str(args.node), str(pathlib.Path(__file__).with_name('capture-page.mjs')),
                str(args.port + 1), origin + '/', str(args.out),
            ], check=True, timeout=50)
            metadata['whole_frame_capture'] = 'whole-frame.png'
            metadata['whole_page_accessibility'] = 'whole-page-accessibility.json'
            print((args.out / 'scenario.done').read_text(), flush=True)
    except Exception as error:
        metadata['result'] = 'fail'
        metadata['failure'] = str(error)
        raise
    finally:
        for process in (host_input, chrome, sink):
            if process is not None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                    process.wait(timeout=5)
                except ProcessLookupError:
                    pass
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
        metadata['elapsed_seconds'] = round(time.monotonic() - started, 2)
        metadata['load_after'] = os.getloadavg()
        (args.out / 'environment.json').write_text(json.dumps(metadata, indent=2) + '\n')


if __name__ == '__main__':
    main()
