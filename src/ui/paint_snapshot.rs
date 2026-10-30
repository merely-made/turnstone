// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(test)]
thread_local! {
    static PAINT_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn count_paint_calls<R>(work: impl FnOnce() -> R) -> (R, usize) {
    PAINT_CALLS.with(|count| count.set(0));
    let result = work();
    (result, PAINT_CALLS.with(std::cell::Cell::get))
}

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
    #[cfg(test)]
    PAINT_CALLS.with(|count| count.set(count.get() + 1));
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
