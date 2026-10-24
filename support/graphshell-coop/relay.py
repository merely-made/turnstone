"""Own relay processes across a controlled outage; JSON control only, no forwarding."""
import hashlib
import json
from pathlib import Path
import queue
import shlex
import subprocess
import threading
import time
from urllib.parse import urlsplit

class Relay:
    def __init__(self, args):
        self.args = args
        self.ssh = ['ssh', '-S', str(args.ssh_control), args.guest_host]
        command = ['python3', '-c', 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())', args.relay_binary]
        assert subprocess.check_output(self.ssh + [shlex.join(command)], text=True).strip() == args.relay_sha256
        self.segments = []
        self.process = None
        self.url = None

    def next_event(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('relay observation deadline elapsed')
        line = self.lines.get(timeout=remaining)
        assert line, 'relay closed before the expected observation'
        value = json.loads(line)
        self.segments[-1]['events'].append(value)
        return value

    def until(self, event):
        deadline = time.monotonic() + 60
        while True:
            value = self.next_event(deadline)
            if value['event'] == event:
                return value

    def wait_for_rejoined(self):
        """A listening relay does not yet prove either endpoint is reachable."""
        expected = {e['endpoint'] for e in self.segments[0]['events']
                    if e['event'] == 'connect'}
        assert len(expected) == 2, 'recovery must retain the same two endpoints'
        deadline = time.monotonic() + 60
        while not expected.issubset({e['endpoint'] for e in self.segments[-1]['events']
                                    if e['event'] == 'connect'}):
            self.next_event(deadline)
        events = self.segments[-1]['events']
        return {'endpoints': sorted(expected),
                'registrations': {peer: min(e['at_ms'] for e in events
                                           if e['event'] == 'connect' and e['endpoint'] == peer)
                                  for peer in sorted(expected)},
                'relay_pid': next(e['pid'] for e in events if e['event'] == 'ready')}

    def start(self):
        assert self.process is None
        bind = self.args.relay_bind if not self.url else urlsplit(self.url).netloc
        log = self.args.output.parent / (self.args.output.name + '-relay-' + str(len(self.segments)+1) + '.log')
        self.log = log.open('w')
        self.process = subprocess.Popen(self.ssh + [shlex.join([self.args.relay_binary, bind])],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.log, text=True, bufsize=1)
        self.lines = queue.Queue()
        stream, lines = self.process.stdout, self.lines
        def read():
            for line in stream:
                lines.put(line)
            lines.put('')
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        self.segments.append({'events': [], 'log': log.name})
        ready = self.until('ready')
        assert self.url is None or ready['url'] == self.url
        self.url = ready['url']
        self.process.stdin.write('snapshot\n')
        self.process.stdin.flush()
        self.until('snapshot')
        return self.url

    def stop(self):
        if self.process is None:
            return
        self.process.stdin.write('stop\n')
        self.process.stdin.flush()
        self.until('stopped')
        self.process.stdin.close()
        self.process.wait(timeout=30)
        self.reader.join(timeout=5)
        self.log.close()
        self.segments[-1]['exit'] = self.process.returncode
        assert self.process.returncode == 0
        self.process = None

    def write(self):
        out = self.args.output
        if out.exists():
            value = {'exit': 0, 'sha256': self.args.relay_sha256, 'segments': self.segments,
                     'events': [e for s in self.segments for e in s['events']]}
            (out / 'relay-server.json').write_text(json.dumps(value, indent=2)+'\n')
            if (out/'accepted/qualification.json').exists():
                (out/'accepted/relay-server.json').write_text(json.dumps(value, indent=2)+'\n')
