#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""One owned native acceptance machine, controlled through JSON lines over stdio.

SSH carries this control channel and public handoff files. It forwards no ports
and delivers no graph state to either native app. Each custodian's random test
passphrase is created and retained only in this process on its own machine.
"""
import argparse
import base64
import getpass
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import signal
import struct
import subprocess
import sys
import traceback
import uuid

import locks

# What a receipt says about its compromises (engine-split D130).
CUSTODY = ('fixture-custodian: djinn_custodian::fixture in its own process (coop_fixture_custodian), '
           'packaged Djinn grants at consent none; production passphrase-fd Djinn via --custodian. '
           'Not the djinn-custodian binary, its confinement, service listener or root-owned client file.')
DOMAIN = 'coop-acceptance'
LINUX_LOADER_FILES = ('ld-linux-x86-64.so.2', 'libc.so.6', 'libm.so.6',
                      'libdl.so.2', 'libpthread.so.0', 'librt.so.1')
GRANT_ORIGIN = ('coop_existing_graph grant turnstone: fixture-only exact domain routes '
                'and custody privileges in profile settings before start')


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def desktop(vulkan_icd=None):
    if platform.system() == 'Darwin':
        expert = plistlib.loads(subprocess.check_output(
            ['/usr/sbin/ioreg', '-rd1', '-c', 'IOPlatformExpertDevice', '-a']))
        root = plistlib.loads(subprocess.check_output(
            ['/usr/sbin/ioreg', '-n', 'Root', '-d1', '-a']))
        root = root[0] if isinstance(root, list) else root
        console = next((u for u in root.get('IOConsoleUsers', [])
                        if u.get('kCGSSessionOnConsoleKey')), {})
        interfaces = subprocess.check_output(['/sbin/ifconfig', '-a'], text=True)
        return expert[0]['IOPlatformUUID'], {
            'console_user': console.get('kCGSSessionUserNameKey'),
            'console_locked': bool(console.get('CGSSessionScreenIsLocked', False)),
            'console_present': bool(console), 'operating_system': 'Darwin',
            'system_version': subprocess.check_output(['/usr/bin/sw_vers', '-productVersion'], text=True).strip(),
            'addresses': sorted(set(re.findall(r'\binet6? ([0-9a-fA-F:.]+(?:%\w+)?)', interfaces))),
            'desktop': {'Type': 'macOS console'}, 'graphical_environment': {},
        }
    assert platform.system() == 'Linux'
    session = subprocess.check_output(['loginctl', 'show-user', getpass.getuser(),
                                       '-p', 'Display', '--value'], text=True).strip()
    raw = subprocess.check_output(['loginctl', 'show-session', session, '-p', 'Name',
                                   '-p', 'Active', '-p', 'State', '-p', 'Type', '-p', 'Remote',
                                   '-p', 'LockedHint', '-p', 'Seat'], text=True)
    state = dict(line.split('=', 1) for line in raw.splitlines() if '=' in line)
    selected = {'DISPLAY', 'WAYLAND_DISPLAY', 'XDG_SESSION_TYPE', 'XDG_CURRENT_DESKTOP',
                'DBUS_SESSION_BUS_ADDRESS', 'XAUTHORITY'}
    raw = subprocess.check_output(['systemctl', '--user', 'show-environment'], text=True)
    env = dict(line.split('=', 1) for line in raw.splitlines() if '=' in line)
    runtime = Path(env['XDG_RUNTIME_DIR'])
    display = Path(env['WAYLAND_DISPLAY'])
    display = display if display.is_absolute() else runtime / display
    assert display.is_socket(), display
    graphical = {k: v for k, v in env.items() if k in selected}
    # The resident uses an isolated runtime directory. Wayland must retain the
    # real desktop socket rather than resolve relative to that private root.
    graphical['WAYLAND_DISPLAY'] = str(display)
    if 'KDE' in graphical.get('XDG_CURRENT_DESKTOP', '').split(':'):
        service, path = 'org.freedesktop.ScreenSaver', '/ScreenSaver'
    else:
        service, path = 'org.gnome.ScreenSaver', '/org/gnome/ScreenSaver'
    screen = subprocess.check_output(['gdbus', 'call', '--session', '--dest', service,
                                      '--object-path', path, '--method', service + '.GetActive'],
                                     env=os.environ | graphical, text=True).strip()
    assert screen in {'(true,)', '(false,)'}, screen
    gpu = vulkan_gpu(vulkan_icd)
    # Select the GPU's own Vulkan driver; never silently fall back to llvmpipe.
    graphical['VK_DRIVER_FILES'] = gpu['vulkan_icd']
    addresses = json.loads(subprocess.check_output(['ip', '-j', 'address', 'show'], text=True))
    distro = dict(line.split('=', 1) for line in Path('/etc/os-release').read_text().splitlines()
                  if '=' in line)
    return Path('/etc/machine-id').read_text().strip(), {
        'console_user': state.get('Name'),
        'console_locked': state.get('LockedHint') != 'no' or screen != '(false,)',
        'console_present': state.get('Active') == 'yes' and state.get('Remote') == 'no',
        'operating_system': 'Linux', 'system_version': distro['PRETTY_NAME'].strip('"'),
        'kernel': platform.release(), 'physical_model': Path('/sys/class/dmi/id/product_name').read_text().strip(),
        'desktop': state | {'session_id': session, 'screensaver_service': service, 'screensaver': screen},
        'graphical_environment': graphical, 'gpu': gpu,
        'addresses': sorted({a['local'] for interface in addresses for a in interface['addr_info']}),
    }


# Kernel DRM driver to the Mesa (or vendor) Vulkan driver name it pairs with.
VULKAN_DRIVERS = {'amdgpu': 'radeon', 'radeon': 'radeon', 'i915': 'intel', 'xe': 'intel',
                  'nouveau': 'nouveau', 'nvidia': 'nvidia', 'msm': 'freedreno',
                  'panfrost': 'panfrost', 'asahi': 'asahi', 'virtio_gpu': 'virtio'}


def vulkan_gpu(vulkan_icd=None):
    """The one GPU and its one Vulkan driver, recorded rather than required (coop-harness D3)."""
    nodes = []
    for node in sorted(Path('/sys/class/drm').glob('renderD*')):
        device = node / 'device'
        nodes.append({'render_node': '/dev/dri/' + node.name,
                      'vendor': (device / 'vendor').read_text().strip(),
                      'device': (device / 'device').read_text().strip(),
                      'kernel_driver': Path(os.readlink(device / 'driver')).name})
    assert len(nodes) == 1, ('exactly one GPU render node', nodes)
    name = VULKAN_DRIVERS.get(nodes[0]['kernel_driver'])
    assert name, ('no Vulkan driver known for', nodes[0])
    # Intel's HasVK ICD serves older i915 GPUs that ANV cannot enumerate.
    # An explicit selection is still restricted to the recorded GPU's driver family.
    names = [name, 'intel_hasvk'] if name == 'intel' else [name]
    icds = [p for root in ['/usr/share/vulkan/icd.d', '/etc/vulkan/icd.d']
            for driver in names
            for p in sorted(Path(root).glob(driver + '_icd*.json'))
            if platform.machine() in p.name or p.name == driver + '_icd.json']
    if vulkan_icd:
        icds = [p for p in icds if p.resolve() == vulkan_icd.resolve()]
    else:
        icds = [p for p in icds if p.name.startswith(name + '_icd')]
    assert len(icds) == 1, ('exactly one Vulkan driver for the GPU', icds)
    library = json.loads(icds[0].read_text())['ICD']['library_path']
    return nodes[0] | {'vulkan_icd': str(icds[0]), 'vulkan_library': library}


class Machine:
    def __init__(self, args):
        self.args = args
        self.loader_prefix = []
        self.native_loader = None
        if args.loader:
            assert platform.system() == 'Linux' and platform.machine() == 'x86_64'
            loader = args.loader.resolve(strict=True)
            assert loader.name == LINUX_LOADER_FILES[0] and os.access(loader, os.X_OK), loader
            runtime = loader.parent
            assert all((runtime / n).is_file() and not (runtime / n).is_symlink()
                       for n in LINUX_LOADER_FILES), runtime
            self.loader_prefix = [str(loader), '--library-path', str(runtime)]
            self.native_loader = {'launch_prefix': self.loader_prefix,
                                  'files': {n: sha(runtime / n) for n in LINUX_LOADER_FILES}}
        self.output = args.output.resolve()
        self.output.mkdir(mode=0o700, parents=True)
        self.public = self.output / 'evidence'
        self.private = self.output / 'private'
        self.public.mkdir()
        self.private.mkdir(mode=0o700)
        from resident import Resident
        name = 'CrossGraph' + args.role.title()
        self.resident = Resident(args.djinn, self.private / args.role, name,
                                 self.public / (args.role + '-resident'), args.custodian)
        grant = json.loads(self.invoke(args.prepare, ['grant', 'turnstone', DOMAIN]))
        assert grant['app'] == 'turnstone' and not grant.get('agent'), grant
        write(self.public / (args.role + '-grant.json'),
              {'origin': GRANT_ORIGIN, 'prepare_sha256': sha(args.prepare), 'grant': grant})
        settings = {'applications': [grant]}
        if args.role == 'owner':
            seed_grant = json.loads(self.invoke(args.prepare, ['grant']))
            assert seed_grant['app'] == 'graphshell' and not seed_grant.get('agent'), seed_grant
            write(self.public / 'owner-seed-grant.json', seed_grant)
            settings['applications'].append(seed_grant)
            settings['reservoir'] = {'enabled': True, 'persona': str(uuid.uuid4())}
        path = Path(self.resident.environment['XDG_DATA_HOME'])
        write(path / 'graphshell/settings' / (name + '.json'), settings)
        write(self.public / (args.role + '-settings.json'), settings)
        os.chmod(self.resident.environment['XDG_RUNTIME_DIR'], 0o700)
        _, self.desktop_state = desktop(args.vulkan_icd)
        self.resident.environment.update(self.desktop_state['graphical_environment'])
        self.phases = {}
        self.bundle = self.private / (args.role + '.app')
        self.executable = (self.bundle / 'Contents/MacOS/turnstone'
                           if platform.system() == 'Darwin' else self.private / 'native/turnstone')
        self.handoff = self.private / 'invitation'
        self.contact = self.private / 'guest-contact.json'
        write(self.public / (args.role + '-machine.json'), self.info())

    def info(self):
        hardware, state = desktop(self.args.vulkan_icd)
        binaries = {'turnstone': sha(self.args.binary), 'djinn': sha(self.args.djinn),
                    'coop_existing_graph': sha(self.args.prepare),
                    'coop_fixture_custodian': sha(self.args.custodian)}
        host = self.args.host_repo
        return {
            'role': self.args.role,
            'machine_id': hashlib.sha256(('graphshell-coop-v1:' + hardware).encode()).hexdigest(),
            'hostname': platform.node(), 'architecture': platform.machine(),
            **state,
            'worker_pid': os.getpid(),
            'worker_sha256': sha(Path(__file__)),
            'source_commit': subprocess.check_output(['git', '-C', str(self.args.repo),
                                                      'rev-parse', 'HEAD'], text=True).strip(),
            'lock_sha256': locks.sha256(locks.current(self.args.repo, 'Cargo.lock')),
            'host_commit': subprocess.check_output(['git', '-C', str(host), 'rev-parse', 'HEAD'],
                                                   text=True).strip(),
            'host_lock_sha256': locks.sha256(locks.current(host, 'host/Cargo.lock')),
            'custody': CUSTODY, 'binaries': binaries, 'viewer': 'turnstone',
            'native_loader': self.native_loader,
            'public': str(self.public), 'private': str(self.private),
        }

    def build_inputs(self):
        return json.loads((self.args.binary.parent / 'build-inputs.json').read_text())

    def path(self, area, relative):
        base = self.public if area == 'public' else self.private
        path = (base / relative).resolve()
        assert path.is_relative_to(base) and path != base, relative
        return path

    def invoke(self, binary, arguments):
        command = [str(binary), *arguments]
        if binary == self.args.binary:
            command = self.loader_prefix + command
        result = subprocess.run(command, env=self.resident.environment,
                                capture_output=True, text=True, timeout=90)
        if result.returncode:
            raise RuntimeError(result.stderr + '\n' + result.stdout)
        return result.stdout

    def start(self):
        return self.resident.start()

    def stop(self, final=False):
        assert all(p.poll() is not None for p in self.phases.values()), 'native phase still running'
        self.resident.stop()
        result = {'resident_exit': self.resident.process.returncode, 'starts': self.resident.starts}
        if final:
            result['custodian_exit'] = self.resident.stop_custodian()
            result['custodian_starts'] = self.resident.custodian_starts
            assert result['custodian_exit'] == 0, result
        return result

    def seed(self, domain):
        assert self.args.role == 'owner' and self.args.prepare
        ids = json.loads(self.invoke(self.args.prepare, ['seed', domain]))
        write(self.public / 'existing-graphs.json', ids)
        for name in ['private', 'chosen']:
            self.prepare(['read', domain, ids[name]], name + '-before.json')
        return ids

    def prepare(self, arguments, save=None):
        assert self.args.role == 'owner' and arguments[0] in ['read', 'ordinary-add']
        text = self.invoke(self.args.prepare, arguments)
        if arguments[0] == 'ordinary-add':
            assert text.strip() == 'saved', text
            result = {'acknowledged': True}
        else:
            result = json.loads(text)
        if save:
            write(self.path('public', save), result)
        return result

    def export_contact(self):
        assert self.args.role == 'guest'
        self.invoke(self.args.binary, ['--contact', str(self.contact)])
        return json.loads(self.contact.read_text())

    def owner_contact(self, contact):
        assert self.args.role == 'owner'
        write(self.contact, contact)
        return sha(self.contact)

    def exported_handoff(self):
        assert self.args.role == 'owner'
        return {'invitation': base64.b64encode((self.handoff / 'invitation.json').read_bytes()).decode(),
                'contact': base64.b64encode((self.handoff / 'contact.json').read_bytes()).decode(),
                'invitation_sha256': sha(self.handoff / 'invitation.json')}

    def install_handoff(self, invitation, contact):
        assert self.args.role == 'guest'
        self.handoff.mkdir(exist_ok=True)
        (self.handoff / 'invitation.json').write_bytes(base64.b64decode(invitation, validate=True))
        (self.handoff / 'contact.json').write_bytes(base64.b64decode(contact, validate=True))
        return sha(self.handoff / 'invitation.json')

    def marker(self, name):
        assert re.fullmatch(r'[a-z-]+', name)
        path = self.private / 'markers' / name
        path.parent.mkdir(exist_ok=True)
        path.write_text('ready\n')
        return True

    def phase_path(self, name):
        return self.public / 'native' / name

    def launch_context(self, name, scenario, domain):
        assert domain == DOMAIN
        assert re.fullmatch(r'[a-z-]+', name) and name not in self.phases
        state = self.info()
        assert state['console_present'] and not state['console_locked'], 'headed desktop is locked'
        assert state['console_user'] == getpass.getuser(), 'another desktop user is active'
        if not self.executable.exists():
            if platform.system() == 'Darwin':
                shutil.copytree(self.args.binary.parent / 'TurnstoneCoop.app', self.bundle)
            else:
                self.executable.parent.mkdir()
                shutil.copy2(self.args.binary, self.executable)
        if platform.system() == 'Darwin':
            subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(self.bundle)], check=True)
        assert sha(self.executable) == state['binaries']['turnstone']
        scenario_path = self.public / (name + '.scn')
        scenario_path.write_text(scenario)
        captures = self.phase_path(name)
        captures.mkdir(parents=True)
        env = self.resident.environment | {
            'TURNSTONE_SCENARIO': str(scenario_path),
            'TURNSTONE_CAPTURE_DIR': str(captures),
            'RUST_LOG': 'warn',
        }
        # Launch the owned bundle executable directly. LaunchServices' open -W
        # can return before it has a native PID on a remote desktop session.
        # Popen owns the actual native lifecycle and reports its exit status.
        command = self.loader_prefix + [str(self.executable)]
        if self.args.role == 'guest':
            command += ['--invitation', str(self.handoff / 'invitation.json'),
                        '--contact', str(self.handoff / 'contact.json')]
        else:
            command += ['mere://resident/' + domain]
        return state, scenario_path, env, command

    def launch(self, name, scenario, domain):
        state, scenario_path, env, command = self.launch_context(name, scenario, domain)
        with (self.public / (name + '.log')).open('w') as log:
            process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
        self.phases[name] = process
        write(self.public / (name + '-launch.json'),
              {'launcher_pid': process.pid, 'machine': state, 'scenario': scenario_path.name})
        return {'launcher_pid': process.pid}

    def progress(self, name, capture=None):
        process = self.phases[name]
        phase = self.phase_path(name)
        if (platform.system() == 'Linux' and process.poll() is None
                and not (self.public / (name + '-graphics.json')).exists()):
            proc = Path('/proc') / str(process.pid)
            nodes = []
            for fd in (proc / 'fd').glob('*'):
                try:
                    target = os.readlink(fd)
                    if target.startswith('/dev/dri/'):
                        vendor = Path('/sys/class/drm') / Path(target).name / 'device/vendor'
                        nodes.append({'node': target, 'vendor': vendor.read_text().strip()})
                except FileNotFoundError:
                    pass
            if nodes:
                try:
                    maps = (proc / 'maps').read_text()
                except FileNotFoundError:
                    maps = ''
                libraries = sorted({line.split()[-1] for line in maps.splitlines()
                                    if 'libvulkan_' in line or '/dri/' in line})
                graphics = {'pid': process.pid, 'render_nodes': nodes, 'driver_libraries': libraries,
                            'VK_DRIVER_FILES': self.resident.environment.get('VK_DRIVER_FILES')}
                if self.native_loader:
                    mapped = {Path(line.split()[-1]) for line in maps.splitlines() if '/' in line}
                    runtime = Path(self.loader_prefix[-1])
                    graphics['loader_files'] = {path.name: sha(path) for path in mapped
                                                if path.parent == runtime}
                write(self.public / (name + '-graphics.json'), graphics)
        return {'running': process.poll() is None, 'exit': process.returncode,
                'receipt': (phase / 'scenario.done').is_file(),
                'capture': bool(capture and (phase / Path(capture).name).is_file()
                                and (phase / (Path(capture).stem + '.coop.json')).is_file())}

    def receipt(self, name):
        """Index the native sentinel, screenshots and raw application observations."""
        phase = self.phase_path(name)
        sentinel = phase / 'scenario.done'
        if not sentinel.exists():
            return None
        result, *log = sentinel.read_text().splitlines()
        assert result in {'RESULT ok', 'RESULT fail'}, result
        captures = []
        # Preserve actual scenario order (alphabetical order loses terminal state).
        scenario = (self.public / (name + '.scn')).read_text().splitlines()
        for step in scenario:
            if not step.startswith('capture '):
                continue
            stem = step.removeprefix('capture ')
            artifact = phase / (stem + '.png')
            if not artifact.exists():
                continue
            with artifact.open('rb') as stream:
                header = stream.read(24)
            assert header[:8] == b'\x89PNG\r\n\x1a\n', artifact
            width, height = struct.unpack('>II', header[16:24])
            fields = json.loads((phase / (stem + '.coop.json')).read_text())
            assert int(fields['pid']) == self.phases[name].pid, fields
            captures.append({'path': str(artifact), 'width': width, 'height': height, 'fields': fields})
            # Historical packet paths and their capture-review bindings remain stable.
            artifact.rename(self.public / artifact.name)
        presentation = [line for line in log if line.startswith('presentation frames=')]
        assert len(presentation) == 1, presentation
        proof = re.fullmatch(r'presentation frames=(\d+) captures=(\d+) blank=(\d+)', presentation[0])
        assert proof and int(proof[1]) > 0 and int(proof[2]) == len(captures) and int(proof[3]) == 0, presentation
        ok = result == 'RESULT ok'
        errors = [] if ok else log
        final = dict(captures[-1]['fields']) if captures else {}
        receipt = {'viewer': 'turnstone', 'ok': ok, 'errors': errors, 'final': final, 'captures': captures}
        write(self.public / (name + '.json'), receipt)
        return receipt

    def finish(self, name):
        process = self.phases[name]
        process.wait(timeout=30)
        assert process.returncode == 0, (name, process.returncode)
        receipt = self.receipt(name)
        assert receipt and receipt['ok'] and not receipt['errors'], receipt
        assert receipt['captures'] and all(c['width'] > 200 and c['height'] > 200
                                           for c in receipt['captures'])
        return receipt

    def outcome(self, name, seconds=150):
        """An expected failure must have the application sentinel and actual native exit."""
        process = self.phases[name]
        try:
            process.wait(timeout=seconds)
            terminated = False
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=15)
            terminated = True
        return {'exit': process.returncode, 'terminated': terminated, 'receipt': self.receipt(name)}

    def collect(self, require_clean=True):
        if require_clean:
            assert all(p.poll() is not None for p in self.phases.values())
            assert self.resident.process.poll() == 0
        return {str(p.relative_to(self.public)): base64.b64encode(p.read_bytes()).decode()
                for p in self.public.rglob('*') if p.is_file()}

    def remaining(self):
        """Processes still running an executable from this machine's binaries or private tree."""
        roots = [self.private.resolve()] + [Path(getattr(self.args, n)).resolve().parent
                                            for n in ['binary', 'djinn', 'custodian', 'prepare']]
        if self.native_loader:
            roots.append(Path(self.loader_prefix[-1]))
        listing = subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True)
        return [line.strip() for line in listing.splitlines()
                if len(line.split()) > 1 and any(line.split()[1].startswith(str(r)) for r in roots)]

    def cleanup(self):
        commands = subprocess.check_output(['ps', '-axo', 'pid=,command='], text=True)
        for line in commands.splitlines():
            parts = line.strip().split(None, 1)
            if len(parts) == 2 and parts[1].startswith(str(self.executable)):
                try:
                    os.kill(int(parts[0]), signal.SIGTERM)
                except ProcessLookupError:
                    pass
        for process in self.phases.values():
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
        self.resident.terminate_owned()


def main():
    parser = argparse.ArgumentParser()
    # --prepare (coop_existing_graph) writes the grant on both roles and seeds on the owner;
    # --custodian is coop_fixture_custodian; --host-repo the Graphshell checkout at the pin.
    for name in ['repo', 'host-repo', 'binary', 'djinn', 'custodian', 'prepare', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--vulkan-icd', type=Path)
    parser.add_argument('--loader', type=Path)
    parser.add_argument('--role', choices=['owner', 'guest'], required=True)
    machine = Machine(parser.parse_args())
    allowed = {'info', 'build_inputs', 'start', 'stop', 'seed', 'prepare', 'export_contact', 'owner_contact',
               'exported_handoff', 'install_handoff', 'marker', 'launch', 'progress',
               'finish', 'outcome', 'collect', 'remaining'}
    try:
        for line in sys.stdin:
            request = json.loads(line)
            try:
                assert request['method'] in allowed
                value = getattr(machine, request['method'])(**request.get('args', {}))
                response = {'id': request['id'], 'ok': True, 'value': value}
            except Exception:
                response = {'id': request['id'], 'ok': False, 'error': traceback.format_exc()}
            print(json.dumps(response), flush=True)
    finally:
        machine.cleanup()


if __name__ == '__main__':
    main()
