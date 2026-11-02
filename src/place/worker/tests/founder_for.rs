// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn founder_for(binding: &PlaceBindingV1) -> InMemoryProvider {
    InMemoryProvider::from_seed([binding.moot.0[0].wrapping_add(40); 32])
}
