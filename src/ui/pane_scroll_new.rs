// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneScroll {
    pub fn new() -> Self {
        Self {
            offset: (0.0, 0.0),
            fade: cambium_winit::ScrollbarFade::new(),
        }
    }

    /// Move by a wheel delta and wake the bars.
    ///
    /// Deliberately unclamped here: the range depends on the laid-out content,
    /// which only exists at paint. Painting clamps and writes the corrected
    /// value back, so an overscroll never accumulates into a pane that must be
    /// scrolled back through empty space.
    pub fn nudge(&mut self, dx: f32, dy: f32) {
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
        self.fade.note((), std::time::Instant::now());
    }

    pub fn offset(&self) -> (f32, f32) {
        self.offset
    }

    /// Whether the bars are still visible, so a caller can keep animating the
    /// fade out rather than leaving a bar frozen on screen.
    pub fn bars_visible(&mut self) -> bool {
        self.fade.any_visible(std::time::Instant::now())
    }

    pub(super) fn alpha(&self) -> f32 {
        self.fade.alpha((), std::time::Instant::now())
    }
}
