// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn windows_virtual_key(key: &WinitKey) -> u32 {
    match key {
        WinitKey::Named(WinitNamedKey::Backspace) => 0x08,
        WinitKey::Named(WinitNamedKey::Tab) => 0x09,
        WinitKey::Named(WinitNamedKey::Enter) => 0x0D,
        WinitKey::Named(WinitNamedKey::Shift) => 0x10,
        WinitKey::Named(WinitNamedKey::Control) => 0x11,
        WinitKey::Named(WinitNamedKey::Alt) => 0x12,
        WinitKey::Named(WinitNamedKey::Pause) => 0x13,
        WinitKey::Named(WinitNamedKey::CapsLock) => 0x14,
        WinitKey::Named(WinitNamedKey::Escape) => 0x1B,
        WinitKey::Named(WinitNamedKey::Space) => 0x20,
        WinitKey::Named(WinitNamedKey::PageUp) => 0x21,
        WinitKey::Named(WinitNamedKey::PageDown) => 0x22,
        WinitKey::Named(WinitNamedKey::End) => 0x23,
        WinitKey::Named(WinitNamedKey::Home) => 0x24,
        WinitKey::Named(WinitNamedKey::ArrowLeft) => 0x25,
        WinitKey::Named(WinitNamedKey::ArrowUp) => 0x26,
        WinitKey::Named(WinitNamedKey::ArrowRight) => 0x27,
        WinitKey::Named(WinitNamedKey::ArrowDown) => 0x28,
        WinitKey::Named(WinitNamedKey::Insert) => 0x2D,
        WinitKey::Named(WinitNamedKey::Delete) => 0x2E,
        WinitKey::Character(value) => value
            .chars()
            .next()
            .map(|character| character.to_ascii_uppercase() as u32)
            .unwrap_or(0),
        _ => 0,
    }
}
