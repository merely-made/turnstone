#!/usr/bin/env python3

# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Tests for locks.py: the note format, the disk-first rule, and ensure()."""
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import locks

SHOWN = (b'# ---- Cargo.lock\nversion = 4\n\n[[package]]\nname = "a"\n'
         b'# ---- host/Cargo.lock\nversion = 4\n')


class Parse(unittest.TestCase):
    def test_sections_are_split_on_headers(self):
        self.assertEqual(locks.parse(SHOWN), {
            'Cargo.lock': b'version = 4\n\n[[package]]\nname = "a"\n',
            'host/Cargo.lock': b'version = 4\n'})

    def test_empty_output_has_no_sections(self):
        self.assertEqual(locks.parse(b''), {})

    def test_crlf_bytes_are_kept(self):
        self.assertEqual(locks.parse(b'# ---- Cargo.lock\r\nx\r\n'), {'Cargo.lock': b'x\r\n'})


def shown(output):
    return mock.patch('locks.subprocess.run', return_value=subprocess.CompletedProcess([], 0, output, b''))


class Disk(unittest.TestCase):
    def test_disk_wins_over_the_note(self):
        with tempfile.TemporaryDirectory() as tmp, shown(SHOWN):
            Path(tmp, 'Cargo.lock').write_bytes(b'on disk')
            self.assertEqual(locks.current(tmp, 'Cargo.lock'), b'on disk')

    def test_note_when_nothing_on_disk(self):
        with tempfile.TemporaryDirectory() as tmp, shown(SHOWN):
            self.assertTrue(locks.current(tmp, 'host/Cargo.lock').startswith(b'version'))

    def test_missing_section_is_named_when_the_commit_has_no_blob(self):
        miss = subprocess.CompletedProcess([], 1, b'', b'')
        calls = [subprocess.CompletedProcess([], 0, SHOWN, b''), miss]
        with tempfile.TemporaryDirectory() as tmp, mock.patch('locks.subprocess.run', side_effect=calls):
            with self.assertRaises(locks.NoLock):
                locks.at_commit(tmp, 'abc', 'nope/Cargo.lock')

    def test_a_commit_before_the_notes_gives_its_tracked_blob(self):
        none = subprocess.CompletedProcess([], 1, b'', b'no note')
        calls = [none, none, none,  # house show, git fetch, house show again; then git show.
                  subprocess.CompletedProcess([], 0, b'tracked', b'')]
        with tempfile.TemporaryDirectory() as tmp, mock.patch('locks.subprocess.run', side_effect=calls):
            self.assertEqual(locks.at_commit(tmp, 'abc', 'Cargo.lock'), b'tracked')

    def test_no_note_is_an_error_for_at_commit_and_quiet_for_ensure(self):
        failed = subprocess.CompletedProcess([], 1, b'', b'no note')
        with tempfile.TemporaryDirectory() as tmp, mock.patch('locks.subprocess.run', return_value=failed):
            with self.assertRaises(locks.NoLock):
                locks.at_commit(tmp, 'abc', 'Cargo.lock')
            self.assertEqual(locks.ensure(tmp), [])

    def test_ensure_writes_only_where_absent_and_the_directory_exists(self):
        with tempfile.TemporaryDirectory() as tmp, shown(SHOWN):
            Path(tmp, 'Cargo.lock').write_bytes(b'mine')
            self.assertEqual(locks.ensure(tmp), [])  # host/ is not there; the root lock is.
            Path(tmp, 'host').mkdir()
            self.assertEqual(locks.ensure(tmp), ['host/Cargo.lock'])
            self.assertEqual(Path(tmp, 'Cargo.lock').read_bytes(), b'mine')
            self.assertEqual(Path(tmp, 'host/Cargo.lock').read_bytes(), b'version = 4\n')


if __name__ == '__main__':
    unittest.main()
