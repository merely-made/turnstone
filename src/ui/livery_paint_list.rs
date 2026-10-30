// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn livery_paint_list(dom: &ScriptedDom, sheet: &str, w: u32, h: u32) -> LiveryPaintList {
    let mut snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    paint_snapshot(dom, &mut snapshot, w, h, &HashMap::new(), (0.0, 0.0))
}
