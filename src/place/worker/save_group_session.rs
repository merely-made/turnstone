// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Persist the group session inside the same Personae-derived sealed boundary
/// the worker reopens. [`admit_invitation`] calls this after every check.
pub fn save_group_session(
    session_dir: &Path,
    identity: &dyn IdentityProvider,
    session: &GroupSession,
) -> Result<(), String> {
    let storage = sealed_group_store(session_dir, identity, session.group().0)?;
    let bytes = session
        .to_bytes()
        .map_err(|error| format!("encode group session: {error}"))?;
    storage
        .save_record(GROUP_SESSION_RECORD, &bytes)
        .map_err(|error| format!("seal group session: {error}"))
}
