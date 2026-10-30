// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub fn is_knot_address(address: &str) -> bool {
    address.split(['?', '#']).next().is_some_and(|base| {
        let base = base.to_ascii_lowercase();
        base.ends_with(".djot") || base.ends_with(".knot")
    }) || address.to_ascii_lowercase().starts_with("knot:")
}
