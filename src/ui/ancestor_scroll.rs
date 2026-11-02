// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn ancestor_scroll<D>(
    dom: &D,
    node: D::NodeId,
    scroll: &HashMap<D::NodeId, (f32, f32)>,
) -> (f32, f32)
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    let mut total = (0.0, 0.0);
    let mut current = dom.parent(node);
    while let Some(parent) = current {
        if let Some(offset) = scroll.get(&parent) {
            total.0 += offset.0;
            total.1 += offset.1;
        }
        current = dom.parent(parent);
    }
    total
}
