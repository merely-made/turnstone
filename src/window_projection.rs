// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native-window host namespaces and one appearance per producer tree.
use accesskit::{NodeId, Rect};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;
use uxtree::{UxTree, node_id_for_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowScope {
    pub slot: usize,
    pub instance: u64,
    pub session: u128,
}

pub(crate) fn namespace(
    tree: &mut UxTree,
    focus: &mut Option<NodeId>,
    scope: WindowScope,
) -> HashMap<NodeId, NodeId> {
    let map: HashMap<_, _> = tree
        .nodes
        .iter()
        .map(|(id, _)| {
            (
                *id,
                node_id_for_path(&format!(
                    "turnstone/window/{}/instance/{}/session/{}/node/{}",
                    scope.slot, scope.instance, scope.session, id.0
                )),
            )
        })
        .collect();
    for (id, node) in &mut tree.nodes {
        *id = map[id];
        remap_node(node, &map);
    }
    tree.root = map[&tree.root];
    *focus = focus.and_then(|id| map.get(&id).copied());
    map
}

pub(crate) fn remap_node(node: &mut accesskit::Node, map: &HashMap<NodeId, NodeId>) {
    macro_rules! lists {($($get:ident,$set:ident);* $(;)?)=>{$(node.$set(node.$get().iter().filter_map(|id|map.get(id).copied()).collect::<Vec<_>>());)*};}
    lists!(children,set_children;controls,set_controls;details,set_details;described_by,set_described_by;flow_to,set_flow_to;labelled_by,set_labelled_by);
    macro_rules! references {($($get:ident,$set:ident,$clear:ident);* $(;)?)=>{$(if let Some(old)=node.$get() {if let Some(new)=map.get(&old) {node.$set(*new);} else {node.$clear();}})*};}
    references!(active_descendant,set_active_descendant,clear_active_descendant;error_message,set_error_message,clear_error_message;in_page_link_target,set_in_page_link_target,clear_in_page_link_target;member_of,set_member_of,clear_member_of;next_on_line,set_next_on_line,clear_next_on_line;previous_on_line,set_previous_on_line,clear_previous_on_line;popup_for,set_popup_for,clear_popup_for);
    if let Some(mut selection) = node.text_selection().cloned() {
        if let (Some(anchor), Some(focus)) = (
            map.get(&selection.anchor.node),
            map.get(&selection.focus.node),
        ) {
            selection.anchor.node = *anchor;
            selection.focus.node = *focus;
            node.set_text_selection(selection);
        } else {
            node.clear_text_selection();
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Appearance {
    pub node: Uuid,
    pub slot: usize,
    pub appearance: u64,
    pub rect: Rect,
}

pub(crate) fn owners(
    appearances: impl IntoIterator<Item = Appearance>,
    focused: Option<(Uuid, u64)>,
) -> BTreeMap<Uuid, Appearance> {
    let mut all: Vec<_> = appearances.into_iter().collect();
    all.sort_by_key(|a| (a.slot, a.appearance));
    let mut owners = BTreeMap::new();
    for a in all {
        if ![a.rect.x0, a.rect.y0, a.rect.x1, a.rect.y1]
            .into_iter()
            .all(f64::is_finite)
            || a.rect.width() <= 0.0
            || a.rect.height() <= 0.0
        {
            continue;
        }
        if focused == Some((a.node, a.appearance)) || !owners.contains_key(&a.node) {
            owners.insert(a.node, a);
        }
    }
    owners
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Node, Role};
    #[test]
    fn window_instance_and_session_namespace_all_local_references() {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2)]);
        root.set_active_descendant(NodeId(2));
        let mut child = Node::new(Role::Button);
        child.set_labelled_by(vec![NodeId(1)]);
        child.set_controls(vec![NodeId(1)]);
        let base = UxTree {
            root: NodeId(1),
            nodes: vec![(NodeId(1), root), (NodeId(2), child)],
        };
        let mut first = UxTree {
            root: base.root,
            nodes: base.nodes.clone(),
        };
        let mut focus = Some(NodeId(2));
        let scope = WindowScope {
            slot: 1,
            instance: 7,
            session: 11,
        };
        let map = namespace(&mut first, &mut focus, scope);
        assert_eq!(first.nodes[0].1.children(), &[map[&NodeId(2)]]);
        assert_eq!(first.nodes[0].1.active_descendant(), focus);
        assert_eq!(first.nodes[1].1.labelled_by(), &[first.root]);
        assert_eq!(first.nodes[1].1.controls(), &[first.root]);
        for other in [
            WindowScope { slot: 2, ..scope },
            WindowScope {
                instance: 8,
                ..scope
            },
            WindowScope {
                session: 12,
                ..scope
            },
        ] {
            let mut tree = UxTree {
                root: base.root,
                nodes: base.nodes.clone(),
            };
            namespace(&mut tree, &mut None, other);
            assert_ne!(tree.root, first.root);
        }
    }
    #[test]
    fn repeated_producer_has_one_owner_and_follows_exact_focused_appearance() {
        let node = Uuid::from_u128(1);
        let rect = Rect::new(5.0, 9.0, 25.0, 29.0);
        let a = Appearance {
            node,
            slot: 0,
            appearance: 20,
            rect,
        };
        let b = Appearance {
            node,
            slot: 1,
            appearance: 10,
            rect,
        };
        let c = Appearance {
            node: Uuid::from_u128(2),
            slot: 2,
            appearance: 1,
            rect,
        };
        let own = owners([b, c, a], None);
        assert_eq!(own.len(), 2);
        assert_eq!(own[&node].slot, 0);
        let own = owners([a, b, c], Some((node, 10)));
        assert_eq!(own[&node].slot, 1);
        assert_eq!(owners([b, a], Some((node, 999)))[&node].slot, 0);
    }
}
