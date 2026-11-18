// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Traverse the exact host/guest forest supplied to the adapter. Local IDs
/// remain local to their tree, and sibling order comes from children.
pub(crate) fn forest_lines(forest: &[accesskit::TreeUpdate]) -> Vec<String> {
    forest_nodes(forest)
        .into_iter()
        .map(|observation| {
            let node = observation.node;
            let role = format!("{:?}", node.role()).to_lowercase();
            match (node.label(), node.value()) {
                (Some(label), Some(value)) => format!("{role}: {label} = {value}"),
                (Some(label), None) => format!("{role}: {label}"),
                (None, Some(value)) => format!("{role} = {value}"),
                (None, None) => role,
            }
        })
        .collect()
}

/// A node in the last complete publication, positioned in window space.
pub(crate) struct ObservedNode<'a> {
    pub tree: accesskit::TreeId,
    pub node: &'a accesskit::Node,
    pub bounds: Option<accesskit::Rect>,
}

pub(crate) fn forest_nodes(forest: &[accesskit::TreeUpdate]) -> Vec<ObservedNode<'_>> {
    use accesskit::{Affine, TreeId};
    let trees: std::collections::HashMap<_, _> = forest.iter().map(|t| (t.tree_id, t)).collect();
    let Some(root) = trees
        .get(&TreeId::ROOT)
        .and_then(|t| t.tree.as_ref())
        .map(|t| t.root)
    else {
        return Vec::new();
    };
    let mut pending = vec![(
        TreeId::ROOT,
        root,
        Affine::IDENTITY,
        None::<accesskit::Rect>,
    )];
    let mut seen = std::collections::HashSet::new();
    let mut nodes = Vec::new();
    while let Some((tree, id, parent_transform, parent_clip)) = pending.pop() {
        if !seen.insert((tree, id)) {
            continue;
        }
        let Some(node) = trees
            .get(&tree)
            .and_then(|t| t.nodes.iter().find(|(n, _)| *n == id))
            .map(|(_, n)| n)
        else {
            continue;
        };
        if node.is_hidden() {
            continue;
        }
        let transform = parent_transform * node.transform().copied().unwrap_or(Affine::IDENTITY);
        let bounds = node.bounds().map(|bounds| {
            let bounds = transform.transform_rect_bbox(bounds);
            parent_clip.map_or(bounds, |clip| bounds.intersect(clip))
        });
        nodes.push(ObservedNode { tree, node, bounds });
        let clip = if node.clips_children() || node.tree_id().is_some() {
            bounds.or(parent_clip)
        } else {
            parent_clip
        };
        if let Some(guest) = node.tree_id() {
            if let Some(root) = trees
                .get(&guest)
                .and_then(|t| t.tree.as_ref())
                .map(|t| t.root)
            {
                pending.push((guest, root, transform, clip));
            }
        } else {
            pending.extend(
                node.children()
                    .iter()
                    .rev()
                    .map(|id| (tree, *id, transform, clip)),
            );
        }
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Affine, Node, NodeId, Rect, Role, Tree, TreeId, TreeUpdate};

    #[test]
    fn published_guest_control_uses_nested_transforms_and_graft_clip() {
        let guest = uxtree::graft::tree_id_for_path("scenario/guest");
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(2)]);
        let graft = uxtree::graft::graft_node(Role::Group, guest, (200.0, 100.0), (40.0, 50.0));

        let mut button = Node::new(Role::Button);
        button.set_label("Apply");
        button.set_transform(Affine::translate((5.0, 10.0)));
        button.set_bounds(Rect::new(10.0, 0.0, 50.0, 20.0));
        let forest = [
            TreeUpdate {
                tree_id: TreeId::ROOT,
                tree: Some(Tree::new(NodeId(1))),
                nodes: vec![(NodeId(1), root), (NodeId(2), graft)],
                focus: NodeId(1),
            },
            TreeUpdate {
                tree_id: guest,
                tree: Some(Tree::new(NodeId(1))),
                nodes: vec![(NodeId(1), button)],
                focus: NodeId(1),
            },
        ];

        let nodes = forest_nodes(&forest);
        let control = nodes
            .iter()
            .find(|node| node.node.label() == Some("Apply"))
            .unwrap();
        assert_eq!(control.tree, guest);
        assert_eq!(control.bounds, Some(Rect::new(215.0, 110.0, 240.0, 130.0)));
    }
}
