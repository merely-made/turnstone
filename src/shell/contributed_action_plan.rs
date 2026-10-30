// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Resolve delivery against the queued native window's current host plan.
/// A pane moving to another window cannot revive its old publication there.
pub(super) fn contributed_action_plan(
    app: &App,
    contents: &NodeDocuments,
    windows: &ContributedActionWindows,
    origin: Option<crate::action::SpaceRef>,
) -> Vec<crate::surface::Surface> {
    match origin {
        Some(crate::action::SpaceRef::Primary) => {
            primary_content_plan(app, contents, windows.primary.0, windows.primary.1)
        },
        Some(crate::action::SpaceRef::Lens(ordinal)) => windows
            .lenses
            .iter()
            .find(|(current, _, _)| *current == ordinal)
            .map(|(_, width, height)| {
                lens::lens_content_plan(app, contents, ordinal, *width, *height)
            })
            .unwrap_or_default(),
        None => Vec::new(),
    }
}
