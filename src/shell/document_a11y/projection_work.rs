// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[inline]
fn record_node_visit() {
    #[cfg(test)]
    NODE_VISITS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
thread_local! {
    static NODE_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_node_visits(f: impl FnOnce()) -> usize {
    NODE_VISITS.with(|count| count.set(0));
    f();
    NODE_VISITS.with(std::cell::Cell::get)
}

pub(super) fn projection_tree(projection: &DocumentA11yProjection) -> Option<TreeUpdate> {
    let mut semantics = std::collections::HashMap::with_capacity(projection.nodes().len());
    for node in projection.nodes() {
        record_node_visit();
        // Preserve DocumentA11yProjection::node's first-match behavior if an
        // unvalidated producer supplies duplicate local IDs.
        semantics.entry(node.id).or_insert(node);
    }
    let mut seen = std::collections::HashSet::new();
    let mut pending = vec![projection.root()];
    let mut nodes = Vec::new();
    let mut multiline = std::collections::HashSet::new();
    let mut focus = NodeId(projection.root().get());
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            return None;
        }
        let semantic = *semantics.get(&id)?;
        if semantic.state.focused && !semantic.state.hidden {
            // Turnstone's traversal chooses the last visible focused node.
            focus = NodeId(id.get());
        }
        if semantic.role == DocumentA11yRole::TextField && semantic.state.multiline {
            multiline.insert(NodeId(id.get()));
        }
        nodes.push(semantic.clone());
        pending.extend(semantic.children.iter().rev().copied());
    }
    // Only validated reachable nodes enter the shared converter, in the same
    // preorder as Turnstone's host tree. Genet owns all semantic field lowering.
    let reachable = DocumentA11yProjection::new(
        projection.revision(),
        projection.support().clone(),
        projection.root(),
        nodes,
    );
    let mut update = genet_render::accesskit_tree_from_projection(reachable);
    for (id, node) in &mut update.nodes {
        if multiline.contains(id) {
            node.set_role(Role::MultilineTextInput);
        }
    }
    update.focus = focus;
    let (_, root) = update
        .nodes
        .iter_mut()
        .find(|(id, _)| id.0 == projection.root().get())?;
    let support = projection.support();
    let description = format!(
        "{:?}: {}",
        support.capability(),
        support.limitations().join(" ")
    );
    root.set_description(match root.description() {
        Some(existing) => format!("{existing}; {description}"),
        None => description,
    });
    Some(update)
}

