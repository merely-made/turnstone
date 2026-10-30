// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn accumulated_translation<D>(
    dom: &D,
    snapshot: &LiverySnapshot<D::NodeId>,
    target: D::NodeId,
) -> (f32, f32)
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    fn find<D>(
        dom: &D,
        snapshot: &LiverySnapshot<D::NodeId>,
        node: D::NodeId,
        target: D::NodeId,
        inherited: (f32, f32),
    ) -> Option<(f32, f32)>
    where
        D: LayoutDom,
        D::NodeId: Copy + Eq + Hash,
    {
        #[cfg(test)]
        super::projection_work_tests::geometry_visit();
        let own = node_transform_translation(snapshot, node);
        let translated = (inherited.0 + own.0, inherited.1 + own.1);
        if node == target {
            return Some(translated);
        }
        for child in dom.dom_children(node) {
            if let Some(found) = find(dom, snapshot, child, target, translated) {
                return Some(found);
            }
        }
        None
    }
    find(dom, snapshot, dom.document(), target, (0.0, 0.0)).unwrap_or_default()
}
