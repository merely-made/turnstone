// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The fixtures' constitution: the product rules at a pinned window, so
/// an authority verdict stays reproducible. Same one grant, same id.
pub(crate) fn place_rules(founder_id: [u8; 32], moot: [u8; 32]) -> ConstitutionRules {
    super::super::place_rules(founder_id, founder_id, moot, 10, Some(1_000))
}
