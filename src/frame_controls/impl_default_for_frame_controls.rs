// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for FrameControls {
    fn default() -> Self {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let root = dom.borrow().document();
        Self::new_at(dom, root)
    }
}
