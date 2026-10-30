// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn hit_test(
    dom: &ScriptedDom,
    sheet: &str,
    w: u32,
    h: u32,
    x: f32,
    y: f32,
) -> Option<DomNodeId> {
    let snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    snapshot_hit_test(dom, &snapshot, x, y, &HashMap::new(), (0.0, 0.0), (w, h))
}
