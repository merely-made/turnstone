// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn both_permissions(effective: bool) -> Option<crate::place::PlacePermissionSnapshot> {
    Some(crate::place::PlacePermissionSnapshot {
        message_write: effective,
        graph_write: effective,
    })
}
