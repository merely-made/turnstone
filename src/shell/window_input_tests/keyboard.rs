// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn keyboard(code: u32, text: Option<&str>) -> PeltInput {
    PeltInput::Keyboard(KeyboardEvent {
        key_code: code,
        scan_code: 0,
        modifiers: Default::default(),
        pressed: true,
        text: text.map(str::to_owned),
    })
}
