// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl A11yFocusPolicy {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::DocumentRoot => "document-root",
        }
    }

    pub(super) fn project(self, update: &mut SurfaceAccessibilityUpdate) {
        // Pinned AAC exports a fixed placeholder, not DOM keyboard focus.
        // This explicit partial projection uses AccessKit's document-root
        // representation; it never supplies missing tree metadata or nodes.
        if self == Self::DocumentRoot {
            if let Some(tree) = &update.tree {
                update.focus = tree.root;
            }
        }
    }
}
