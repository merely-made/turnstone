// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for OuterFrame {
    fn default() -> Self {
        Self::new(cambium::DomHandle::new(std::cell::RefCell::new(
            ScriptedDom::new(),
        )))
    }
}
