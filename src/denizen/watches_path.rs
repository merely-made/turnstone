// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Where a participant's signed delegation certificates persist:
/// `sessions/<id>/denizens/<subject>.certs.json`. The certificate is a signed
/// blob, so it lives beside the world rather than inside the browsable graph;
/// the grant projection stays the human-readable audit record of the same
/// grant.
/// Where a session's standing subscriptions live, beside the bindings and
/// certificates.
///
/// One file rather than one per subject, unlike certificates: a watch table is
/// read and written whole (its cursors advance together at each drain), so
/// splitting it would mean a directory scan to answer one question.
pub fn watches_path(session_dir: &Path) -> PathBuf {
    session_dir.join("denizens").join("watches.txt")
}
