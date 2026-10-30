// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn sealed_group_store(
    session_dir: &Path,
    identity: &dyn IdentityProvider,
    moot: [u8; 32],
) -> Result<SealedRecordStorage, String> {
    let key = identity
        .derive_keypair(&place_secret_salt(moot))
        .map_err(|error| format!("derive place secret key: {error}"))?
        .to_seed();
    Ok(SealedRecordStorage::open_with_key(
        place_secrets_dir(session_dir),
        key,
    ))
}
