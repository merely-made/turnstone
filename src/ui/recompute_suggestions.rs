// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Recompute the suggestion list for the current text against graph truth.
/// Find first: node matches ranked by last-visited recency; then the go row
/// for address-shaped input; hints otherwise.
pub fn recompute_suggestions(
    state: &mut OmnibarState,
    canvas: &Canvas,
    actions: &[(String, crate::action::Action)],
) {
    recompute_suggestions_with_limit(state, canvas, actions, &[], MAX_NODE_MATCHES + 1);
}
