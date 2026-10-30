// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn keyboard_params(
    kind: &str,
    event: &native::KeyboardInput,
    text: Option<&str>,
) -> String {
    // Match Scry 0.7.1's key identities and native virtual-key fields. JSON
    // serialization preserves Unicode, quotes and control characters.
    let (key, code) = match event.virtual_key_code {
        0x30..=0x39 => {
            let ch = char::from_u32(event.virtual_key_code).unwrap_or('0');
            (ch.to_string(), format!("Digit{ch}"))
        },
        0x41..=0x5A => {
            let upper = char::from_u32(event.virtual_key_code).unwrap_or('A');
            let key = event
                .characters_ignoring_modifiers
                .chars()
                .next()
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| upper.to_ascii_lowercase().to_string());
            (key, format!("Key{upper}"))
        },
        0x10 | 0xA0 | 0xA1 => ("Shift".into(), "ShiftLeft".into()),
        0x11 | 0xA2 | 0xA3 => ("Control".into(), "ControlLeft".into()),
        0x12 | 0xA4 | 0xA5 => ("Alt".into(), "AltLeft".into()),
        0x5B | 0x5C => ("Meta".into(), "MetaLeft".into()),
        0x0D => ("Enter".into(), "Enter".into()),
        0x08 => ("Backspace".into(), "Backspace".into()),
        0x09 => ("Tab".into(), "Tab".into()),
        0x1B => ("Escape".into(), "Escape".into()),
        0x20 => (" ".into(), "Space".into()),
        _ => (
            event
                .characters_ignoring_modifiers
                .chars()
                .next()
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| "Unidentified".into()),
            String::new(),
        ),
    };
    let modifiers = u32::from(event.modifiers.alt)
        | (u32::from(event.modifiers.control) << 1)
        | (u32::from(event.modifiers.meta) << 2)
        | (u32::from(event.modifiers.shift) << 3);
    let mut params = serde_json::json!({
        "type": kind,
        "windowsVirtualKeyCode": event.virtual_key_code,
        "nativeVirtualKeyCode": event.virtual_key_code,
        "code": code, "key": key, "modifiers": modifiers, "autoRepeat": event.is_repeat,
    });
    if let Some(text) = text {
        params["text"] = text.into();
        params["unmodifiedText"] = event.characters_ignoring_modifiers.clone().into();
    }
    params.to_string()
}
