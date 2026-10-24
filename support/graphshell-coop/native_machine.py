#!/usr/bin/env python3
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
import subprocess
import sys
import traceback
import uuid


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def desktop():
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
    screen = subprocess.check_output(['gdbus', 'call', '--session', '--dest', 'org.gnome.ScreenSaver',
                                      '--object-path', '/org/gnome/ScreenSaver', '--method',
                                      'org.gnome.ScreenSaver.GetActive'],
                                     env=os.environ | graphical, text=True).strip()
    assert screen in {'(true,)', '(false,)'}, screen
    adapters = sorted(Path('/usr/share/vulkan/icd.d').glob('*radeon*.json'))
    adapters = [p for p in adapters if platform.machine() in p.name]
    assert len(adapters) == 1, adapters
    # Qualify the installed Radeon driver; do not silently select llvmpipe.
    graphical['VK_DRIVER_FILES'] = str(adapters[0])
    addresses = json.loads(subprocess.check_output(['ip', '-j', 'address', 'show'], text=True))
    distro = dict(line.split('=', 1) for line in Path('/etc/os-release').read_text().splitlines()
                  if '=' in line)
    return Path('/etc/machine-id').read_text().strip(), {
        'console_user': state.get('Name'),
        'console_locked': state.get('LockedHint') != 'no' or screen != '(false,)',
        'console_present': state.get('Active') == 'yes' and state.get('Remote') == 'no',
        'operating_system': 'Linux', 'system_version': distro['PRETTY_NAME'].strip('"'),
        'kernel': platform.release(), 'physical_model': Path('/sys/class/dmi/id/product_name').read_text().strip(),
        'desktop': state | {'session_id': session, 'gnome_screensaver': screen},
        'graphical_environment': graphical,
        'addresses': sorted({a['local'] for interface in addresses for a in interface['addr_info']}),
    }


