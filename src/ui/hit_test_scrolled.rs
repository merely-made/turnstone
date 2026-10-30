// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Hit-test a pane at its scroll offset.
///
/// Separate from the unscrolled path because a click must resolve against what
/// the person is *looking at*: hit-testing an unscrolled layout under a
/// scrolled pane activates whatever row happens to share those coordinates at
/// offset zero.
pub fn hit_test_scrolled(
    dom: &ScriptedDom,
    sheet: &str,
    w: u32,
    h: u32,
    x: f32,
    y: f32,
    scroll: &PaneScroll,
) -> Option<DomNodeId> {
    let snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    let offset = clamp_scroll(scroll.offset, snapshot.content_extent, (w, h));
    snapshot_hit_test(dom, &snapshot, x, y, &HashMap::new(), offset, (w, h))
}
