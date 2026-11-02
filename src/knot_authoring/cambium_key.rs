// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A neutral session key as the Cambium key event Knot's editor takes.
pub(super) fn cambium_key(key: SessionKey, modifiers: SessionModifiers) -> Option<KeyEvent> {
    let key = match key {
        SessionKey::Character(text) => Key::Character(text),
        SessionKey::Enter => Key::Named(NamedKey::Enter),
        SessionKey::Tab => Key::Named(NamedKey::Tab),
        SessionKey::Backspace => Key::Named(NamedKey::Backspace),
        SessionKey::Delete => Key::Named(NamedKey::Delete),
        SessionKey::Escape => Key::Named(NamedKey::Escape),
        SessionKey::Space => Key::Named(NamedKey::Space),
        SessionKey::ArrowLeft => Key::Named(NamedKey::ArrowLeft),
        SessionKey::ArrowRight => Key::Named(NamedKey::ArrowRight),
        SessionKey::ArrowUp => Key::Named(NamedKey::ArrowUp),
        SessionKey::ArrowDown => Key::Named(NamedKey::ArrowDown),
        SessionKey::Home => Key::Named(NamedKey::Home),
        SessionKey::End => Key::Named(NamedKey::End),
        SessionKey::PageUp => Key::Named(NamedKey::PageUp),
        SessionKey::PageDown => Key::Named(NamedKey::PageDown),
        SessionKey::Unidentified => return None,
    };
    Some(KeyEvent::with_mods(
        key,
        cambium::Modifiers {
            shift: modifiers.shift,
            ctrl: modifiers.control,
            alt: modifiers.alt,
            meta: modifiers.meta,
        },
    ))
}
