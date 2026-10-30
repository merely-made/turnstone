// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn nodes(dom: &ScriptedDom, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        out.push(node);
        let children = dom.dom_children(node).collect::<Vec<_>>();
        pending.extend(children.into_iter().rev());
    }
    out
}
