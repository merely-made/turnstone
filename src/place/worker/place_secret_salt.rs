// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn place_secret_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(59);
    salt.extend_from_slice(b"turnstone.place.secrets.v1/");
    salt.extend_from_slice(&moot);
    salt
}
