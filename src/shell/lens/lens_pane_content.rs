// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn lens_pane_content(
    app: &crate::app::App,
    ordinal: usize,
    id: PaneId,
) -> Option<PaneContent> {
    app.lenses
        .get(ordinal)
        .and_then(|s| s.as_ref())
        .and_then(|space| {
            space
                .iter_leaves()
                .find(|(pid, _, _)| *pid == id)
                .map(|(_, content, _)| content.clone())
        })
}
