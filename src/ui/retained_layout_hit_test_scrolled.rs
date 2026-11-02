// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RetainedLayout {
    /// [`hit_test`](Self::hit_test) at a pane's scroll offset.
    ///
    /// **One-shot.** This builds a layout, uses it once and drops it, which is
    /// right for a test or a single measurement and wrong for anything that
    /// paints repeatedly: a renderer calling this per frame re-cascades and
    /// re-shapes its whole document to draw an unchanged screen. Panes go
    /// through [`RetainedLayout`] instead.
    pub fn hit_test_scrolled(
        &mut self,
        dom: &mut ScriptedDom,
        sheet: &str,
        w: u32,
        h: u32,
        x: f32,
        y: f32,
        scroll: &PaneScroll,
    ) -> Option<DomNodeId> {
        if self.inline_target().is_some() {
            return self.inline_hit(dom, x, y);
        }

        self.ensure(dom, sheet, w, h);
        let scoped =
            cambium_rootstock::WindowDom::new(dom, self.root.unwrap_or_else(|| dom.document()));
        let snapshot = self.snapshot.as_ref().expect("ensure leaves a snapshot");
        let scale = self.scale;
        self.viewport_scroll = clamp_scroll(
            (scroll.offset.0 / scale, scroll.offset.1 / scale),
            snapshot.content_extent,
            self.logical(),
        );
        snapshot_hit_test(
            &scoped,
            snapshot,
            x / scale,
            y / scale,
            &self.element_scroll,
            self.viewport_scroll,
            self.logical(),
        )
    }
}
