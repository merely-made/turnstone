// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The Frozen Projection pane's subtree: the same AccessKit tree
/// `graphshell_client::frozen` builds for any host, over the same scene the
/// endpoint discloses. Rebuilt from app truth here rather than read out of the
/// renderer, because the a11y projection works off `&App` and the frozen form
/// is a pure reading of the graph.
pub(super) fn project_frozen_projection(app: &App) -> UxTree {
    let graph = app.graph_runtimes.graph();
    let scene = crate::remote_projection::disclose_scene(
        graph,
        app.graph_runtimes.focused_key(),
        (248.0, 168.0),
        sceno::Spiral::default(),
    );
    let names = graph
        .nodes()
        .map(|(key, node)| {
            (
                sceno::SourceRef::new(cartography::MERE_GRAPH_ADAPTER, node.id.to_string()),
                graph.node_display_label(key),
            )
        })
        .collect();
    graphshell_client::frozen::FrozenScene::freeze(&scene, "Disclosed projection", &names)
        .to_ux_tree("turnstone/frozen-projection")
}
