// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn subtree_node_rect(
    dom: &ScriptedDom,
    root: DomNodeId,
    node: DomNodeId,
    sheet: &str,
    w: u32,
    h: u32,
) -> Option<(f32, f32, f32, f32)> {
    let view = SubtreeView::new(dom, root);
    let snapshot = build_livery_snapshot(&view, sheet, w, h, TextSystem::new());
    snapshot_node_rect(&view, &snapshot, node, &HashMap::new(), (0.0, 0.0))
}
