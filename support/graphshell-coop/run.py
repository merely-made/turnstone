#!/usr/bin/env python3
"""Existing graph co-op, a Mac and a Linux ThinkPad, no forwarded data-plane ports."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import select
import shlex
import subprocess
import sys
import time


HERE = Path(__file__).resolve().parent


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def await_(**fields):
    return 'await ' + json.dumps(fields) + '\nwait 30000\n'


def click(identifier):
    return 'click role:button @id=' + identifier + '\n'


def input_(index, text):
    return f'click role:textbox @id=coop-field-{index}\ninput-text {text}\n'


def capture(machine, name):
    return 'wheel 0 -10000 px on .coop-panel\nsettle 2\ncapture ' + str(machine.public / (name + '.png')) + '\n'


def barrier(machine, name):
    return await_(file=str(machine.private / 'markers' / name))


RELAY_URL = None


class Worker:
    def __init__(self, role, command, log, trace):
        if role == 'guest' and RELAY_URL:
            command = [*command[:-1], shlex.join(['env', 'GRAPHSHELL_RELAY_ACCEPTANCE_URL=' + RELAY_URL]) + ' ' + command[-1]]
        self.role = role
        self.trace = trace
        self.number = 0
        self.log = log.open('w')
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.log, text=True, bufsize=1)
        self.info = self.call('info')
        self.public = Path(self.info['public'])
        self.private = Path(self.info['private'])

    def call(self, method, **args):
        self.number += 1
        self.process.stdin.write(json.dumps({'id': self.number, 'method': method, 'args': args}) + '\n')
        self.process.stdin.flush()
        assert select.select([self.process.stdout], [], [], 240)[0], (self.role, method, 'RPC timeout')
        line = self.process.stdout.readline()
        assert line, (self.role, method, 'worker closed its control stream')
        response = json.loads(line)
        self.trace.append({'machine': self.role, 'method': method, 'ok': response['ok']})
        assert response['id'] == self.number
        if not response['ok']:
            raise RuntimeError(self.role + ': ' + method + '\n' + response['error'])
        return response['value']

    def launch(self, name, script, domain):
        return self.call('launch', name=name, scenario=script, domain=domain)

    def wait(self, phase, capture=None, seconds=180):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            state = self.call('progress', name=phase, capture=capture)
            ready = state['capture'] if capture else state['receipt']
            if ready:
                return
            assert state['running'], (self.role, phase, state)
            time.sleep(.2)
        raise TimeoutError((self.role, phase, capture))

    def finish(self, name):
        self.wait(name)
        return self.call('finish', name=name)

    def collect(self, destination, require_clean=True):
        files = self.call('collect', require_clean=require_clean)
        for name, encoded in files.items():
            path = (destination / name).resolve()
            assert path.is_relative_to(destination.resolve())
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(base64.b64decode(encoded, validate=True))

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=40)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
        self.log.close()


def workflow(args, relay=None):
    out = args.output.resolve()
    out.mkdir(mode=0o700)
    accepted = out / 'accepted'
    accepted.mkdir()
    trace = []
    workers = []
    receipts = {}
    phases = {'owner': ['baseline', 'owner-live', 'owner-cold', 'private-return'],
              'guest': ['guest-live', 'guest-return', 'guest-cold']}
    common = ['--role', 'owner', '--repo', str(args.repo), '--output', str(out / 'local'),
              '--binary', str(args.owner_binaries / 'graphshell_mere'),
              '--djinn', str(args.owner_binaries / 'djinn'),
              '--prepare', str(args.owner_binaries / 'coop_existing_graph')]
    owner = Worker('owner', [sys.executable, '-u', str(HERE / 'machine.py'), *common],
                   out / 'owner-worker.log', trace)
    workers.append(owner)
    remote = ['python3', '-u', str(args.guest_worker), '--role', 'guest',
              '--repo', str(args.guest_repo), '--output', str(args.guest_output),
              '--binary', str(args.guest_binaries / 'graphshell_mere'),
              '--djinn', str(args.guest_binaries / 'djinn')]
    try:
        guest = Worker('guest', ['ssh', '-S', str(args.ssh_control), args.guest_host,
                                shlex.join(remote)], out / 'guest-worker.log', trace)
        workers.append(guest)
        topology = {'owner': owner.info, 'guest': guest.info,
                    'ssh_use': 'JSON control, empty barriers and public contact/invitation handoff only; no port forwarding or graph-state injection'}
        assert owner.info['machine_id'] != guest.info['machine_id']
        assert owner.info['source_commit'] == guest.info['source_commit']
        assert owner.info['lock_sha256'] == guest.info['lock_sha256']
        assert owner.info['worker_sha256'] == guest.info['worker_sha256'] == sha(HERE / 'machine.py')
        write(out / 'topology.json', topology)
        for machine in workers:
            assert machine.info['console_present'] and not machine.info['console_locked'], machine.info['role'] + ' desktop locked'
            machine.call('start')
        domain = 'coop-acceptance'
        ids = owner.call('seed', domain=domain)
        chosen, private = ids['chosen'], ids['private']
        assert chosen != private
        before = owner.call('prepare', arguments=['read', domain, chosen])
        existing = next(s['id'] for s in before['snapshot']['scene']['tables']['sources']
                        if s and s['adapter'] == 'mere.graph')
        owner.call('owner_contact', contact=guest.call('export_contact'))
        script = (await_(status='ready', attached=chosen)
                  + 'click role:button @data-session-step=switch:' + private + '\n'
                  + await_(attached=private) + capture(owner, 'private-before')
                  + 'click role:button @data-session-step=switch:' + chosen + '\n'
                  + await_(attached=chosen) + capture(owner, 'existing-graph'))
        owner.launch('baseline', script, domain)
        receipts['baseline'] = owner.finish('baseline')
        script = (await_(status='ready', attached=chosen) + click('coop-use')
                  + await_(**{'coop-nodes': '1'}) + capture(owner, 'owner-existing')
                  + input_(2, owner.private / 'guest-contact.json')
                  + input_(3, owner.private / 'invitation')
                  + 'click role:button Invite editor\n'
                  + await_(**{'coop-access': 'editor'}) + capture(owner, 'owner-invited')
                  + await_(**{'coop-nodes': '2', 'nodes': '2'}) + capture(owner, 'owner-saw-guest')
                  + 'click role:button @data-session-step=switch:' + private + '\n'
                  + await_(attached=private, **{'coop-available': 'false'})
                  + capture(owner, 'owner-private-during-coop')
                  + barrier(owner, 'guest-still-scoped')
                  + 'click role:button @data-session-step=switch:' + chosen + '\n'
                  + await_(attached=chosen) + click('coop-use')
                  + await_(**{'coop-nodes': '2', 'coop-access': 'editor'})
                  + capture(owner, 'owner-returned-to-chosen')
                  + await_(**{'coop-nodes': '3', 'nodes': '3'})
                  + capture(owner, 'owner-ordinary-push') + barrier(owner, 'revoke-now')
                  + click('coop-revoke') + await_(**{'coop-access': 'revoked'})
                  + capture(owner, 'owner-revoked') + barrier(owner, 'guest-withdrawn'))
        owner.launch('owner-live', script, domain)
        owner.wait('owner-live', 'owner-invited.png')
        handoff = owner.call('exported_handoff')
        original = handoff['invitation_sha256']
        assert guest.call('install_handoff', invitation=handoff['invitation'], contact=handoff['contact']) == original
        script = ('settle 20\n' + capture(guest, 'guest-start')
                  + await_(**{'coop-nodes': '1', 'sessions': '0', 'archives': '0', 'coop': '~active '})
                  + capture(guest, 'guest-joined') + input_(0, 'https://shared.invalid/guest')
                  + input_(1, 'Guest source') + click('coop-add')
                  + await_(**{'coop-nodes': '2', 'coop-status': '~Saved'}) + capture(guest, 'guest-added')
                  + barrier(guest, 'owner-private') + capture(guest, 'guest-still-scoped')
                  + await_(**{'coop-nodes': '3', 'relations': '1'}) + capture(guest, 'guest-push')
                  + 'click role:button @data-key=' + existing + '\n' + await_(selected=existing)
                  + input_(1, 'Guest revised existing source') + click('coop-title')
                  + await_(**{'coop-status': '~Saved', 'coop': '~Guest revised existing source'})
                  + capture(guest, 'guest-renamed'))
        guest.launch('guest-live', script, domain)
        guest.wait('guest-live', 'guest-added.png')
        owner.wait('owner-live', 'owner-private-during-coop.png')
        guest.call('marker', name='owner-private')
        guest.wait('guest-live', 'guest-still-scoped.png')
        owner.call('marker', name='guest-still-scoped')
        owner.wait('owner-live', 'owner-returned-to-chosen.png')
        assert owner.call('exported_handoff')['invitation_sha256'] == original
        owner.call('prepare', arguments=['ordinary-add', domain, chosen, 'https://shared.invalid/ordinary-owner'])
        receipts['guest-live'] = guest.finish('guest-live')
        recovery = ''
        if args.mode == 'recovery':
            recovery = (
                'click role:button @data-key=' + existing + '\n' + await_(selected=existing)
                + input_(1, 'Guest retry after relay recovery')
                + capture(guest, 'guest-before-relay-loss')
                + barrier(guest, 'attempt-offline') + click('coop-title')
                + capture(guest, 'guest-save-pending')
                + await_(**{'coop-available': 'false', 'nodes': '0'})
                + capture(guest, 'guest-save-failed') + barrier(guest, 'relay-restored')
                + 'click role:button Reconnect\n'
                + await_(**{'coop-nodes': '3', 'coop': '~active relay '})
                + capture(guest, 'guest-relay-restored')
                + 'click role:button @data-key=' + existing + '\n' + await_(selected=existing)
                + input_(1, 'Guest retry after relay recovery') + click('coop-title')
                + await_(**{'coop-status': '~Saved', 'coop': '~Guest retry after relay recovery'})
                + capture(guest, 'guest-save-retried'))
        guest.launch('guest-return', await_(**{'coop-nodes': '3', 'coop': '~Guest revised existing source'})
                     + capture(guest, 'guest-return') + recovery
                     + await_(**{'coop-available': 'false', 'nodes': '0', 'selected': ''})
                     + capture(guest, 'guest-withdrawn'), domain)
        guest.wait('guest-return', 'guest-return.png')
        if args.mode == 'recovery':
            guest.wait('guest-return', 'guest-before-relay-loss.png')
            owner.call('prepare', arguments=['read', domain, chosen], save='before-relay-loss.json')
            relay.stop()
            guest.call('marker', name='attempt-offline')
            guest.wait('guest-return', 'guest-save-failed.png', seconds=120)
            owner.call('prepare', arguments=['read', domain, chosen], save='after-failed-save.json')
            relay.start()
            write(accepted / 'relay-ready-for-reconnect.json', relay.wait_for_rejoined())
            guest.call('marker', name='relay-restored')
            guest.wait('guest-return', 'guest-save-retried.png', seconds=120)
        owner.call('marker', name='revoke-now')
        owner.wait('owner-live', 'owner-revoked.png')
        guest.wait('guest-return', 'guest-withdrawn.png')
        owner.call('marker', name='guest-withdrawn')
        receipts['guest-return'] = guest.finish('guest-return')
        receipts['owner-live'] = owner.finish('owner-live')
        owner.call('prepare', arguments=['read', domain, chosen], save='chosen-after-live.json')
        for machine in workers:
            machine.call('stop')
        for machine in workers:
            machine.call('start')
        script = (await_(status='ready', attached=chosen) + click('coop-use')
                  + await_(**{'coop-nodes': '3', 'coop-access': 'revoked', 'coop': ('~Guest retry after relay recovery' if args.mode == 'recovery' else '~Guest revised existing source')})
                  + capture(owner, 'owner-cold-retained') + input_(3, owner.private / 'invitation')
                  + click('coop-export') + await_(**{'coop-nodes': '3'})
                  + input_(0, 'https://shared.invalid/owner-after-revoke') + input_(1, 'Owner after revoke')
                  + click('coop-add') + await_(**{'coop-nodes': '4'}) + capture(owner, 'owner-cold-edited')
                  + barrier(owner, 'guest-cold-denied') + capture(owner, 'owner-cold-refused'))
        owner.launch('owner-cold', script, domain)
        owner.wait('owner-cold', 'owner-cold-edited.png')
        cold = owner.call('exported_handoff')
        assert cold['invitation_sha256'] == original
        assert guest.call('install_handoff', invitation=cold['invitation'], contact=cold['contact']) == original
        guest.launch('guest-cold', await_(**{'coop-status': '~Revoked', 'coop-available': 'false',
                                          'nodes': '0', 'sessions': '0', 'archives': '0'})
                     + capture(guest, 'guest-cold-denied'), domain)
        receipts['guest-cold'] = guest.finish('guest-cold')
        owner.call('marker', name='guest-cold-denied')
        receipts['owner-cold'] = owner.finish('owner-cold')
        owner.call('prepare', arguments=['read', domain, chosen], save='chosen-after-cold.json')
        owner.launch('private-return', await_(status='ready')
                     + 'click role:button @data-session-step=switch:' + private + '\n'
                     + await_(attached=private) + capture(owner, 'private-after'), domain)
        receipts['private-return'] = owner.finish('private-return')
        owner.call('prepare', arguments=['read', domain, private], save='private-after.json')
        for machine in workers:
            machine.call('stop')
            machine.collect(accepted)
        pids = [{'machine': machine.role, 'pid': int(receipts[name]['final']['pid'])}
                for machine in workers for name in phases[machine.role]]
        assert len({(entry['machine'], entry['pid']) for entry in pids}) == 7
        write(accepted / 'qualification.json', {'result': 'ok', 'graphs': ids, 'existing_surface': existing,
              'machines': topology, 'native_processes': pids, 'phases': phases,
              'original_invitation_sha256': original, 'cold_invitation_sha256': cold['invitation_sha256'],
              'custodian_launches': 4, 'all_owned_processes_stopped': True,
              'mode': args.mode, 'relay_rejoin_barrier': args.mode == 'recovery',
              'controller_sha256': sha(HERE / 'run.py'), 'worker_sha256': sha(HERE / 'machine.py')})
        print(json.dumps({'result': 'ok', 'evidence': str(accepted)}), flush=True)
    except Exception:
        for machine in workers:
            try:
                machine.collect(out / ('failed-' + machine.role), require_clean=False)
            except Exception:
                pass
        raise
    finally:
        write(out / 'control-trace.json', trace)
        for machine in workers:
            machine.close()


def main():
    parser = argparse.ArgumentParser()
    for name in ['repo', 'owner-binaries', 'guest-repo', 'guest-binaries',
                 'guest-worker', 'guest-output', 'ssh-control', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--guest-host', default='thinkpad-l14-f')
    parser.add_argument('--mode', choices=['direct', 'relay', 'recovery'], default='direct')
    parser.add_argument('--relay-binary')
    parser.add_argument('--relay-sha256')
    parser.add_argument('--relay-bind', default='192.168.4.32:0')
    args = parser.parse_args()
    global RELAY_URL
    relay = None
    if args.mode != 'direct':
        from relay import Relay
        relay = Relay(args)
        RELAY_URL = relay.start()
        import os
        os.environ['GRAPHSHELL_RELAY_ACCEPTANCE_URL'] = RELAY_URL
    try:
        workflow(args, relay)
    finally:
        if relay:
            relay.stop()
            relay.write()


if __name__ == '__main__':
    main()
