// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

impl RetainedLayout {
    pub(crate) fn reset_element_scroll(&mut self, dom: &ScriptedDom, node: DomNodeId) {
        if let Some((layout, owner)) = self.inline_target() {
            if cambium_rootstock::WindowDom::new(dom, owner).contains(node) {
                layout.borrow_mut().reset_element_scroll(dom, node);
            }
        } else {
            self.element_scroll.remove(&node);
            self.projections.get_mut().clear();
        }
    }

    /// Scroll an overflow owner in this pane, including an inline mount.
    pub(crate) fn scroll_element(&mut self, dom: &ScriptedDom, node: DomNodeId, dy: f32) -> bool {
        if let Some((layout, owner)) = self.inline_target() {
            return cambium_rootstock::WindowDom::new(dom, owner).contains(node)
                && layout.borrow_mut().scroll_element(dom, node, dy);
        }
        let Some(snapshot) = self.snapshot.as_ref() else {
            return false;
        };
        let Some(fragment) = snapshot.fragments.get(node) else {
            return false;
        };
        let subtree = cambium_rootstock::WindowDom::new(dom, node);
        let extent = content_extent(&subtree, &snapshot.fragments);
        let max = (extent.1 - fragment.y - fragment.height).max(0.0);
        if max <= 0.0 {
            return false;
        }
        let offset = self.element_scroll.entry(node).or_default();
        offset.1 = (offset.1 - dy / self.scale).clamp(0.0, max);
        self.projections.get_mut().clear();
        true
    }
}
