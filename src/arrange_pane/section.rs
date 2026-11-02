// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn section(title: &str) -> ArrangeView {
    Box::new(
        el::<_, ArrangeState, ArrangeIntent>("div", title.to_string())
            .attr("class", "list-section-title"),
    )
}
