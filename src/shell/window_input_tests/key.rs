// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn key(n: u128) -> ContentKey {
    ContentKey::Node(uuid::Uuid::from_u128(n))
}
