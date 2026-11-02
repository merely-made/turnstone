// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl<'a, D: LayoutDom> SubtreeView<'a, D> {
    pub(super) fn new(dom: &'a D, root: D::NodeId) -> Self {
        Self { dom, root }
    }
}
