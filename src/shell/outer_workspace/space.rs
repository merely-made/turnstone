// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn space(slot: usize) -> SpaceRef {
    if slot == 0 {
        SpaceRef::Primary
    } else {
        SpaceRef::Lens(slot - 1)
    }
}
