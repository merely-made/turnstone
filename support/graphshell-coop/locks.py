#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Locks for the harness. No Cargo.lock is tracked (engine-split D139): what cargo
resolved for a commit is a git note read with `house resolved show <commit>`.

A lock on disk is used when it is there; a lock for a commit comes from the note.
"""
import hashlib
import os
from pathlib import Path
import subprocess

NOTES = 'refs/notes/resolved'
MARK = b'# ---- '


class NoLock(Exception):
    pass


def parse(shown):
    """The `# ---- <path>` sections of `house resolved show` output: {path: bytes}."""
    sections, path, body = {}, None, []
    for line in shown.splitlines(keepends=True):
        if line.startswith(MARK):
            if path is not None:
                sections[path] = b''.join(body)
            path, body = line[len(MARK):].decode().strip(), []
        elif path is not None:
            body.append(line)
    if path is not None:
        sections[path] = b''.join(body)
    return sections


def note(repo, commit):
    """Every lock the resolved note holds for `commit` in `repo`: {path: bytes}."""
    command = ['house', 'resolved', 'show', commit]
    shown = subprocess.run(command, cwd=repo, capture_output=True)
    if shown.returncode != 0:  # A fresh clone has no notes yet.
        subprocess.run(['git', 'fetch', '--quiet', 'origin', NOTES + ':' + NOTES], cwd=repo,
                       capture_output=True)
        shown = subprocess.run(command, cwd=repo, capture_output=True)
    if shown.returncode != 0:
        raise NoLock((str(repo), commit, 'no resolved note: ' + shown.stderr.decode().strip()))
    return parse(shown.stdout)


def at_commit(repo, commit, relative):
    """The lock `relative` as cargo resolved it for `commit`: the note's. A commit from
    before D139 has no note but tracked the lock, and its own blob is that lock."""
    try:
        sections = note(repo, commit)
    except NoLock:
        sections = {}
    if relative in sections:
        return sections[relative]
    blob = subprocess.run(['git', 'show', commit + ':' + relative], cwd=repo, capture_output=True,
                          env={**os.environ, 'MSYS_NO_PATHCONV': '1'})
    if blob.returncode != 0:
        raise NoLock((str(repo), commit, relative + ' is in neither the resolved note nor the commit'))
    return blob.stdout


def current(repo, relative, commit='HEAD'):
    """The lock on disk when there is one, else the note's for `commit`."""
    path = Path(repo) / relative
    return path.read_bytes() if path.exists() else at_commit(repo, commit, relative)


def ensure(repo, commit='HEAD'):
    """Put the note's locks for `commit` on disk where none is, before a build.
    A commit with no note is left alone: cargo resolves, and the receipt records that."""
    try:
        sections = note(repo, commit)
    except NoLock:
        return []
    written = []
    for relative, body in sections.items():
        path = Path(repo) / relative
        if not path.exists() and path.parent.is_dir():
            path.write_bytes(body)
            written.append(relative)
    return written


def sha256(data):
    return hashlib.sha256(data).hexdigest()
