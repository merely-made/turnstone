// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for OmnibarConfig {
    fn default() -> Self {
        Self {
            placement: ChromePlacement::Overlay,
            // The MAXIMUM rows the omnibar offers, not a fixed count: the
            // projection clamps it to what fits above the window's bottom
            // edge (`ui::visible_row_limit`). Ten rather than the original
            // seven — seven was sized for six node matches plus one
            // literal-open row, which left the actions and recall lanes
            // fighting the find lane for the same seats.
            row_limit: 10,
            default_scope: OmnibarScope::Address,
            shortcuts: vec![
                OmnibarShortcut {
                    key: 'l',
                    ctrl: true,
                    scope: OmnibarScope::Address,
                },
                OmnibarShortcut {
                    key: 'k',
                    ctrl: true,
                    scope: OmnibarScope::Command,
                },
            ],
        }
    }
}
