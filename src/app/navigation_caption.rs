// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The content controlled by the navigation row can differ from the graph's
/// selection when a Workbench tab owns the browser commands.
pub(crate) fn navigation_caption(app: &App) -> Option<String> {
    let label = app.browser_command_target().and_then(|(member, _)| {
        let graph = app.graph_runtimes.graph_containing_member(member)?;
        let canvas = app.graph_runtimes.canvas(graph)?;
        let graph = canvas.graph();
        let (key, _) = graph.nodes().find(|(_, node)| node.id == member)?;
        Some(graph.node_display_caption(key))
    });
    label.or_else(|| focused_caption(&app.graph_runtimes))
}
