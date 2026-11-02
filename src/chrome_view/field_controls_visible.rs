// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn field_controls_visible(state: &ChromeState) -> bool {
    !state.open && state.decision.is_none() && state.menu.is_none() && state.shellbar_visible
}
