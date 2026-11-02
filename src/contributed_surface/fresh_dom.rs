// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fresh_dom() -> DomHandle {
    std::rc::Rc::new(std::cell::RefCell::new(ScriptedDom::new()))
}
