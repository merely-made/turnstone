// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The viewport row budget when one visible row has grown beyond the ordinary
/// single-line height. `extra_height` is already in live chrome pixels.
pub(crate) fn visible_row_limit_with_extra_height(
    configured: usize,
    placement: &crate::panes::ChromePlacement,
    viewport_h: f32,
    ui_zoom: f32,
    extra_height: f32,
) -> usize {
    use crate::panes::{ChromeEdge, ChromePlacement};
    let zoom = if ui_zoom > 0.0 { ui_zoom } else { 1.0 };
    // Where the card's top edge lands, matching `chrome_view::chrome_position`.
    // A placement with no chrome station has no geometry to read, so the
    // configured value stands alone.
    let card_top = match placement {
        ChromePlacement::Overlay | ChromePlacement::Floating => CARD_TOP,
        ChromePlacement::Docked(ChromeEdge::Top) => 8.0,
        ChromePlacement::Docked(ChromeEdge::Bottom) => (viewport_h - 52.0).max(8.0),
        ChromePlacement::Docked(ChromeEdge::Left | ChromeEdge::Right) => {
            (viewport_h * 0.5 - 18.0).max(8.0)
        },
        ChromePlacement::Pane(_) | ChromePlacement::Hidden => return configured,
    };
    let (row_height, card_chrome_height) = ordinary_chrome_row_metrics(zoom);
    let room =
        viewport_h - card_top - card_chrome_height - CARD_BOTTOM_MARGIN - extra_height.max(0.0);
    let fits = (room / row_height).floor().max(0.0) as usize;
    // One complete review takes priority over three ordinary rows. Do not
    // force extra suggestions into room that its wrapped text already needs.
    let floor = if extra_height > 0.0 {
        1
    } else {
        MIN_VISIBLE_ROWS
    };
    configured.min(fits.max(floor))
}
