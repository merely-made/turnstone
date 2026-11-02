// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn transport_salt(moot: [u8; 32]) -> Vec<u8> {
    let mut salt = Vec::with_capacity(61);
    salt.extend_from_slice(b"turnstone.place.transport.v1/");
    salt.extend_from_slice(&moot);
    salt
}
