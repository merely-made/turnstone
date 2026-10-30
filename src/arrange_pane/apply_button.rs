// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A button whose press turns the pane's state into an intent.
pub(super) fn apply_button(
    label: &'static str,
    intent: impl Fn(&mut ArrangeState) -> Option<ArrangeIntent> + 'static,
) -> ArrangeView {
    Box::new(
        button(label, move |state: &mut ArrangeState, _: PointerClick| {
            intent(state)
        })
        .attr("class", "setting-apply"),
    )
}
