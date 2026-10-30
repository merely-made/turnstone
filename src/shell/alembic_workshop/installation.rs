// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn installation(owner: &Owner) -> [u8; 16] {
    let digest = blake3::hash(format!("turnstone-alembic-install-v1/{owner:?}").as_bytes());
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id
}
