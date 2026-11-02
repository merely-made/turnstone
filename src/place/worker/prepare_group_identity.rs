// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Establish this profile's durable group identity for one Moot and return the
/// publishable pre-key bundle.
///
/// This must happen before an invitation can be issued, not after one arrives.
/// `GroupSession::new` draws its long-term key from the RNG, so the recipient
/// id is not derivable from the Personae root: the session that generated the
/// published pre-key is the only one a welcome can be addressed to. Creating a
/// fresh session at admission time would produce a different recipient and
/// refuse every genuine welcome.
///
/// Idempotent. A second call returns the same bundle rather than rotating the
/// identity out from under a welcome already in flight.
pub fn prepare_group_identity(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<Vec<u8>, String> {
    let storage = sealed_group_store(directory, identity, moot)?;
    if let Some(bundle) = storage
        .load_record(GROUP_PREKEY_RECORD)
        .map_err(|error| format!("load sealed group pre-key: {error}"))?
    {
        return Ok(bundle);
    }
    let (session, prekey) = GroupSession::new(GroupSessionId(moot), identity)
        .map_err(|error| format!("create group session: {error}"))?;
    let bundle = prekey
        .to_bytes()
        .map_err(|error| format!("encode group pre-key: {error}"))?;
    save_group_session(directory, identity, &session)?;
    storage
        .save_record(GROUP_PREKEY_RECORD, &bundle)
        .map_err(|error| format!("seal group pre-key: {error}"))?;
    Ok(bundle)
}