/// Replace the spawn outline, including its now-unreachable descendants.
pub(super) fn replace_document(
    tree: &mut uxtree::UxTree,
    root: NodeId,
    replacement: Option<uxtree::UxTree>,
) {
    let mut previous = std::collections::HashMap::with_capacity(tree.nodes.len());
    for (id, node) in &tree.nodes {
        record_node_visit();
        previous.entry(*id).or_insert(node);
    }
    let mut pending = vec![root];
    let mut retired = std::collections::HashSet::new();
    while let Some(id) = pending.pop() {
        if retired.insert(id)
            && let Some(node) = previous.get(&id)
        {
            pending.extend(node.children().iter().copied());
        }
    }
    tree.nodes.retain(|(id, _)| !retired.contains(id));
    if let Some(replacement) = replacement {
        // The document root keeps its host identity and position among siblings.
        if !tree
            .nodes
            .iter()
            .any(|(_, node)| node.children().contains(&root))
        {
            if let Some((_, node)) = tree.nodes.iter_mut().find(|(id, _)| *id == tree.root) {
                let mut children = node.children().to_vec();
                children.push(root);
                node.set_children(children);
            }
        }
        tree.nodes.extend(replacement.nodes);
    } else {
        for (_, node) in &mut tree.nodes {
            let children: Vec<_> = node
                .children()
                .iter()
                .copied()
                .filter(|id| !retired.contains(id))
                .collect();
            node.set_children(children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic(id: u64, children: &[u64]) -> inker::DocumentA11yNode {
        inker::DocumentA11yNode {
            id: DocumentA11yNodeId::new(id),
            parent: None,
            children: children
                .iter()
                .copied()
                .map(DocumentA11yNodeId::new)
                .collect(),
            role: DocumentA11yRole::Button,
            name: Some(format!("Control {id}")),
            description: None,
            value: None,
            numeric_value: None,
            numeric_minimum: None,
            numeric_maximum: None,
            bounds: None,
            state: Default::default(),
            actions: vec![DocumentA11yAction::Click],
        }
    }

    fn projection(nodes: Vec<inker::DocumentA11yNode>) -> DocumentA11yProjection {
        DocumentA11yProjection::new(
            1,
            inker::DocumentA11ySupport::new(inker::A11yCapability::Partial, ["work control"])
                .unwrap(),
            DocumentA11yNodeId::new(1),
            nodes,
        )
    }

    #[test]
    fn document_lowering_resolves_all_semantic_ids_with_linear_work() {
        let count = 64;
        let children: Vec<_> = (2..=count).collect();
        let mut nodes = vec![semantic(1, &children)];
        nodes.extend((2..=count).map(|id| semantic(id, &[])));
        let projection = projection(nodes);
        let visits = count_node_visits(|| {
            let tree = projection_tree(&projection).unwrap();
            assert_eq!(tree.nodes.len(), count as usize);
            assert_eq!(tree.nodes.last().unwrap().1.label(), Some("Control 64"));
        });
        assert!(
            visits >= count as usize && visits <= count as usize * 2,
            "{count} semantics used {visits} ID search steps"
        );
    }

    #[test]
    fn document_lowering_preserves_first_duplicate_and_refuses_missing_or_cyclic_nodes() {
        let mut duplicate = semantic(2, &[]);
        duplicate.name = Some("later duplicate".into());
        let malformed = projection(vec![semantic(1, &[2]), semantic(2, &[]), duplicate]);
        let tree = projection_tree(&malformed).unwrap();
        assert_eq!(tree.nodes[1].1.label(), Some("Control 2"));
        assert!(projection_tree(&projection(vec![semantic(1, &[2])])).is_none());
        assert!(projection_tree(&projection(vec![semantic(1, &[2]), semantic(2, &[1])])).is_none());
    }

    #[test]
    fn document_lowering_keeps_host_multiline_focus_and_support_overrides() {
        let mut root = semantic(1, &[2, 3, 4]);
        root.role = DocumentA11yRole::Document;
        root.description = Some("Owner summary".into());
        let mut field = semantic(2, &[]);
        field.role = DocumentA11yRole::TextField;
        field.state.multiline = true;
        field.state.focused = true;
        field.value = Some("first line\nsecond line".into());
        field.actions = vec![DocumentA11yAction::Focus, DocumentA11yAction::SetValue];
        field.bounds = Some(inker::DocumentA11yBounds {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        });
        let mut last_visible = semantic(3, &[]);
        last_visible.state.focused = true;
        let mut hidden = semantic(4, &[]);
        hidden.state.focused = true;
        hidden.state.hidden = true;
        let mut unreachable = semantic(5, &[]);
        unreachable.state.focused = true;
        let mut duplicate = semantic(2, &[]);
        duplicate.name = Some("later duplicate".into());
        // Producer storage order need not be document traversal order.
        let source = projection(vec![
            last_visible,
            root,
            hidden,
            field,
            duplicate,
            unreachable,
        ]);
        let tree = projection_tree(&source).unwrap();
        assert_eq!(
            tree.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![NodeId(1), NodeId(2), NodeId(3), NodeId(4)]
        );
        assert_eq!(tree.focus, NodeId(3), "last visible traversal focus wins");
        assert_eq!(
            tree.nodes[0].1.description(),
            Some("Owner summary; Partial: work control")
        );
        let field = &tree.nodes[1].1;
        assert_eq!(field.role(), Role::MultilineTextInput);
        assert_eq!(field.label(), Some("Control 2"));
        assert_eq!(field.value(), Some("first line\nsecond line"));
        assert_eq!(
            field.bounds(),
            Some(accesskit::Rect::new(10.0, 20.0, 40.0, 60.0))
        );
        assert!(field.supports_action(Action::Focus));
        assert!(field.supports_action(Action::SetValue));
        assert!(tree.nodes[3].1.is_hidden());
    }

    #[test]
    fn document_retirement_resolves_old_descendants_with_linear_work() {
        let count = 64;
        let mut nodes = Vec::new();
        for id in 1..=count {
            let mut node = Node::new(Role::Group);
            if id < count {
                node.set_children(vec![NodeId(id + 1)]);
            }
            nodes.push((NodeId(id), node));
        }
        let mut tree = uxtree::UxTree {
            root: NodeId(1),
            nodes,
        };
        let visits = count_node_visits(|| replace_document(&mut tree, NodeId(2), None));
        assert_eq!(tree.nodes.len(), 1);
        assert!(tree.nodes[0].1.children().is_empty());
        assert!(
            visits >= count as usize && visits <= count as usize * 2,
            "{count} outline nodes used {visits} ID search steps"
        );
    }
}
