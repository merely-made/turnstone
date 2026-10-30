// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Debug for OmnibarState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmnibarState")
            .field("open", &self.open)
            .field("mode", &self.mode)
            .field(
                "text",
                &if matches!(self.mode, OmnibarMode::MicronForm(_)) {
                    "[redacted]".to_string()
                } else {
                    self.presented_text()
                },
            )
            .field("cursor", &self.cursor)
            .field("selected", &self.selected)
            .field("suggestions", &self.suggestions)
            .finish()
    }
}
