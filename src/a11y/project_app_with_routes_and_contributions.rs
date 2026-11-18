// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn project_app_with_routes_and_contributions(
    app: &App,
    mut contributions: std::collections::HashMap<crate::panes::PaneId, UxTree>,
) -> (UxTree, ActionEnvelopes) {
    let mut orrery_pane = None;
    let tree = project_app_capturing(app, &mut orrery_pane, &mut |pane| {
        contributions.remove(&pane)
    });

    let mut routes = ActionEnvelopes::new();
    for (path, _, action) in toolbar_actions(app) {
        let id = node_id_for_path(&path);
        routes.insert(
            id,
            action_envelope(tree.root, id, A11yRoute::Action(action)),
        );
    }
    let id = node_id_for_path("turnstone/chrome/omnibar");
    routes.insert(id, action_envelope(tree.root, id, A11yRoute::OpenOmnibar));
    for (path, _, route) in omnibar_rows(app) {
        if let Some(route) = route {
            let id = node_id_for_path(&path);
            routes.insert(id, action_envelope(tree.root, id, route));
        }
    }
    if let Some(pane) = orrery_pane {
        for (_, node) in app.graph_runtimes.graph().nodes() {
            let id = node_id_for_path(&format!("turnstone/frozen-projection/instance/{}", node.id));
            let route = A11yRoute::SelectMember {
                pane,
                member: node.id,
            };
            routes.insert(id, action_envelope(tree.root, id, route));
        }
    }
    (tree, routes)
}
