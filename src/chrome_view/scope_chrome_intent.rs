// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One window's chrome view: the caption chip, plus the omnibar card on the
/// primary while open. Positioned by transform-translate (the property the
/// canvas gnode pool proves the retained engine honors on absolutes).
pub(super) fn scope_chrome_intent(
    state: &mut ChromeState,
    command: crate::scope_navigation::ScopeCommand,
) -> ChromeIntent {
    if let crate::scope_navigation::ScopeCommand::Hint(hint) = command {
        state.scope_controls.hint = hint;
        ChromeIntent::MenuChanged
    } else {
        ChromeIntent::Scope(command)
    }
}
