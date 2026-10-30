// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Resolve only the focused view and test membership in the saved-reader set.
pub(super) fn focused_saved_reader_node_for(
    app: &crate::app::App,
    is_reader: impl FnOnce(uuid::Uuid) -> bool,
) -> Option<uuid::Uuid> {
    let focused = match app.focus {
        crate::surface::FocusTarget::Content { node, .. } => Some(node),
        crate::surface::FocusTarget::Graph(pane) => app.graph_pane_focused_member(pane),
        crate::surface::FocusTarget::Pane(pane)
            if app.pane_content(pane) == Some(&PaneContent::Workbench) =>
        {
            app.pane_context
                .resolve_context(
                    pane,
                    &crate::panes::ContextBinding::Own,
                    crate::panes::SourceSelector::Graph,
                )
                .and_then(|context| context.member)
        },
        _ => None,
    }?;
    is_reader(focused).then_some(focused)
}
