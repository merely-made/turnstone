// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn append_scrollbars(
    list: &mut LiveryPaintList,
    content: (f32, f32),
    viewport: (u32, u32),
    scroll: (f32, f32),
    alpha: f32,
) {
    if alpha <= 0.0 {
        return;
    }
    let color = paint_list_api::ColorF {
        r: 0.55,
        g: 0.60,
        b: 0.70,
        a: 0.75 * alpha.clamp(0.0, 1.0),
    };
    let (vw, vh) = (viewport.0 as f32, viewport.1 as f32);
    let max_y = (content.1 - vh).max(0.0);
    if max_y > 0.0 {
        let thumb_h = (vh * vh / content.1).clamp(24.0, vh);
        let y = scroll.1 / max_y * (vh - thumb_h);
        list.push_overlay_rect(
            paint_list_api::LayoutRect::from_origin_and_size(
                paint_list_api::LayoutPoint::new((vw - 6.0).max(0.0), y),
                paint_list_api::LayoutSize::new(4.0, thumb_h),
            ),
            color,
        );
    }
    let max_x = (content.0 - vw).max(0.0);
    if max_x > 0.0 {
        let thumb_w = (vw * vw / content.0).clamp(24.0, vw);
        let x = scroll.0 / max_x * (vw - thumb_w);
        list.push_overlay_rect(
            paint_list_api::LayoutRect::from_origin_and_size(
                paint_list_api::LayoutPoint::new(x, (vh - 6.0).max(0.0)),
                paint_list_api::LayoutSize::new(thumb_w, 4.0),
            ),
            color,
        );
    }
}
