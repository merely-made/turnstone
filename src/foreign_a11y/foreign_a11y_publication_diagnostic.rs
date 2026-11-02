// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ForeignA11y {
    /// Persist identities and topology by default; page text needs an explicit
    /// public-fixture opt-in. Password nodes and their descendants stay redacted.
    pub(crate) fn publication_diagnostic(&self, public_fixture: bool) -> serde_json::Value {
        let (surface_count, tree_count, node_count) = self.published_counts();
        let surfaces = self.published_surfaces().iter().map(|(surface, published)| {
            let trees = published.trees.values().map(|tree| {
                let protected = protected_nodes(tree);
                let nodes = tree.nodes.iter().map(|(id, node)| {
                    let text_allowed = public_fixture && !protected.contains(id);
                    let actions = ADVERTISED_ACTIONS.iter().filter(|action| node.supports_action(**action))
                        .map(|action| format!("{action:?}")).collect::<Vec<_>>();
                    serde_json::json!({
                        "node_id": id.0, "role": format!("{:?}", node.role()),
                        "children": node.children().iter().map(|id| id.0).collect::<Vec<_>>(),
                        "graft_tree_id": node.tree_id().map(|id| id.0.to_string()),
                        "label": text_allowed.then(|| node.label()).flatten(),
                        "value": text_allowed.then(|| node.value()).flatten(),
                        "text_redacted": !text_allowed,
                        "actions": actions,
                        "bounds": node.bounds().map(|bounds| [bounds.x0, bounds.y0, bounds.x1, bounds.y1]),
                        "transform": node.transform().map(|transform| transform.as_coeffs()),
                    })
                }).collect::<Vec<_>>();
                serde_json::json!({"tree_id": tree.tree_id.0.to_string(),
                    "root_node_id": tree.tree.as_ref().map(|tree| tree.root.0),
                    "focus_node_id": tree.focus.0, "nodes": nodes})
            }).collect::<Vec<_>>();
            let ordered_updates = published.ordered_updates.iter().map(|update| {
                serde_json::json!({"tree_id": update.tree_id.0.to_string(),
                    "node_ids": update.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
                    "focus_node_id": update.focus.0})
            }).collect::<Vec<_>>();
            serde_json::json!({"surface": surface.to_string(), "root_tree_id": published.root.0.to_string(),
                "trees": trees, "ordered_updates": ordered_updates})
        }).collect::<Vec<_>>();
        serde_json::json!({
            "scope": "successful in-process composed foreign frame; OS activation and AT acceptance unqualified",
            "public_fixture_text": public_fixture,
            "surface_count": surface_count, "tree_count": tree_count, "node_count": node_count,
            "surfaces": surfaces,
        })
    }

    pub(crate) fn action_target(&self, request: &ActionRequest) -> Result<Uuid, String> {
        let ActionTarget::Guest(surface) = self.owners.route(request) else {
            return Err("action names a host, unknown or retired foreign tree".into());
        };
        if !self.mounted.contains(surface) || !self.ready(*surface) {
            return Err("action names a hidden or incomplete foreign surface".into());
        }
        let node = self.surfaces[surface]
            .trees
            .get(&request.target_tree)
            .and_then(|tree| tree.nodes.get(&request.target_node))
            .ok_or("action names an absent or retired foreign node")?;
        if !node.supports_action(request.action) {
            return Err("foreign node does not advertise the requested action".into());
        }
        Ok(*surface)
    }
}
