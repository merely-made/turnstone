# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Freeze caller-built web and native candidates without executing acceptance."""
from pathlib import Path
import shutil
import subprocess
from common import parser, fresh_output, sha, write_json, check_budget, copy_tree


def main():
    cli = parser(__doc__)
    cli.add_argument('--web', type=Path, required=True,
                     help='Already-built site, including pkg and scenario inputs')
    cli.add_argument('--native-binary', type=Path, required=True)
    cli.add_argument('--build-source', required=True,
                     help='Full commit used by the successful build; must be current HEAD')
    cli.add_argument('--compiler', required=True, help='Actual compiler recorded by that build')
    args = cli.parse_args()
    repo = args.repo.resolve()
    head = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
    if head != args.build_source:
        cli.error('Build source must be the current full commit')
    web, binary = args.web.resolve(), args.native_binary.resolve()
    if (not (web / 'workspace.html').is_file() or not (web / 'pkg/graphshell_web_bg.wasm').is_file()
            or not (web / 'pkg/graphshell_web.js').is_file() or not binary.is_file()):
        cli.error('The caller-built site, WebAssembly package and native binary must exist')
    # Capture the original browser/native source owners and root dependency inputs.
    owners = ['crates/cambium', 'ports/graphshell', 'ports/tabard', 'crates/genet',
              'Cargo.toml', 'Cargo.lock', 'scripts/run_macos_scenario.py']
    paths = subprocess.check_output(['git', '-C', str(repo), 'ls-files', '-z', '--', *owners]).decode().split('\0')
    paths = [p for p in paths if p and (repo / p).is_file() and
             (Path(p).suffix in {'.rs', '.css', '.js', '.html', '.wgsl', '.scn', '.py'}
              or Path(p).name in {'Cargo.toml', 'Cargo.lock'})]
    if subprocess.check_output(['git', '-C', str(repo), 'diff', 'HEAD', '--', *owners]):
        cli.error('Candidate source inputs must be committed')
    if check_budget(web, args.max_bytes) + binary.stat().st_size > args.max_bytes:
        cli.error('Combined candidate exceeds the receipt budget')
    out = fresh_output(args.output, repo, web, binary)
    copy_tree(web, out / 'web', args.max_bytes)
    (out / 'frozen').mkdir()
    native = out / 'frozen' / binary.name
    shutil.copy2(binary, native)
    sources = {p: sha(repo / p) for p in sorted(paths)}
    common = {'source': head, 'build_source': head, 'repo': str(repo),
              'compiler': args.compiler, 'source_sha256': sources,
              'build_source_is_caller_declared': True,
              'browser_executed': False, 'native_executed': False}
    write_json(out / 'candidate.json', {**common, 'package_sha256': {
        p.relative_to(out).as_posix(): sha(p) for p in sorted((out / 'web').rglob('*')) if p.is_file()}})
    write_json(out / 'native-candidate.json', {**common, 'binary': str(native), 'binary_sha256': sha(native)})
    check_budget(out, args.max_bytes)


if __name__ == '__main__':
    main()
