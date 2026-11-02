// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn nodes_at(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(node) = todo.pop() {
        out.push(node);
        todo.extend(dom.dom_children(node));
    }
    out
}
