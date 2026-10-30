// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn protected_nodes(tree: &TreeUpdate) -> HashSet<NodeId> {
    let nodes: HashMap<_, _> = tree.nodes.iter().map(|(id, node)| (*id, node)).collect();
    let mut pending: Vec<_> = tree
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::PasswordInput)
        .map(|(id, _)| *id)
        .collect();
    let mut protected = HashSet::new();
    while let Some(id) = pending.pop() {
        if protected.insert(id) {
            if let Some(node) = nodes.get(&id) {
                pending.extend(node.children().iter().copied());
            }
        }
    }
    protected
}
