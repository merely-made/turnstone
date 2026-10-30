// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Parse "`<x> <y>`" into a pair of f32 window coordinates.
pub(super) fn parse_xy(s: &str) -> Option<(f32, f32)> {
    let mut it = s.split_whitespace();
    let x = it.next()?.parse().ok()?;
    let y = it.next()?.parse().ok()?;
    Some((x, y))
}
