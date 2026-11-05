# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Collect caller-supplied Instrument runs; never replay or relabel old rounds."""
from pathlib import Path
import re
import shutil
import subprocess
from common import (parser, fresh_output, read_json, write_json, verify_sources,
                    verify_release, verify_package, sha, check_budget, copy_tree)


def main():
    cli = parser(__doc__)
    for option in ('package', 'browser', 'native'):
        cli.add_argument('--' + option, type=Path, required=True)
    cli.add_argument('--expected-vendor', default='amd')
    cli.add_argument('--expected-architecture', default='gcn-5')
    args = cli.parse_args()
    from PIL import Image
    repo = args.repo.resolve()
    package, browser_dir, native_dir = (getattr(args, x).resolve() for x in ('package', 'browser', 'native'))
    candidate = read_json(package / 'candidate.json')
    native_candidate = read_json(package / 'native-candidate.json')
    verify_sources(repo, candidate)
    verify_sources(repo, native_candidate)
    if candidate['source'] != native_candidate['source']:
        raise ValueError('Browser and native candidates have different source identities')
    if sha(Path(native_candidate['binary'])) != native_candidate['binary_sha256']:
        raise ValueError('Native candidate binary drift')
    verify_package(package, candidate)
    browser = read_json(browser_dir / 'summary.json')
    if (browser['source'] != candidate['source'] or not browser['headed']
            or browser['page_errors'] != 0 or len(browser['runs']) != 8
            or browser['adapter']['vendor'] != args.expected_vendor
            or browser['adapter']['architecture'] != args.expected_architecture):
        raise ValueError('Browser qualification or expected adapter mismatch')
    verify_release(read_json(browser_dir / 'release.json'))
    runtime = read_json(browser_dir / 'runtime.json')
    if (runtime.get('passed') is not True or runtime.get('owned_pids_remaining') != []
            or not runtime.get('owned_pids_observed') or not runtime.get('temporary_profile_removed')
            or not runtime.get('http_port_rebound') or len(runtime.get('spawned', [])) != 1
            or runtime['spawned'][0].get('exit_observed') is not True):
        raise ValueError('Actual owned browser/process/profile/port release did not pass')
    expected_runs = {'wide-entry', 'forme-original', 'forme-reopen', 'open-reading',
                     'light-workbench', 'narrow-workbench', 'narrow-graph', 'commands-focus'}
    if set(browser['runs']) != expected_runs:
        raise ValueError('Missing or substituted Instrument browser case')
    for run in browser['runs']:
        result = read_json(browser_dir / (run + '.json'))
        if (not (browser_dir / (run + '.png')).is_file()
                or result['scenario']['result']['result'] != 'ok'
                or result['scenario']['gate_failures'] or result['errors'] or result['hidden']):
            raise ValueError('Missing browser capture or failed scenario: ' + run)
    for directory, filename in ((browser_dir, 'candidate.json'),
                                (native_dir, 'native-candidate.json')):
        binding = read_json(directory / 'input.json')
        if (binding.get('source') != candidate['source']
                or binding.get('candidate_sha256') != sha(package / filename)):
            raise ValueError('Run belongs to a different frozen candidate')
    release = read_json(native_dir / 'native-release.json')
    if (release.get('inhibitor_closed') is not True or release.get('failure') is not None
            or {r['label'] for r in release['runs']} != {'native-instrument', 'native-usability'}
            or len(release['runs']) != 2 or any(r['exit'] for r in release['runs'])):
        raise ValueError('Native run or inhibitor release failed')
    native = []
    for label in ('native-instrument', 'native-usability'):
        process = read_json(native_dir / label / 'process.json')
        bundle = read_json(native_dir / label / 'bundle-release.json')
        if bundle.get('removed') is not True or bundle.get('remaining') != [] or bundle.get('retained_bundle'):
            raise ValueError('Owned native application bundle was not released: ' + label)
        scenario = f'ports/tabard/desktop/scenarios/{label.removeprefix("native-")}.scn'
        if (process['binary_sha256'] != native_candidate['binary_sha256']
                or process['scenario_sha256'] != candidate['source_sha256'][scenario]
                or not process.get('owned_pids_observed') or process.get('owned_pids_remaining') != []):
            raise ValueError('Native executable/scenario identity or actual process release mismatch: ' + label)
        receipt = (native_dir / label / 'scenario.done').read_text()
        frames = re.search(r'frames: (\d+) captured, (\d+) blank, (\d+) distinct', receipt)
        presented = re.search(r'presentation: (\d+) frames, (\d+) redraws, (\d+) unpresented', receipt)
        if not process['success'] or process['timeout'] or not receipt.startswith('RESULT ok\n') or not frames or int(frames[2]) or not presented or int(presented[1]) <= 0:
            raise ValueError('Native rendered scenario did not pass: ' + label)
        if len(list((native_dir / label).glob('*.png'))) != int(frames[1]):
            raise ValueError('Native capture count differs from actual PNG files: ' + label)
        native.append({'label': label, 'process': process, 'captured': int(frames[1]), 'presented': int(presented[1])})
    with Image.open(native_dir / 'native-instrument/instrument_light.png') as light, Image.open(native_dir / 'native-instrument/instrument_dark.png') as dark:
        if light.getpixel((20, 20)) != dark.getpixel((20, 20)) or light.getpixel((1700, 500)) == dark.getpixel((1700, 500)):
            raise ValueError('Neutral frame / reader-mode pixel controls failed')
    size = sum(check_budget(p, args.max_bytes) for p in (browser_dir, native_dir))
    if size > args.max_bytes:
        raise ValueError('Combined collection exceeds its receipt budget')
    out = fresh_output(args.output, repo, package, browser_dir, native_dir)
    copy_tree(browser_dir, out / 'browser', args.max_bytes)
    copy_tree(native_dir, out / 'native', args.max_bytes)
    for name in ('candidate.json', 'native-candidate.json'):
        shutil.copy2(package / name, out / name)
    artifacts = {}
    for image in sorted(out.rglob('*.png')):
        with Image.open(image) as pixels:
            colors = pixels.convert('RGB').getcolors(pixels.width * pixels.height)
            if not colors or len(colors) < 128:
                raise ValueError('Blank or low-color capture: ' + str(image))
            artifacts[image.relative_to(out).as_posix()] = {
                'sha256': sha(image), 'width': pixels.width, 'height': pixels.height, 'unique_rgb': len(colors)}
    write_json(out / 'qualification.json', {
        'scope': 'Caller-supplied Instrument browser and macOS pilot; no replay by collector',
        'build_source': candidate['build_source'], 'compiler': candidate['compiler'],
        'current_source': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
        'browser': browser, 'native': native, 'artifacts': artifacts,
        'limits': ['No physical pointer/screen-reader qualification',
                   'No cross-application interoperability or transfer to other consumer apps',
                   'Old Instrument rounds remain historical; this collector does not renew them']})
    check_budget(out, args.max_bytes)


if __name__ == '__main__':
    main()
