// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn binding(seed: u8) -> PlaceBindingV1 {
    PlaceBindingV1::new(
        PlaceId([seed; 32]),
        SharedContainerId([seed.wrapping_add(1); 32]),
        ChatSpaceId([seed.wrapping_add(2); 32]),
        "hall",
    )
    .unwrap()
}