class Machine:
    def __init__(self, args):
        self.args = args
        self.output = args.output.resolve()
        self.output.mkdir(mode=0o700, parents=True)
        self.public = self.output / 'evidence'
        self.private = self.output / 'private'
        self.public.mkdir()
        self.private.mkdir(mode=0o700)
        from resident import Resident
        self.resident = Resident(args.djinn, self.private / args.role,
                                 'CrossGraph' + args.role.title(),
                                 self.public / (args.role + '-resident'))
        if args.role == 'owner':
            settings = {'reservoir': {'enabled': True, 'persona': str(uuid.uuid4())}}
            path = Path(self.resident.environment['XDG_DATA_HOME'])
            write(path / 'graphshell/settings/CrossGraphOwner.json', settings)
            write(self.public / 'owner-settings.json', settings)
        os.chmod(self.resident.environment['XDG_RUNTIME_DIR'], 0o700)
        _, self.desktop_state = desktop()
        self.resident.environment.update(self.desktop_state['graphical_environment'])
        self.phases = {}
        self.bundle = self.private / (args.role + '.app')
        self.executable = (self.bundle / 'Contents/MacOS/graphshell_mere'
                           if platform.system() == 'Darwin' else self.private / 'native/graphshell_mere')
        self.handoff = self.private / 'invitation'
        self.contact = self.private / 'guest-contact.json'
        write(self.public / (args.role + '-machine.json'), self.info())

    def info(self):
        hardware, state = desktop()
        binaries = {'graphshell_mere': sha(self.args.binary), 'djinn': sha(self.args.djinn)}
        if self.args.prepare:
            binaries['coop_existing_graph'] = sha(self.args.prepare)
        return {
            'role': self.args.role,
            'machine_id': hashlib.sha256(('graphshell-coop-v1:' + hardware).encode()).hexdigest(),
            'hostname': platform.node(), 'architecture': platform.machine(),
            **state,
            'worker_pid': os.getpid(),
            'worker_sha256': sha(Path(__file__)),
            'source_commit': subprocess.check_output(['git', '-C', str(self.args.repo),
                                                      'rev-parse', 'HEAD'], text=True).strip(),
            'lock_sha256': sha(self.args.repo / 'Cargo.lock'),
            'binaries': binaries,
            'public': str(self.public), 'private': str(self.private),
        }

    def path(self, area, relative):
        base = self.public if area == 'public' else self.private
        path = (base / relative).resolve()
        assert path.is_relative_to(base) and path != base, relative
        return path

    def invoke(self, binary, arguments):
        result = subprocess.run([str(binary), *arguments], env=self.resident.environment,
                                capture_output=True, text=True, timeout=90)
        if result.returncode:
            raise RuntimeError(result.stderr + '\n' + result.stdout)
        return result.stdout

    def start(self):
        return self.resident.start()

    def stop(self):
        assert all(p.poll() is not None for p in self.phases.values()), 'native phase still running'
        self.resident.stop()
        return {'resident_exit': self.resident.process.returncode, 'starts': self.resident.starts}

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
        path.touch()
        return True

    def launch(self, name, scenario, domain):
        assert re.fullmatch(r'[a-z-]+', name) and name not in self.phases
        state = self.info()
        assert state['console_present'] and not state['console_locked'], 'headed desktop is locked'
        assert state['console_user'] == getpass.getuser(), 'another desktop user is active'
        if not self.executable.exists():
            if platform.system() == 'Darwin':
                shutil.copytree(self.args.binary.parent / 'GraphshellCoop.app', self.bundle)
            else:
                self.executable.parent.mkdir()
                shutil.copy2(self.args.binary, self.executable)
        if platform.system() == 'Darwin':
            subprocess.run(['/usr/bin/codesign', '--verify', '--strict', str(self.bundle)], check=True)
        assert sha(self.executable) == state['binaries']['graphshell_mere']
        scenario_path = self.public / (name + '.scn')
        scenario_path.write_text(scenario)
        env = self.resident.environment | {
            'GRAPHSHELL_MERE_SCENARIO': str(scenario_path),
            'GRAPHSHELL_MERE_RECEIPT': str(self.public / (name + '.json')),
            'GRAPHSHELL_MERE_JSON_RECEIPT': '1',
            'GRAPHSHELL_MERE_PRESENTATION_TIMEOUT_SECS': '60', 'RUST_LOG': 'warn',
        }
        # Launch the owned bundle executable directly. LaunchServices' open -W
        # can return before it has a native PID on a remote desktop session.
        # Popen owns the actual native lifecycle and reports its exit status.
        command = [str(self.executable)]
        if self.args.role == 'guest':
            command += ['--invitation', str(self.handoff / 'invitation.json'),
                        '--contact', str(self.handoff / 'contact.json')]
        else:
            command += [domain]
        with (self.public / (name + '.log')).open('w') as log:
            process = subprocess.Popen(command, env=env, stdout=log, stderr=log)
        self.phases[name] = process
        write(self.public / (name + '-launch.json'),
              {'launcher_pid': process.pid, 'machine': state, 'scenario': scenario_path.name})
        return {'launcher_pid': process.pid}

    def progress(self, name, capture=None):
        process = self.phases[name]
        receipt = self.public / (name + '.json')
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
                                    if 'libvulkan_radeon' in line or '/dri/' in line})
                write(self.public / (name + '-graphics.json'),
                      {'pid': process.pid, 'render_nodes': nodes, 'driver_libraries': libraries,
                       'VK_DRIVER_FILES': self.resident.environment.get('VK_DRIVER_FILES')})
        return {'running': process.poll() is None, 'exit': process.returncode,
                'receipt': receipt.exists(),
                'capture': bool(capture and self.path('public', capture).exists())}

    def finish(self, name):
        process = self.phases[name]
        process.wait(timeout=30)
        assert process.returncode == 0, (name, process.returncode)
        receipt = json.loads((self.public / (name + '.json')).read_text())
        assert receipt['ok'] and not receipt['errors'], receipt
        assert receipt['captures'] and all(c['width'] > 200 and c['height'] > 200
                                           for c in receipt['captures'])
        return receipt

    def collect(self, require_clean=True):
        if require_clean:
            assert all(p.poll() is not None for p in self.phases.values())
            assert self.resident.process.poll() == 0
        return {str(p.relative_to(self.public)): base64.b64encode(p.read_bytes()).decode()
                for p in self.public.rglob('*') if p.is_file()}

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
    for name in ['repo', 'binary', 'djinn', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--prepare', type=Path)
    parser.add_argument('--role', choices=['owner', 'guest'], required=True)
    machine = Machine(parser.parse_args())
    allowed = {'info', 'start', 'stop', 'seed', 'prepare', 'export_contact', 'owner_contact',
               'exported_handoff', 'install_handoff', 'marker', 'launch', 'progress',
               'finish', 'collect'}
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
