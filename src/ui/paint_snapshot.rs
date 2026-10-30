// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn paint_snapshot<D>(
    dom: &D,
    snapshot: &mut LiverySnapshot<D::NodeId>,
    w: u32,
    h: u32,
    element_scroll: &HashMap<D::NodeId, (f32, f32)>,
    viewport_scroll: (f32, f32),
) -> LiveryPaintList
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    emit_paint_list_with_text_system_scrolled_with_images(
        dom,
        &snapshot.styles,
        &snapshot.fragments,
        DeviceIntSize::new(w as i32, h as i32),
        0,
        &mut snapshot.text,
        element_scroll,
        &HashMap::new(),
    )
    .translated(-viewport_scroll.0, -viewport_scroll.1)
}
