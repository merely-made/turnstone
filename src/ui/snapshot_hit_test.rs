// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn snapshot_hit_test<D>(
    dom: &D,
    snapshot: &LiverySnapshot<D::NodeId>,
    x: f32,
    y: f32,
    element_scroll: &HashMap<D::NodeId, (f32, f32)>,
    viewport_scroll: (f32, f32),
    viewport: (u32, u32),
) -> Option<D::NodeId>
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    let mut hit = None;
    walk_dom(dom, dom.document(), &mut |node| {
        if dom.kind(node) != NodeKind::Element {
            return;
        }
        if snapshot
            .styles
            .computed_style(node, "pointer-events")
            .is_some_and(|value| value == "none")
        {
            return;
        }
        let Some((left, top, width, height)) = snapshot_visible_rect(
            dom,
            snapshot,
            node,
            element_scroll,
            viewport_scroll,
            viewport,
        ) else {
            return;
        };
        if x >= left && x <= left + width && y >= top && y <= top + height {
            // DOM paint order is sufficient for Turnstone's translate-only
            // pane/chrome surfaces; a deeper/later element replaces its host.
            hit = Some(node);
        }
    });
    hit
}
