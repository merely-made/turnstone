#!/usr/bin/env python3
"""Review or invoke one serialized lane through the existing native adapter.

This wrapper manages isolated paths and receipt checks, never windows or input.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys

BASE = Path(__file__).resolve().parent
ADAPTER = Path('/Users/markik/Code/worktrees/mere-tabard-apps/scripts/run_macos_scenario.py')
LANES = {
    'seed': ('tabard_application.scn', 'tabard_workshop.scn', 7),
    'reopen': ('tabard_application_reopen.scn', None, 2),
    'same-id': ('tabard_application_same_id.scn', 'tabard_workshop_same_id.scn', 4),
    'same-id-reopen': ('tabard_application_same_id_reopen.scn', None, 2),
}


def resets():
    return [{"path": str(path), "mtime_ns": path.stat().st_mtime_ns}
            for path in sorted(Path('/Library/Logs/DiagnosticReports').glob('*.gpuRestart'))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--lane', choices=LANES, required=True)
    parser.add_argument('--run', action='store_true', help='Launch only after CPU/build and exclusive GPU handoff')
    options = parser.parse_args()
    main_scenario, editor_scenario, expected = LANES[options.lane]
    output = BASE / options.lane
    environment = {
        'TURNSTONE_ROOT': str(BASE / 'turnstone-profile'),
        'MERE_ROOT': str(BASE / 'mere-profile'),
        'XDG_DATA_HOME': str(BASE / 'xdg-data'),
        'XDG_CONFIG_HOME': str(BASE / 'xdg-config'),
        'XDG_RUNTIME_DIR': '/tmp/turnstone-tabard13-gzrzoriq',
        'GRAPHSHELL_APP_ENDPOINT': '/tmp/turnstone-tabard13-gzrzoriq/graphshell-app.sock',
        'TURNSTONE_THEME_LIBRARY': str(BASE / 'library/themes.json'),
        'PERSONAE_PROFILE': 'turnstone-tabard-native-acceptance',
        'RUST_LOG': 'info',
        'CAMBIUM_HOST_PERF_TRACE': '1',
        'MESQUITE_CAPTURE_PAINT': '1',
    }
    if editor_scenario:
        environment.update({
            'TURNSTONE_THEME_SCENARIO': str(BASE / 'scenarios' / editor_scenario),
            'TURNSTONE_THEME_CAPTURE_DIR': str(output / 'workshop'),
            'TURNSTONE_THEME_RECEIPT': str(output / 'workshop/scenario.done'),
        })
    command = [sys.executable, str(ADAPTER), '--binary', str(options.binary.resolve()),
               '--prefix', 'TURNSTONE', '--scenario', str(BASE / 'scenarios' / main_scenario),
               '--output', str(output / 'app'), '--timeout', '180']
    for key, value in environment.items():
        command += ['--env', key + '=' + value]
    print(json.dumps({'lane': options.lane, 'argv': command, 'run': options.run}, indent=2), flush=True)
    if not options.run:
        return 0
    if options.lane != 'seed' and not (BASE / 'library/themes.json').is_file():
        raise RuntimeError('accepted authored library is required before restoration/edit lanes')
    output.mkdir(exist_ok=False)
    (output / 'argv.json').write_text(json.dumps(command, indent=2) + '\n')
    before = resets()
    (output / 'gpu-reset-before.json').write_text(json.dumps(before, indent=2) + '\n')
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, timeout=200)
    (output / 'adapter.log').write_text(result.stdout)
    print(result.stdout, end='', flush=True)
    after = resets()
    (output / 'gpu-reset-after.json').write_text(json.dumps(after, indent=2) + '\n')
    if result.returncode != 0:
        raise RuntimeError('native adapter failed; retained receipt and logs')
    if after != before:
        raise RuntimeError('GPU restart reports changed; retain receipt and stop native lanes')
    receipt = (output / 'app/scenario.done').read_text()
    if not receipt.startswith('RESULT ok\n') or ' blank=0\n' not in receipt:
        raise RuntimeError('main presentation/capture receipt did not qualify')
    if len(list((output / 'app').glob('*.png'))) != expected:
        raise RuntimeError('main capture set is incomplete')
    if editor_scenario:
        workshop = output / 'workshop'
        if not (workshop / 'scenario.done').read_text().startswith('RESULT ok\n'):
            raise RuntimeError('workshop receipt did not qualify')
        if len(list(workshop.glob('*.png'))) != 2:
            raise RuntimeError('workshop capture set is incomplete')
    print('Receipt checks passed; all PNGs still require visual inspection.', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
