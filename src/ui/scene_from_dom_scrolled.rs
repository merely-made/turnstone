// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// [`scene_from_dom`] at a pane's scroll offset, with auto-hiding overlay bars.
///
/// The bars come from the engine's own `append_scrollbars`, which draws both
/// axes from fragment geometry at the container's edges, so they cannot land
/// outside the pane: this scene is composited at exactly the pane's viewport,
/// and anything past it is clipped by the composite rather than spilling into
/// a neighbouring pane.
///
/// **One-shot.** This builds a layout, uses it once and drops it, which is
/// right for a test or a single measurement and wrong for anything that
/// paints repeatedly: a renderer calling this per frame re-cascades and
/// re-shapes its whole document to draw an unchanged screen. Panes go
/// through [`RetainedLayout`] instead.
pub fn scene_from_dom_scrolled(
    dom: &ScriptedDom,
    sheet: &str,
    w: u32,
    h: u32,
    scroll: &mut PaneScroll,
) -> netrender::Scene {
    let mut snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    scroll.offset = clamp_scroll(scroll.offset, snapshot.content_extent, (w, h));
    let viewport = DeviceIntSize::new(w as i32, h as i32);
    let mut plist = paint_snapshot(dom, &mut snapshot, w, h, &HashMap::new(), scroll.offset);
    append_scrollbars(
        &mut plist,
        snapshot.content_extent,
        (w, h),
        scroll.offset,
        scroll.alpha(),
    );
    composite(viewport, &plist)
}
