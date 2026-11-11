# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Two serial source-bound macOS cases; caller owns the native runtime lane."""
import plistlib
import subprocess
import sys
import time
from pathlib import Path
from common import (parser, fresh_output, read_json, write_json, verify_sources,
                    sha, check_budget, console_unlocked)


def main():
    cli = parser(__doc__)
    cli.add_argument('--package', type=Path, required=True)
    cli.add_argument('--launcher', type=Path, help='Explicit maintained launcher when tools and product source differ')
    args = cli.parse_args()
    if sys.platform != 'darwin':
        cli.error('This native runner qualifies macOS only')
    repo, package = args.repo.resolve(), args.package.resolve()
    manifest = read_json(package / 'native-candidate.json')
    if not Path(manifest['binary']).resolve().is_relative_to(package):
        cli.error('The native binary must belong to the frozen package')
    if sha(Path(manifest['binary'])) != manifest['binary_sha256']:
        cli.error('Frozen native binary drift')
    verify_sources(repo, manifest)
    console = plistlib.loads(subprocess.check_output(['/usr/sbin/ioreg', '-n', 'Root', '-d1', '-a']))
    if not console_unlocked(console):
        cli.error('The owned native console must be unlocked')
    out = fresh_output(args.output, repo, package)
    write_json(out / 'input.json', {'source': manifest['source'],
                                   'candidate_sha256': sha(package / 'native-candidate.json')})
    profile = out / 'profile'
    profile.mkdir()
    launcher = (args.launcher or repo / 'scripts/run_macos_scenario.py').resolve(strict=True)
    commands = [{'label': 'native-' + name, 'command': [
        sys.executable, str(launcher),
        '--binary', manifest['binary'], '--prefix', 'TABARD',
        '--scenario', str(repo / f'ports/tabard/desktop/scenarios/{name}.scn'),
        '--output', str(out / ('native-' + name)), '--timeout', '120',
        '--env', 'TABARD_WIDTH=1280', '--env', 'TABARD_HEIGHT=900',
        '--', '--library', str(profile / (name + '.json'))]}
        for name in ('instrument', 'usability')]
    write_json(out / 'native-commands.json', commands)
    write_json(out / 'launcher.json', {'path': str(launcher), 'sha256': sha(launcher)})
    inhibit = subprocess.Popen(['/usr/bin/caffeinate', '-i', '-s', '-t', '300'])
    started = time.monotonic()
    results, failure = [], None
    try:
        for run in commands:
            with (out / (run['label'] + '-launch.log')).open('x') as log:
                result = subprocess.run(run['command'], stdout=log, stderr=subprocess.STDOUT, timeout=130)
            results.append({'label': run['label'], 'exit': result.returncode})
            if result.returncode:
                break
    except Exception as error:
        failure = str(error)
        raise
    finally:
        closed = False
        try:
            inhibit.terminate()
            inhibit.wait(timeout=5)
            closed = True
        finally:
            write_json(out / 'native-release.json', {
                'released_at': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                'elapsed_seconds': round(time.monotonic() - started, 3),
                'inhibitor_closed': closed, 'failure': failure, 'runs': results})
    check_budget(out, args.max_bytes)
    return int(len(results) != 2 or any(r['exit'] for r in results))


if __name__ == '__main__':
    raise SystemExit(main())
