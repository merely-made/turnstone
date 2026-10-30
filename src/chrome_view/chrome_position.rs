// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Map a configured projection placement to this pre-A4 chrome surface. A
/// `Pane` placement has no station yet, so it deliberately renders nothing
/// rather than fabricating a second pane tree.
pub(crate) fn chrome_position(
    placement: &ChromePlacement,
    width: f32,
    height: f32,
    item_width: f32,
) -> Option<(f32, f32)> {
    let centred = || ((width - item_width) / 2.0).max(8.0);
    match placement {
        ChromePlacement::Overlay | ChromePlacement::Floating => Some((centred(), CARD_TOP)),
        ChromePlacement::Docked(ChromeEdge::Top) => Some((centred(), 8.0)),
        ChromePlacement::Docked(ChromeEdge::Bottom) => Some((centred(), (height - 52.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Left) => Some((8.0, (height * 0.5 - 18.0).max(8.0))),
        ChromePlacement::Docked(ChromeEdge::Right) => Some((
            (width - item_width - 8.0).max(8.0),
            (height * 0.5 - 18.0).max(8.0),
        )),
        ChromePlacement::Pane(_) | ChromePlacement::Hidden => None,
    }
}
