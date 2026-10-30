// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// How much horizontal room one suggestion row's TEXT actually has, in px at
/// zoom 1: the card's fixed width less `.omni`'s 8px padding and the row's
/// own 8px padding, both twice. This conservative width guard is checkable
/// before a card exists; the height budget measures the actual card below.
pub(crate) const ROW_TEXT_BUDGET: f32 = CARD_W - 16.0 - 16.0;
