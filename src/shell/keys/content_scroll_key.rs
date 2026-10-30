// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_scroll_key(key: &WinitKey, shift: bool) -> Option<SessionScrollKey> {
    Some(match key {
        WinitKey::Named(WinitNamedKey::ArrowDown) => SessionScrollKey::LineDown,
        WinitKey::Named(WinitNamedKey::ArrowUp) => SessionScrollKey::LineUp,
        WinitKey::Named(WinitNamedKey::PageDown) => SessionScrollKey::PageDown,
        WinitKey::Named(WinitNamedKey::PageUp) => SessionScrollKey::PageUp,
        WinitKey::Named(WinitNamedKey::Home) => SessionScrollKey::Home,
        WinitKey::Named(WinitNamedKey::End) => SessionScrollKey::End,
        WinitKey::Named(WinitNamedKey::Space) if shift => SessionScrollKey::PageUp,
        WinitKey::Named(WinitNamedKey::Space) => SessionScrollKey::PageDown,
        _ => return None,
    })
}
