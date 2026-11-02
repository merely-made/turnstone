// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn context_cards_visible(state: &ChromeState) -> bool {
    state.shellbar_visible
        && !state.focus_cards.is_empty()
        && state.menu.is_none()
        && !state.open
        && state.decision.is_none()
}
