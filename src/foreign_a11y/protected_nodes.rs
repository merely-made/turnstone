// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn protected_nodes(tree: &TreeUpdate) -> HashSet<NodeId> {
    protected_addresses(std::slice::from_ref(tree))
        .into_iter()
        .map(|(_, id)| id)
        .collect()
}

/// Turnstone's password policy protects the node and all of its descendants.
/// Evaluate the policy using structure alone, including children across grafts.
pub(crate) fn protected_addresses(forest: &[TreeUpdate]) -> HashSet<TreeAddress> {
    let roots: HashMap<_, _> = forest
        .iter()
        .filter_map(|update| Some((update.tree_id, update.tree.as_ref()?.root)))
        .collect();
    let nodes: HashMap<_, _> = forest
        .iter()
        .flat_map(|tree| {
            tree.nodes
                .iter()
                .map(|(id, node)| ((tree.tree_id, *id), node))
        })
        .collect();
    let mut pending: Vec<_> = nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::PasswordInput)
        .map(|(address, _)| *address)
        .collect();
    let mut protected = HashSet::new();
    while let Some(address @ (tree, _)) = pending.pop() {
        if protected.insert(address)
            && let Some(node) = nodes.get(&address)
        {
            pending.extend(node.children().iter().map(|id| (tree, *id)));
            if let Some(graft) = node.tree_id()
                && let Some(&root) = roots.get(&graft)
            {
                pending.push((graft, root));
            }
        }
    }
    protected
}
