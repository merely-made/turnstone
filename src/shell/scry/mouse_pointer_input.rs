// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn mouse_pointer_input(ev: &PointerEvent) -> Result<native::MouseInput, SurfaceError> {
    use native::MouseEventKind as Kind;
    let (kind, mouse_data) = match (ev.phase, ev.button) {
        (PointerPhase::Move, _) => (Kind::Move, 0),
        (PointerPhase::Cancel, _) => (Kind::Leave, 0),
        (PointerPhase::Down, Some(MouseButton::Left)) => (Kind::LeftButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Left)) => (Kind::LeftButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Middle)) => (Kind::MiddleButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Middle)) => (Kind::MiddleButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Right)) => (Kind::RightButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Right)) => (Kind::RightButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Back)) => (Kind::XButtonDown, 1),
        (PointerPhase::Up, Some(MouseButton::Back)) => (Kind::XButtonUp, 1),
        (PointerPhase::Down, Some(MouseButton::Forward)) => (Kind::XButtonDown, 2),
        (PointerPhase::Up, Some(MouseButton::Forward)) => (Kind::XButtonUp, 2),
        (_, None) => {
            return Err(SurfaceError::InputFailed(
                "Scry mouse Down/Up requires the changed button".into(),
            ));
        },
    };
    Ok(native::MouseInput {
        kind,
        virtual_keys: native::MouseVirtualKeys {
            control: ev.modifiers.ctrl,
            shift: ev.modifiers.shift,
            left_button: ev.buttons.contains(PointerButtons::PRIMARY),
            right_button: ev.buttons.contains(PointerButtons::SECONDARY),
            middle_button: ev.buttons.contains(PointerButtons::AUXILIARY),
            x_button1: ev.buttons.contains(PointerButtons::BACK),
            x_button2: ev.buttons.contains(PointerButtons::FORWARD),
        },
        mouse_data,
        point: (ev.position.x as i32, ev.position.y as i32),
    })
}
