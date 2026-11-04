# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Controls for the split verifiers and caller-owned qualification site."""
import base64
from functools import partial
import hashlib
from http.server import ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen
import ordinary_paths as checks
import counted_topology
import retired_decay
import shared_attachments
from qualification_server import Handler


class Tools(unittest.TestCase):
    def test_foreign_host_absolute_paths_and_local_relative_inputs_are_distinct(self):
        self.assertTrue(checks.absolute_evidence_path('/Users/owner/Code/mere'))
        self.assertTrue(checks.absolute_evidence_path('C:/Users/owner/Code/mere'))
        for value in ('relative/source', 'C:relative', '\\rooted-without-drive'):
            self.assertFalse(checks.absolute_evidence_path(value))
        with tempfile.TemporaryDirectory() as temp:
            for value in ('/absolute.json', 'C:/absolute.json', 'C:relative.json', '\\rooted.json'):
                with self.subTest(value=value), self.assertRaises(checks.VerificationError):
                    checks.resolve_relative(Path(temp), value, 'record', '.json')

    def test_shared_checks_and_explicit_git_repository(self):
        self.assertIs(counted_topology.CHECK, checks)
        self.assertIs(retired_decay.CHECK, checks)
        self.assertIs(shared_attachments.CHECK, checks)
        repo = Path(__file__).resolve().parents[2]
        self.assertEqual(counted_topology.REPOSITORY, repo)
        self.assertEqual(retired_decay.REPOSITORY, repo)
        expected = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'])
        self.assertEqual(counted_topology.git_bytes('rev-parse', 'HEAD'), expected)
        self.assertEqual(retired_decay.git_blob(expected.decode().strip(), 'Cargo.toml'),
                         subprocess.check_output(['git', '-C', str(repo), 'show', 'HEAD:Cargo.toml']))

    def test_changed_log_and_package_escape_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'process.log').write_bytes(b'changed')
            with self.assertRaises(checks.VerificationError):
                checks.verify_log(root, {'log_sha256': hashlib.sha256(b'original').hexdigest()}, root / 'process.json')
            with self.assertRaises(checks.VerificationError):
                checks.resolve_relative(root, '../foreign.json', 'record', '.json')

    def test_missing_generated_package_is_a_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'build.json').write_text(json.dumps({
                'schema': 'ordinary-responsive-links-build/v1', 'source_revision': '1' * 40,
                'artifacts': [{'path': 'pkg/app.js', 'sha256': '0' * 64},
                              {'path': 'pkg/app.wasm', 'sha256': '0' * 64}]}))
            with self.assertRaisesRegex(checks.VerificationError, 'missing or empty build artifact'):
                checks.verify_build_manifest(root)

    def test_server_serves_explicit_site_and_separate_package_then_collects(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            site, package, output = (root / n for n in ('site', 'package', 'output'))
            for p in (site, package, output):
                p.mkdir()
            (site / 'index.html').write_bytes(b'explicit site')
            (package / 'app.js').write_bytes(b'packaged script')
            handler = partial(Handler, directory=str(site), package=package, output=output)
            server = ThreadingHTTPServer(('127.0.0.1', 0), handler)
            server.receipt_lock = threading.Lock()
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            origin = f'http://127.0.0.1:{server.server_port}'
            try:
                with urlopen(origin + '/') as result:
                    self.assertEqual(result.read(), b'explicit site')
                with urlopen(origin + '/pkg/app.js') as result:
                    self.assertEqual(result.read(), b'packaged script')
                with self.assertRaises(HTTPError):
                    urlopen(origin + '/pkg/%2e%2e/foreign')
                payload = json.dumps({'scenario': {'state': 'ok'}, 'captures': [
                    {'name': 'image', 'dataUrl': 'data:image/png;base64,' + base64.b64encode(b'capture bytes').decode()}]}).encode()
                request = Request(origin + '/scenario-receipt', payload, headers={
                    'Referer': origin + '/?scenario=p7.scn', 'Content-Type': 'application/json'})
                with urlopen(request) as response:
                    self.assertEqual(response.status, 204)
                self.assertEqual((output / 'p7/image.png').read_bytes(), b'capture bytes')
                self.assertEqual(json.loads((output / 'p7/result.json').read_text())['scenario']['state'], 'ok')
                # A new server invocation must not reuse the collected directory.
                retry = subprocess.run([sys.executable, str(Path(__file__).with_name('qualification_server.py')),
                                        '--site', str(site), '--package', str(package),
                                        '--output', str(output), '--port', '0'], capture_output=True, timeout=10)
                self.assertNotEqual(retry.returncode, 0)
                self.assertEqual((output / 'p7/image.png').read_bytes(), b'capture bytes')
                server.RequestHandlerClass = partial(Handler, directory=str(site), package=package,
                                                     output=output, max_bytes=1)
                with self.assertRaises(HTTPError):
                    urlopen(Request(origin + '/scenario-receipt', payload, headers={
                        'Referer': origin + '/?scenario=overbudget.scn', 'Content-Type': 'application/json'}))
                self.assertFalse((output / 'overbudget/result.json').exists())
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)


if __name__ == '__main__':
    unittest.main()
