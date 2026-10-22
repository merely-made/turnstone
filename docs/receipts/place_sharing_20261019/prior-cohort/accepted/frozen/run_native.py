#!/usr/bin/env python3
"""Current-stack native place admission, selected address sharing and cold return."""
import argparse
import errno
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from resident import Resident

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def read(path):
    return json.loads(path.read_text())


class Native:
    def __init__(self, name, scenario, resident, output, frozen, origin, control, exchange, timeout):
        self.directory = output / name
        environment = resident.environment | {
            'TURNSTONE_CAPTURE_DIR': str(self.directory),
            'TURNSTONE_A11Y_PUBLIC_FIXTURE_RECEIPT': '1',
            'TURNSTONE_THEME_LIBRARY': str(resident.root / 'themes.json'),
            'RUST_LOG': 'warn,turnstone=debug,taproot=info,swarm_discovery=error',
            'ORIGIN': origin, 'CONTROL': str(control), 'EXCHANGE': str(exchange),
        }
        command = [sys.executable, str(frozen / 'run_macos_scenario.py'),
                   '--binary', str(frozen / 'turnstone'),
                   '--bundle-root', str(resident.root / 'native-bundle'), '--prefix', 'TURNSTONE',
                   '--scenario', str(frozen / scenario), '--output', str(self.directory),
                   '--timeout', str(timeout)]
        for key, value in environment.items():
            if key.startswith(('TURNSTONE_', 'GRAPHSHELL_', 'MERE_', 'XDG_')) or key in {
                'LOCALAPPDATA', 'TMPDIR', 'SSH_AUTH_SOCK', 'RUST_LOG', 'ORIGIN', 'CONTROL', 'EXCHANGE',
            }:
                command += ['--env', key + '=' + value]
        write(output / (name + '-command.json'), command)
        self.stream = (output / (name + '-controller.log')).open('w')
        self.process = subprocess.Popen(command, cwd=ROOT, env=resident.environment,
                                        stdout=self.stream, stderr=subprocess.STDOUT)

    def check(self):
        if self.process.poll() is not None:
            assert self.process.returncode == 0, str(self.directory) + ' launcher failed'
            result = read(self.directory / 'process.json')
            assert result['success'] and not result['timeout'], str(self.directory) + ' scenario failed'

    def stop_owned(self):
        launch = self.directory / 'launch.json'
        if self.process.poll() is None and launch.exists():
            executable = Path(read(launch)['owned_executable'])
            paths = {str(executable), str(executable.resolve())}
            listing = subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True)
            for line in listing.splitlines():
                pid, _, command = line.strip().partition(' ')
                command = command.lstrip()
                if any(command == p or command.startswith(p + ' ') for p in paths):
                    try:
                        os.kill(int(pid), signal.SIGTERM)
                    except ProcessLookupError:
                        pass
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=5)
        self.stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--binary-inputs', type=Path, required=True)
    parser.add_argument('--djinn', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=600)
    options = parser.parse_args()
    output = options.output.resolve()
    if sys.platform != 'darwin' or output.exists() or output.is_relative_to(ROOT):
        parser.error('requires macOS and a new external output directory')
    output.mkdir(parents=True)
    frozen = output / 'frozen'
    frozen.mkdir()
    shutil.copy2(options.binary, frozen / 'turnstone')
    shutil.copy2(options.djinn, frozen / 'djinn')
    harness = [p for p in HERE.iterdir() if p.suffix in {'.scn', '.py'}]
    for path in harness:
        shutil.copy2(path, frozen / path.name)
    names = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard',
                                     'src', 'scenarios', 'Cargo.toml', 'Cargo.lock'], cwd=ROOT, text=True).splitlines()
    sources = {name: sha(ROOT / name) for name in sorted(set(names)) if (ROOT / name).is_file()}
    compiled = read(options.binary_inputs)
    assert compiled['binary_sha256'] == sha(frozen / 'turnstone'), 'wrong native executable'
    assert compiled['source_sha256'] == sources, 'native build inputs do not match current source'
    shutil.copy2(options.binary_inputs, frozen / 'app-build-inputs.json')
    private = Path(tempfile.mkdtemp(prefix='tspl-', dir='/tmp'))
    exchange = private / 'exchange'
    exchange.mkdir()
    control = output / 'control'
    control.mkdir()
    write(output / 'inputs.json', {
        'base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'source_sha256': sources, 'binary_sha256': sha(frozen / 'turnstone'),
        'app_build_inputs_sha256': sha(frozen / 'app-build-inputs.json'),
        'djinn_sha256': sha(frozen / 'djinn'),
        'harness_sha256': {p.name: sha(p) for p in harness},
        'private_runtime': str(private), 'os': subprocess.check_output(['sw_vers'], text=True),
    })
    requests = []
    request_lock = threading.Lock()

    class Pages(BaseHTTPRequestHandler):
        def do_GET(self):
            with request_lock:
                requests.append(self.path)
                write(output / 'requests.json', requests)
            pages = {'/shared.html': 'Shared place address',
                     '/founder-private.html': 'Unshared founder sentinel',
                     '/reader-private.html': 'Refused reader sentinel'}
            text = pages.get(self.path)
            body = ('<!doctype html><meta charset="utf-8"><title>' + (text or 'Missing') +
                    '</title><h1>' + (text or 'Missing') + '</h1><p>Public fictional place fixture.</p>').encode()
            self.send_response(200 if text else 404)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Pages)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = 'http://127.0.0.1:' + str(server.server_port)
    write(output / 'origin.json', {'base_url': origin})
    residents = {name: Resident(frozen / 'djinn', private / name, name, output / ('resident-' + name))
                 for name in ['founder', 'joiner', 'reader']}
    native = []
    awake = subprocess.Popen(['/usr/bin/caffeinate', '-d', '-i', '-t', str(3 * options.timeout + 300)])

    def marker(name):
        (control / name).write_text('ready\n')

    def wait(predicate, label, timeout=None):
        deadline = time.monotonic() + (timeout or options.timeout)
        while time.monotonic() < deadline:
            for run in native:
                run.check()
            if predicate():
                return
            time.sleep(.25)
        raise TimeoutError(label)

    def launch(name, scenario, role):
        assert all(sha(ROOT / name) == digest for name, digest in sources.items()), 'source drift'
        run = Native(name, scenario, residents[role], output, frozen, origin, control, exchange, options.timeout)
        native.append(run)
        return run

    def exited(run):
        wait(lambda: run.process.poll() is not None, str(run.directory))
        run.check()

    try:
        for resident in residents.values():
            status = resident.start()
            assert status['ready'] and status['lock'] == 'unlocked' and not status['installed']
        founder = launch('founder', 'founder.scn', 'founder')
        wait(lambda: (exchange / 'writer.place-card.json').exists(), 'founder card')
        joiner = launch('joiner', 'joiner.scn', 'joiner')
        exited(joiner)
        marker('joiner.stopped')
        returning = launch('joiner-return', 'joiner_return.scn', 'joiner')
        wait(lambda: (exchange / 'reader.place-card.json').exists(), 'reader card')
        reader = launch('reader', 'reader.scn', 'reader')
        exited(returning)
        exited(reader)
        marker('guests.stopped')
        exited(founder)
        for resident in residents.values():
            resident.stop()
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
        with socket.socket() as probe:
            refused = probe.connect_ex(('127.0.0.1', int(origin.rsplit(':', 1)[1]))) == errno.ECONNREFUSED
        assert refused
        write(output / 'origin-stopped.json', {'tcp_refused': True, 'before_cold_reopen': True})
        for role in ['founder', 'joiner']:
            status = residents[role].start()
            assert status['ready'] and status['lock'] == 'unlocked' and not status['installed']
        cold_founder = launch('cold-founder', 'cold_founder.scn', 'founder')
        wait(lambda: (control / 'cold-founder.listening').exists(), 'cold founder listening')
        card = read(exchange / 'cold.place-card.json')
        card['binding']['root'] = 'ff' * 32
        write(exchange / 'foreign.place-card.json', card)
        cold_joiner = launch('cold-joiner', 'cold_joiner.scn', 'joiner')
        exited(cold_joiner)
        exited(cold_founder)
        for role in ['founder', 'joiner']:
            residents[role].stop()
        write(output / 'native-complete.json', {
            'result': 'pass', 'origin_stopped_before_cold_reopen': True,
            'runs': {run.directory.name: read(run.directory / 'process.json') for run in native},
        })
    finally:
        for run in native:
            run.stop_owned()
        for resident in residents.values():
            resident.terminate_owned()
        server.shutdown()
        server.server_close()
        awake.terminate()
        awake.wait(timeout=5)


if __name__ == '__main__':
    main()
