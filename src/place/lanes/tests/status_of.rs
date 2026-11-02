// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn status_of(
    founder: &ServedFounder,
    snapshot: crate::place::OfflinePlaceSnapshot,
) -> Vec<String> {
    crate::place::PlaceState::Offline {
        binding: founder.binding.clone(),
        generation: 1,
        snapshot,
    }
    .status_lines()
}
