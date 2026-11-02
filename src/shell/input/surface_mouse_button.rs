// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn surface_mouse_button(button: MouseButton) -> Option<inker::MouseButton> {
    match button {
        MouseButton::Left => Some(inker::MouseButton::Left),
        MouseButton::Middle => Some(inker::MouseButton::Middle),
        MouseButton::Right => Some(inker::MouseButton::Right),
        MouseButton::Back => Some(inker::MouseButton::Back),
        MouseButton::Forward => Some(inker::MouseButton::Forward),
        _ => None,
    }
}
