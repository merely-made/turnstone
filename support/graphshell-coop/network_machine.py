#!/usr/bin/env python3
"""Native machine owner with an isolated UDP-blackholed guest network."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import time
from urllib.parse import urlsplit

import native_machine as base

PASTA = ['pasta', '--foreground', '--quiet', '--config-net', '--no-udp',
         '-u', 'none', '-U', 'none', '-t', 'none', '-T', 'none', '-4',
         '--address', '10.77.0.2', '--netmask', '24', '--gateway', '10.77.0.1', '--no-pidns']
ENTRY = '''import json,os,socket,sys,time
from pathlib import Path
from urllib.parse import urlsplit
root=Path(sys.argv[1]);target=urlsplit(sys.argv[2]);port=int(sys.argv[3]);command=json.loads(sys.argv[4])
def write(name,value):(root/name).write_text(json.dumps(value))
write('ready.json',{'pid':os.getpid(),'netns':os.readlink('/proc/self/ns/net')})
end=time.monotonic()+30
while not any(line.split()[1]=='00000000' for line in Path('/proc/net/route').read_text().splitlines()[1:]):
 assert time.monotonic()<end,'namespace route not configured'
 time.sleep(.05)
probe=socket.socket(socket.AF_INET,socket.SOCK_DGRAM)
n=probe.sendto(b'namespace-relay-probe',(target.hostname,port))
with socket.create_connection((target.hostname,target.port),timeout=5):pass
write('probe.json',{'pid':os.getpid(),'netns':os.readlink('/proc/self/ns/net'),'udp_send_accepted':n,'relay_tcp':'connected','uid':os.getuid()})
while not (root/'exec').exists():
 assert time.monotonic()<end,'native launch not released'
 time.sleep(.05)
os.execv(command[0],command)
'''


def await_file(path, process):
    end = time.monotonic() + 30
    while not path.exists():
        assert process.poll() is None, ('namespace entry exited', process.returncode)
        assert time.monotonic() < end, path
        time.sleep(.05)
    return json.loads(path.read_text())


class NativeProcess:
    """Own Pasta's lifecycle while observing its unchanged native child PID."""
    def __init__(self, process, pid):
        self.process, self.pid = process, pid
    @property
    def returncode(self):
        return self.process.returncode
    def poll(self):
        return self.process.poll()
    def wait(self, timeout=None):
        return self.process.wait(timeout=timeout)
    def terminate(self):
        self.process.terminate()
    def kill(self):
        self.process.kill()


class RelayMachine(base.Machine):
    def __init__(self, args):
        self.proxies = {}
        super().__init__(args)
        if args.role == 'owner':
            path = Path(self.resident.environment['XDG_DATA_HOME']) / 'graphshell/settings/CrossGraphOwner.json'
            settings = json.loads(path.read_text())
            settings['coop_relay_urls'] = [os.environ['GRAPHSHELL_RELAY_ACCEPTANCE_URL']]
            base.write(path, settings)
            base.write(self.public / 'owner-settings.json', settings)
        base.write(self.public / (args.role + '-machine.json'), self.info())

    def info(self):
        result = super().info()
        result['base_worker_sha256'] = result['worker_sha256']
        result['worker_sha256'] = base.sha(Path(__file__))
        result['co_op_relay'] = os.environ['GRAPHSHELL_RELAY_ACCEPTANCE_URL']
        return result

    def launch(self, name, scenario, domain):
        if self.args.role != 'guest':
            return super().launch(name, scenario, domain)
        assert sys.platform == 'linux' and base.re.fullmatch(r'[a-z-]+', name) and name not in self.phases
        state = self.info()
        assert state['console_present'] and not state['console_locked']
        if not self.executable.exists():
            self.executable.parent.mkdir()
            shutil.copy2(self.args.binary, self.executable)
        assert base.sha(self.executable) == state['binaries']['graphshell_mere']
        script = self.public / (name + '.scn')
        script.write_text(scenario)
        env = self.resident.environment | {
            'GRAPHSHELL_MERE_SCENARIO': str(script),
            'GRAPHSHELL_MERE_RECEIPT': str(self.public / (name + '.json')),
            'GRAPHSHELL_MERE_JSON_RECEIPT': '1',
            'GRAPHSHELL_MERE_PRESENTATION_TIMEOUT_SECS': '60', 'RUST_LOG': 'warn',
        }
        command = [str(self.executable), '--invitation', str(self.handoff / 'invitation.json'),
                   '--contact', str(self.handoff / 'contact.json')]
        entry = self.private / 'namespace-entry.py'
        entry.write_text(ENTRY)
        rendezvous = self.private / ('namespace-' + name)
        rendezvous.mkdir()
        url = state['co_op_relay']
        host = urlsplit(url).hostname
        assert host in state['addresses'], 'test relay must be on the guest host'
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as receiver:
            receiver.bind((host, 0))
            receiver.settimeout(.5)
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
                control.sendto(b'host-control', receiver.getsockname())
            assert receiver.recv(1024) == b'host-control'
            with (self.public / (name + '.log')).open('w') as log:
                proxy = subprocess.Popen(PASTA + [sys.executable, str(entry),
                                            str(rendezvous), url, str(receiver.getsockname()[1]),
                                            json.dumps(command)], env=env, stdout=log, stderr=log)
            self.proxies[name] = proxy
            ready = await_file(rendezvous / 'ready.json', proxy)
            process = NativeProcess(proxy, ready['pid'])
            self.phases[name] = process
            probe = await_file(rendezvous / 'probe.json', process)
            assert probe['udp_send_accepted'] == len(b'namespace-relay-probe')
            try:
                received = receiver.recv(1024)
            except TimeoutError:
                received = None
            assert received is None, 'guest UDP escaped its isolated namespace'
        host_ns = os.readlink('/proc/self/ns/net')
        assert ready['netns'] == probe['netns'] != host_ns
        base.write(self.public / (name + '-network.json'),
                   {'native_pid': process.pid, 'proxy_pid': proxy.pid, 'probe': probe,
                    'host_netns': host_ns, 'host_udp_control_received': True,
                    'host_received_guest_udp': False, 'pasta_command': PASTA,
                    'pasta_version': subprocess.check_output(['pasta', '--version'], text=True).strip()})
        (rendezvous / 'exec').touch()
        base.write(self.public / (name + '-launch.json'),
                   {'launcher_pid': process.pid, 'machine': state, 'scenario': script.name})
        return {'launcher_pid': process.pid}

    def finish(self, name):
        result = super().finish(name)
        if self.args.role == 'guest':
            for capture in result['captures']:
                update = json.loads(capture['fields']['coop'])
                paths = [p for p in update['connection'] if p.startswith('active ')]
                assert all(p.startswith('active relay ') for p in paths), paths
                if update['data'] and Path(capture['path']).stem != 'guest-start':
                    assert paths, 'no observed relay path for admitted guest'
            self.stop_proxy(name)
        return result

    def stop_proxy(self, name):
        proxy = self.proxies[name]
        if proxy.poll() is None:
            proxy.terminate()
        proxy.wait(timeout=15)
        base.write(self.public / (name + '-network-stop.json'),
                   {'proxy_pid': proxy.pid, 'exit': proxy.returncode, 'stopped': True})

    def cleanup(self):
        try:
            super().cleanup()
        finally:
            for name in self.proxies:
                self.stop_proxy(name)


base.Machine = RelayMachine
if __name__ == '__main__':
    base.main()
