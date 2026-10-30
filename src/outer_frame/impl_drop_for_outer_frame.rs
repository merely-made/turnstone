// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for OuterFrame {
    fn drop(&mut self) {
        use layout_dom_api::LayoutDomMut;
        let handle = self.controls.dom.clone();
        // Removing a root releases its entire subtree in Genet. Move live
        // bodies home first, preserving destination-first window custody.
        self.controls.withdraw_mounts(self.window_root);
        let mut dom = handle.borrow_mut();
        if dom.is_live(self.window_scope) {
            dom.remove(self.window_scope);
        }
    }
}
