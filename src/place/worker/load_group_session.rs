// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Reopen the sealed group session established by [`prepare_group_identity`].
pub(crate) fn load_group_session(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<GroupSession, String> {
    let storage = sealed_group_store(directory, identity, moot)?;
    let bytes: Vec<u8> = storage
        .load_record(GROUP_SESSION_RECORD)
        .map_err(|error| format!("load sealed group session: {error}"))?
        .ok_or_else(|| "sealed group session is absent".to_string())?;
    GroupSession::from_bytes(&bytes)
        .map_err(|error| format!("decode sealed group session: {error}"))
}
