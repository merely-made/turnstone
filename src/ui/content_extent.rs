// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_extent<D>(dom: &D, fragments: &LiveryLayout<D::NodeId>) -> (f32, f32)
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    let mut extent = (0.0_f32, 0.0_f32);
    walk_dom(dom, dom.document(), &mut |node| {
        if let Some(fragment) = fragments.get(node) {
            extent.0 = extent.0.max(fragment.x + fragment.width);
            extent.1 = extent.1.max(fragment.y + fragment.height);
        }
    });
    extent
}
