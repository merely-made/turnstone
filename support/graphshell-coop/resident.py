#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Isolated real Djinn processes, using their ordinary custody and control doors.

Custody is the djinn-custodian fixture (`coop_fixture_custodian`, Graphshell host/),
served as its own process for the worker's whole life, as an installed custodian
outlives Djinn restarts. The production `passphrase-fd` Djinn reaches it only
through the client file handed to it as `--custodian` (engine-split D130).
"""
import json
import os
from pathlib import Path
import secrets
import select
import subprocess
import time


class Resident:
    def __init__(self, binary, root, name, evidence, custodian):
        self.binary = Path(binary).resolve()
        self.custodian_binary = Path(custodian).resolve()
        self.custodian = None
        self.custodian_log = None
        self.client_file = None
        self.root = Path(root).resolve()
        self.name = name
        self.evidence = Path(evidence)
        self.evidence.mkdir(parents=True, exist_ok=True)
        self.passphrase = secrets.token_urlsafe(32)
        self.starts = 0
        self.custodian_starts = 0
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

    def start_custodian(self):
        if self.custodian is not None:
            assert self.custodian.poll() is None, f'{self.name} custodian exited: {self.custodian.returncode}'
            return
        self.custodian_starts += 1
        self.custodian_log = (self.evidence / 'custodian.log').open('w')
        self.custodian = subprocess.Popen(
            [str(self.custodian_binary), str(self.root / 'custodian'), self.name],
            env=self.environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.custodian_log, text=True)
        self.custodian.stdin.write(self.passphrase + '\n')
        self.custodian.stdin.flush()
        assert select.select([self.custodian.stdout], [], [], 60)[0], self.name + ' custodian readiness'
        ready = json.loads(self.custodian.stdout.readline())
        assert ready['ready'] and ready['profile'] == self.name and ready['custody'] == 'fixture-custodian', ready
        assert ready['pid'] == self.custodian.pid, ready
        (self.evidence / 'custodian.json').write_text(json.dumps(ready, indent=2) + '\n')
        self.client_file = ready['client_file']

    def stop_custodian(self):
        if self.custodian is None:
            return None
        if self.custodian.poll() is None:
            self.custodian.stdin.write('quit\n')
            self.custodian.stdin.close()
            self.custodian.wait(timeout=30)
        self.custodian_log.close()
        return self.custodian.returncode

    def start(self):
        assert self.process is None or self.process.poll() is not None
        self.start_custodian()
        self.starts += 1
        events = self.evidence / f'events-{self.starts}.jsonl'
        command = [str(self.binary), '--dir', str(self.root / 'vault'), '--profile', self.name,
                   '--receipt-agent-endpoint', str(self.root / 'agent.sock'),
                   '--browser-endpoint', self.environment['GRAPHSHELL_DEVICE_ENDPOINT'],
                   '--app-endpoint', self.environment['GRAPHSHELL_APP_ENDPOINT'],
                   '--data-root', str(self.root / 'data'), '--events-file', str(events),
                   '--log-file', str(self.evidence / f'djinn-{self.starts}.log'),
                   '--log-filter', 'warn,djinn=info', '--passphrase-fd', '0',
                   '--custodian', self.client_file]
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
        for process in [self.process, self.custodian]:
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        for stream in [self.stream, self.custodian_log]:
            if stream is not None:
                stream.close()
