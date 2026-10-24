#!/usr/bin/env python3
"""Isolated real Djinn processes, using their ordinary custody and control doors."""
import json
import os
from pathlib import Path
import secrets
import subprocess
import time


class Resident:
    def __init__(self, binary, root, name, evidence):
        self.binary = Path(binary).resolve()
        self.root = Path(root).resolve()
        self.name = name
        self.evidence = Path(evidence)
        self.evidence.mkdir(parents=True, exist_ok=True)
        self.passphrase = secrets.token_urlsafe(32)
        self.starts = 0
        self.process = None
        self.stream = None
        self.environment = {
            k: v for k, v in os.environ.items()
            if not k.startswith(('TURNSTONE_', 'GRAPHSHELL_', 'PERSONAE_', 'MERE_'))
        }
        for key, directory in {
            'XDG_DATA_HOME': 'local', 'LOCALAPPDATA': 'local',
            'XDG_CONFIG_HOME': 'config', 'XDG_CACHE_HOME': 'cache',
            'XDG_RUNTIME_DIR': 'runtime', 'MERE_ROOT': 'mere', 'TMPDIR': 'tmp',
        }.items():
            path = self.root / directory
            path.mkdir(parents=True, exist_ok=True)
            self.environment[key] = str(path)
        self.environment['GRAPHSHELL_APP_ENDPOINT'] = str(self.root / 'app.sock')
        self.environment['GRAPHSHELL_DEVICE_ENDPOINT'] = str(self.root / 'browser.sock')
        self.environment.pop('SSH_AUTH_SOCK', None)
        self.environment.pop('SSH_AGENT_PID', None)
        self.environment['TURNSTONE_ROOT'] = str(self.root / 'turnstone')

    def control(self, flag):
        return subprocess.run([
            str(self.binary), flag, '--app-endpoint', self.environment['GRAPHSHELL_APP_ENDPOINT'],
        ], env=self.environment, capture_output=True, text=True, timeout=20, check=True)

    def start(self):
        assert self.process is None or self.process.poll() is not None
        self.starts += 1
        events = self.evidence / f'events-{self.starts}.jsonl'
        command = [str(self.binary), '--dir', str(self.root / 'vault'), '--profile', self.name,
                   '--receipt-agent-endpoint', str(self.root / 'agent.sock'),
                   '--browser-endpoint', self.environment['GRAPHSHELL_DEVICE_ENDPOINT'],
                   '--app-endpoint', self.environment['GRAPHSHELL_APP_ENDPOINT'],
                   '--data-root', str(self.root / 'data'), '--events-file', str(events),
                   '--log-file', str(self.evidence / f'djinn-{self.starts}.log'),
                   '--log-filter', 'warn,djinn=info', '--passphrase-fd', '0']
        self.stream = (self.evidence / f'stderr-{self.starts}.log').open('w')
        self.process = subprocess.Popen(command, env=self.environment, stdin=subprocess.PIPE,
                                        stdout=self.stream, stderr=subprocess.STDOUT, text=True)
        self.process.stdin.write(self.passphrase + '\n')
        self.process.stdin.close()
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            assert self.process.poll() is None, f'{self.name} resident exited: {self.process.returncode}'
            records = [json.loads(line) for line in events.read_bytes().splitlines(keepends=True)
                       if line.endswith(b'\n')] if events.exists() else []
            if any(record.get('event') == 'ready' for record in records):
                status = json.loads(self.control('--resident-status').stdout)
                (self.evidence / f'status-{self.starts}.json').write_text(json.dumps(status, indent=2) + '\n')
                return status
            time.sleep(.25)
        raise TimeoutError(self.name + ' resident readiness')

    def stop(self):
        if self.process is not None and self.process.poll() is None:
            self.control('--stop-resident')
            self.process.wait(timeout=60)
            assert self.process.returncode == 0
        if self.stream is not None:
            self.stream.close()

    def terminate_owned(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self.stream is not None:
            self.stream.close()
