//! AccessKit composition for product-contributed retained pane surfaces.
//!
//! Genet owns DOM semantics and Livery geometry. This module performs the two
//! host duties that cannot live there: namespace each independent DOM inside
//! Turnstone's one platform tree, and translate pane-local bounds into window
//! coordinates.

use std::collections::HashMap;

use accesskit::{NodeId, Role, TreeUpdate};
use genet_scripted_dom::NodeId as DomNodeId;
use uxtree::{UxTree, node_id_for_path};

use crate::contributed_surface::ContributedSurfaceSessions;
use crate::panes::PaneId;
use crate::surface::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContributedA11yRoute {
    pub pane: PaneId,
    pub node: DomNodeId,
}

pub(crate) struct ContributedA11yProjection {
    pub trees: HashMap<PaneId, UxTree>,
    pub routes: HashMap<NodeId, ContributedA11yRoute>,
    pub focus: Option<NodeId>,
}

/// Project every laid-out contributed session that still has a visible pane.
/// A session has no accessibility tree before its first retained layout; that
/// is an honest absence rather than a second speculative layout.
pub(crate) fn project(
    sessions: &ContributedSurfaceSessions,
    pane_rects: &HashMap<PaneId, Rect>,
    focused_pane: Option<PaneId>,
) -> ContributedA11yProjection {
    let mut trees = HashMap::new();
    let mut routes = HashMap::new();
    let mut focus = None;
    let mut panes: Vec<_> = sessions.iter().collect();
    panes.sort_by_key(|(pane, _)| pane.0);

    for (pane, session) in panes {
        let Some(rect) = pane_rects.get(&pane).copied() else {
            continue;
        };
        let Some((update, action_map)) = session.accessibility_tree() else {
            continue;
        };
        let has_dom_focus = session.accessibility_focus().is_some();
        let label = session.descriptor().label.as_str();
        let (tree, pane_routes, pane_focus) = namespace_tree(
            pane,
            label,
            rect,
            update,
            action_map,
            focused_pane == Some(pane) && has_dom_focus,
        );
        trees.insert(pane, tree);
        routes.extend(pane_routes);
        focus = focus.or(pane_focus);
    }

    ContributedA11yProjection {
        trees,
        routes,
        focus,
    }
}

fn namespace_tree(
    pane: PaneId,
    label: &str,
    rect: Rect,
    update: TreeUpdate,
    action_map: HashMap<NodeId, DomNodeId>,
    carries_focus: bool,
) -> (
    UxTree,
    HashMap<NodeId, ContributedA11yRoute>,
    Option<NodeId>,
) {
    let local_root = update
        .tree
        .as_ref()
        .expect("a full retained DOM projection carries its root")
        .root;
    let remap: HashMap<_, _> = update
        .nodes
        .iter()
        .map(|(local, _)| {
            (
                *local,
                node_id_for_path(&format!("turnstone/contributed/{}/dom/{local:?}", pane.0)),
            )
        })
        .collect();

    let mut routes = HashMap::new();
    let mut nodes = Vec::with_capacity(update.nodes.len());
    for (local, mut node) in update.nodes {
        let global = remap[&local];
        node.set_children(
            node.children()
                .iter()
                .filter_map(|child| remap.get(child).copied())
                .collect::<Vec<_>>(),
        );
        if let Some(bounds) = node.bounds() {
            node.set_bounds(accesskit::Rect::new(
                bounds.x0 + rect.x as f64,
                bounds.y0 + rect.y as f64,
                bounds.x1 + rect.x as f64,
                bounds.y1 + rect.y as f64,
            ));
        }
        // ScriptedDom's document node represents a standalone host window.
        // Inside Turnstone it is a pane subtree, so it must not claim a second
        // native window. Its semantic document child remains untouched.
        if local == local_root {
            node.set_role(Role::Group);
            node.set_label(label);
            node.set_bounds(accesskit::Rect::new(
                rect.x as f64,
                rect.y as f64,
                (rect.x + rect.w) as f64,
                (rect.y + rect.h) as f64,
            ));
        }
        if let Some(dom_node) = action_map.get(&local).copied() {
            routes.insert(
                global,
                ContributedA11yRoute {
                    pane,
                    node: dom_node,
                },
            );
        }
        nodes.push((global, node));
    }

    let root = remap[&local_root];
    let focus = carries_focus
        .then(|| remap.get(&update.focus).copied())
        .flatten();
    (UxTree { root, nodes }, routes, focus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Node, Tree, TreeId};

    fn local_tree() -> (TreeUpdate, HashMap<NodeId, DomNodeId>) {
        let root = NodeId(1);
        let child = NodeId(2);
        let mut button = Node::new(Role::Button);
        button.set_label("Save");
        button.set_bounds(accesskit::Rect::new(5.0, 7.0, 25.0, 27.0));
        button.add_action(Action::Click);
        button.add_action(Action::Focus);
        let mut group = Node::new(Role::Window);
        group.set_children(vec![child]);
        (
            TreeUpdate {
                nodes: vec![(child, button), (root, group)],
                tree: Some(Tree::new(root)),
                tree_id: TreeId::ROOT,
                focus: child,
            },
            HashMap::from([(child, DomNodeId::from_raw(2))]),
        )
    }

    #[test]
    fn namespacing_places_bounds_and_preserves_distinct_actions() {
        let (tree, routes, focus) = namespace_tree(
            PaneId(9),
            "Knot document",
            Rect::new(100.0, 40.0, 300.0, 200.0),
            local_tree().0,
            local_tree().1,
            true,
        );
        let focus = focus.expect("the focused pane carries DOM focus");
        let (_, button) = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == focus)
            .expect("focused button");
        assert_eq!(
            button.bounds(),
            Some(accesskit::Rect::new(105.0, 47.0, 125.0, 67.0))
        );
        assert!(button.supports_action(Action::Click));
        assert!(button.supports_action(Action::Focus));
        assert!(routes.contains_key(&focus));
    }

    #[test]
    fn equal_dom_ids_in_two_panes_never_collide() {
        let (left, _, _) = namespace_tree(
            PaneId(1),
            "left",
            Rect::new(0.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        let (right, _, _) = namespace_tree(
            PaneId(2),
            "right",
            Rect::new(100.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        assert!(
            left.nodes
                .iter()
                .all(|(left, _)| { right.nodes.iter().all(|(right, _)| left != right) })
        );
    }

    #[test]
    fn an_unfocused_dom_does_not_invent_focus_and_its_root_uses_the_pane_rect() {
        let (tree, _, focus) = namespace_tree(
            PaneId(3),
            "Knot document",
            Rect::new(30.0, 50.0, 400.0, 250.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        assert_eq!(focus, None);
        let (_, root) = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == tree.root)
            .expect("namespaced root");
        assert_eq!(
            root.bounds(),
            Some(accesskit::Rect::new(30.0, 50.0, 430.0, 300.0))
        );
    }
}
