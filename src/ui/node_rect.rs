// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn node_rect(
    dom: &ScriptedDom,
    node: DomNodeId,
    sheet: &str,
    w: u32,
    h: u32,
) -> Option<(f32, f32, f32, f32)> {
    let snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    snapshot_node_rect(dom, &snapshot, node, &HashMap::new(), (0.0, 0.0))
}
