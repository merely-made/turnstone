// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A lens has its own pane tree and window root, without primary chrome or
/// primary graph/document outlines. The shell adds actual visible producers.
pub(crate) fn project_lens_with_contributions(
    app: &App,
    ordinal: usize,
    mut contributions: std::collections::HashMap<crate::panes::PaneId, UxTree>,
) -> Option<(UxTree, A11yRoutes)> {
    let layout = app.lenses.get(ordinal)?.as_ref()?;
    let mut frozen = None;
    let panes = crate::panes::project_frisket_with(layout, |content, pane| match content {
        PaneContent::Workbench => app
            .workbench_for_pane(pane)
            .map(mere::platen::project_tile_layout),
        PaneContent::Orrery => {
            let mut root = Node::new(Role::Group);
            let count = app
                .graph_for_pane(pane)
                .and_then(|graph| app.graph_runtimes.canvas(graph))
                .map(|canvas| canvas.graph().nodes().count())
                .unwrap_or(0);
            root.set_label(format!("graph canvas, {count} nodes"));
            Some(stitch(
                &format!("turnstone/lens/{ordinal}/canvas/{}", pane.0),
                root,
                Vec::new(),
            ))
        },
        PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION => {
            frozen = Some(pane);
            Some(project_frozen_projection(app))
        },
        PaneContent::Gloss(_) | PaneContent::Registered(_) => contributions.remove(&pane),
        _ => None,
    });
    let mut root = Node::new(Role::Window);
    root.set_label(lens_window_label(ordinal));
    let tree = stitch(&format!("turnstone/lens/{ordinal}"), root, vec![panes]);
    let mut routes = A11yRoutes::new();
    if let Some(pane) = frozen {
        for (_, node) in app.graph_runtimes.graph().nodes() {
            routes.insert(
                node_id_for_path(&format!("turnstone/frozen-projection/instance/{}", node.id)),
                A11yRoute::SelectMember {
                    pane,
                    member: node.id,
                },
            );
        }
    }
    Some((tree, routes))
}
