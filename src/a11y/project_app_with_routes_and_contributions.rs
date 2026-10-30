// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn project_app_with_routes_and_contributions(
    app: &App,
    mut contributions: std::collections::HashMap<crate::panes::PaneId, UxTree>,
) -> (UxTree, A11yRoutes) {
    let mut orrery_pane = None;
    let tree = project_app_capturing(app, &mut orrery_pane, &mut |pane| {
        contributions.remove(&pane)
    });

    let mut routes = A11yRoutes::new();
    for (path, _, action) in toolbar_actions(app) {
        routes.insert(node_id_for_path(&path), A11yRoute::Action(action));
    }
    routes.insert(
        node_id_for_path("turnstone/chrome/omnibar"),
        A11yRoute::OpenOmnibar,
    );
    for (path, _, route) in omnibar_rows(app) {
        if let Some(route) = route {
            routes.insert(node_id_for_path(&path), route);
        }
    }
    if let Some(pane) = orrery_pane {
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
    (tree, routes)
}
