// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn surface_button_mask(button: MouseButton) -> inker::PointerButtons {
    match button {
        MouseButton::Left => inker::PointerButtons::PRIMARY,
        MouseButton::Right => inker::PointerButtons::SECONDARY,
        MouseButton::Middle => inker::PointerButtons::AUXILIARY,
        MouseButton::Back => inker::PointerButtons::BACK,
        MouseButton::Forward => inker::PointerButtons::FORWARD,
        _ => inker::PointerButtons::NONE,
    }
}
