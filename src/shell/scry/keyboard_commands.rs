// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn keyboard_commands(
    event: &native::KeyboardInput,
) -> Result<Vec<String>, native::WebSurfaceError> {
    use native::KeyEventKind;
    let commands = match event.kind {
        KeyEventKind::Down => {
            let mut commands = vec![keyboard_params("rawKeyDown", event, None)];
            if !event.characters.is_empty() {
                commands.push(keyboard_params("char", event, Some(&event.characters)));
            }
            commands
        },
        KeyEventKind::Up => vec![keyboard_params("keyUp", event, None)],
        KeyEventKind::ModifiersChanged => {
            let down = match event.virtual_key_code {
                0x10 | 0xA0 | 0xA1 => event.modifiers.shift,
                0x11 | 0xA2 | 0xA3 => event.modifiers.control,
                0x12 | 0xA4 | 0xA5 => event.modifiers.alt,
                0x5B | 0x5C => event.modifiers.meta,
                0x14 => event.modifiers.caps_lock,
                _ => false,
            };
            vec![keyboard_params(
                if down { "rawKeyDown" } else { "keyUp" },
                event,
                None,
            )]
        },
        _ => {
            return Err(native::WebSurfaceError::Unsupported(
                "Turnstone has no CDP mapping for this keyboard event kind",
            ));
        },
    };
    Ok(commands)
}
