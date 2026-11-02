// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Traverse the exact host/guest forest supplied to the adapter. Local IDs
/// remain local to their tree, and sibling order comes from children.
pub(crate) fn forest_lines(forest: &[accesskit::TreeUpdate]) -> Vec<String> {
    use accesskit::TreeId;
    let trees: std::collections::HashMap<_, _> = forest.iter().map(|t| (t.tree_id, t)).collect();
    let Some(root) = trees
        .get(&TreeId::ROOT)
        .and_then(|t| t.tree.as_ref())
        .map(|t| t.root)
    else {
        return Vec::new();
    };
    let mut pending = vec![(TreeId::ROOT, root)];
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    while let Some((tree, id)) = pending.pop() {
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
        let role = format!("{:?}", node.role()).to_lowercase();
        lines.push(match (node.label(), node.value()) {
            (Some(label), Some(value)) => format!("{role}: {label} = {value}"),
            (Some(label), None) => format!("{role}: {label}"),
            (None, Some(value)) => format!("{role} = {value}"),
            (None, None) => role,
        });
        if let Some(guest) = node.tree_id() {
            if let Some(root) = trees
                .get(&guest)
                .and_then(|t| t.tree.as_ref())
                .map(|t| t.root)
            {
                pending.push((guest, root));
            }
        } else {
            pending.extend(node.children().iter().rev().map(|id| (tree, *id)));
        }
    }
    lines
}
