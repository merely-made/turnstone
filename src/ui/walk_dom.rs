// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn walk_dom<D>(dom: &D, node: D::NodeId, visit: &mut impl FnMut(D::NodeId))
where
    D: LayoutDom,
    D::NodeId: Copy,
{
    visit(node);
    for child in dom.dom_children(node) {
        walk_dom(dom, child, visit);
    }
}
