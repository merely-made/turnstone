// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Salt for the key that signs this place's constitutional genesis.
///
/// A derived key, not the master: `IdentityProvider` never surrenders the
/// master seed, and Gemot's genesis wants a signing seed. The constitution's
/// checkpoint signer is therefore this profile's place-founding key, while
/// membership and every capability grant still name the Personae root.
pub(super) fn founding_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(61);
    salt.extend_from_slice(b"turnstone.place.founding.v1/");
    salt.extend_from_slice(&moot);
    salt
}
