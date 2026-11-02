// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Lay out a `ScriptedDom` under `sheet` and composite its paint list into a
/// scene — the retained Livery/Buckram path the chrome runs, generalized over the sheet so
/// a cambium-built DOM (rung 5 slice D toolkit adoption) renders the same way,
/// under its own class stylesheet. Text-only DOM; a view with custom-paint
/// leaves goes through [`scene_from_dom_with_leaves`].
///
/// **One-shot.** This builds a layout, uses it once and drops it, which is
/// right for a test or a single measurement and wrong for anything that
/// paints repeatedly: a renderer calling this per frame re-cascades and
/// re-shapes its whole document to draw an unchanged screen. Panes go
/// through [`RetainedLayout`] instead.
pub fn scene_from_dom(dom: &ScriptedDom, sheet: &str, w: u32, h: u32) -> netrender::Scene {
    let mut snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    let viewport = DeviceIntSize::new(w as i32, h as i32);
    let plist = paint_snapshot(dom, &mut snapshot, w, h, &HashMap::new(), (0.0, 0.0));
    composite(viewport, &plist)
}
