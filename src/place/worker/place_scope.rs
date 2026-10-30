// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The Moot-scoped capability a place delegation carries.
pub fn place_scope(moot: [u8; 32]) -> CapabilityScope {
    CapabilityScope {
        domain: MOOT_DELEGATION_DOMAIN.into(),
        resource: moot.to_vec(),
        path_prefix: place_capability_prefix(),
        actions: [MOOT_ACT_ACTION.to_string()].into_iter().collect(),
    }
}
