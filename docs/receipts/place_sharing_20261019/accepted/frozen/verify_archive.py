#!/usr/bin/env python3
"""Check a committed source archive against the compiled and public receipts."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('commit', nargs='?', default='HEAD')
    options = parser.parse_args()
    commit = subprocess.check_output(['git', 'rev-parse', options.commit], cwd=ROOT, text=True).strip()
    data = subprocess.check_output(['git', 'archive', '--format=tar', commit], cwd=ROOT)
    prefix = HERE.relative_to(ROOT).as_posix() + '/'
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        def content(name):
            file = archive.extractfile(name)
            assert file is not None, name
            return file.read()

        inputs = json.loads(content(prefix + 'accepted/inputs.json'))
        artifacts = json.loads(content(prefix + 'artifact-sha256.json'))
        for name, digest in inputs['source_sha256'].items():
            assert hashlib.sha256(content(name)).hexdigest() == digest, 'compiled source drift: ' + name
        for name, digest in artifacts.items():
            assert hashlib.sha256(content(prefix + name)).hexdigest() == digest, 'public artifact drift: ' + name
        for name, digest in inputs['harness_sha256'].items():
            assert hashlib.sha256(content(prefix + 'accepted/frozen/' + name)).hexdigest() == digest, name
    print(json.dumps({'result': 'pass', 'commit': commit,
                      'compiled_source_files': len(inputs['source_sha256']),
                      'public_artifacts': len(artifacts), 'git_archive_bytes': len(data)}, indent=2))


if __name__ == '__main__':
    main()
