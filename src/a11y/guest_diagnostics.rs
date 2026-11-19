// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's product policy for foreign diagnostic receipts.
use cambium_winit_a11y::guest::ForeignA11y;
use uxtree::lint::ADVERTISED_ACTIONS;

use super::protected_nodes::protected_nodes;

/// Persist identities and topology by default; page text needs an explicit
/// public-fixture opt-in. Password nodes and their descendants stay redacted.
pub(crate) fn publication_diagnostic(
    forest: &ForeignA11y,
    public_fixture: bool,
) -> serde_json::Value {
    let (surface_count, tree_count, node_count) = forest.published_counts();
    let surfaces = forest.published_surfaces().iter().map(|(surface, published)| {
        let trees = published.trees().values().map(|tree| {
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
        let ordered_updates = published.ordered_updates().iter().map(|update| {
            serde_json::json!({"tree_id": update.tree_id.0.to_string(),
                "node_ids": update.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
                "focus_node_id": update.focus.0})
        }).collect::<Vec<_>>();
        serde_json::json!({"surface": surface.to_string(), "root_tree_id": published.root().0.to_string(),
            "trees": trees, "ordered_updates": ordered_updates})
    }).collect::<Vec<_>>();
    serde_json::json!({
        "scope": "successful in-process composed foreign frame; OS activation and AT acceptance unqualified",
        "public_fixture_text": public_fixture,
        "surface_count": surface_count, "tree_count": tree_count, "node_count": node_count,
        "surfaces": surfaces,
    })
}

/// Actual labels/values from the successful composed frame. The caller
/// must opt into known public fixture content before observing these.
pub(crate) fn published_text(forest: &ForeignA11y) -> String {
    forest
        .published_surfaces()
        .iter()
        .flat_map(|(_, surface)| surface.trees().values())
        .flat_map(|tree| {
            let protected = protected_nodes(tree);
            tree.nodes
                .iter()
                .filter(move |(id, _)| !protected.contains(id))
                .flat_map(|(_, node)| [node.label(), node.value()])
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate, Uuid};

    fn surface(id: u128) -> Uuid {
        Uuid::from_u128(id)
    }

    fn tree(id: u64) -> TreeId {
        TreeId(Uuid::from_u64_pair(0, id))
    }

    #[test]
    fn fixture_text_optin_never_exposes_password_nodes_or_descendants() {
        let mut foreign = ForeignA11y::new();
        foreign.admit(surface(1), tree(501)).unwrap();
        let mut root = Node::new(Role::Document);
        root.set_label("public fixture marker");
        root.set_children(vec![NodeId(1)]);
        let mut password = Node::new(Role::PasswordInput);
        password.set_label("secret label");
        password.set_value("secret value");
        password.add_action(Action::SetValue);
        password.set_children(vec![NodeId(2)]);
        let mut text = Node::new(Role::TextRun);
        text.set_value("secret descendant");
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(0), root), (NodeId(1), password), (NodeId(2), text)],
                    tree: Some(TreeInfo::new(NodeId(0))),
                    tree_id: tree(501),
                    focus: NodeId(0),
                },
            )
            .unwrap();
        let id = NodeId(10);
        let mut host = Node::new(Role::Window);
        host.set_children(vec![id]);
        let graft = foreign
            .graft(surface(1), Rect::new(0.0, 0.0, 50.0, 50.0))
            .unwrap();
        foreign
            .frame(
                TreeUpdate {
                    nodes: vec![(NodeId(0), host), (id, graft)],
                    tree_id: TreeId::ROOT,
                    tree: Some(TreeInfo::new(NodeId(0))),
                    focus: id,
                },
                &[surface(1)],
            )
            .unwrap();
        let redacted = publication_diagnostic(&foreign, false).to_string();
        assert!(!redacted.contains("public fixture marker"));
        assert!(!redacted.contains("secret"));
        let public = publication_diagnostic(&foreign, true).to_string();
        assert!(public.contains("public fixture marker"));
        assert!(public.contains("SetValue"));
        assert!(!public.contains("secret"));
        assert_eq!(published_text(&foreign), "public fixture marker");
        assert_eq!(foreign.published_counts(), (1, 1, 3));
    }
}
