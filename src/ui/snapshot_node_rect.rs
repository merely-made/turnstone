// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn snapshot_node_rect<D>(
    dom: &D,
    snapshot: &LiverySnapshot<D::NodeId>,
    node: D::NodeId,
    element_scroll: &HashMap<D::NodeId, (f32, f32)>,
    viewport_scroll: (f32, f32),
) -> Option<(f32, f32, f32, f32)>
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    let fragment = snapshot.fragments.get(node)?;
    let translated = accumulated_translation(dom, snapshot, node);
    let nested = ancestor_scroll(dom, node, element_scroll);
    Some((
        fragment.x + translated.0 - nested.0 - viewport_scroll.0,
        fragment.y + translated.1 - nested.1 - viewport_scroll.1,
        fragment.width,
        fragment.height,
    ))
}
