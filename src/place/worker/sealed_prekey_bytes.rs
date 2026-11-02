// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// This profile's own published pre-key bundle for one Moot.
pub(super) fn sealed_prekey_bytes(
    directory: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<Vec<u8>, String> {
    sealed_group_store(directory, identity, moot)?
        .load_record(GROUP_PREKEY_RECORD)
        .map_err(|error| format!("load sealed group pre-key: {error}"))?
        .ok_or_else(|| "this profile has no prepared group identity".to_string())
}
