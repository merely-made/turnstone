// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(in super::super) fn mouse_pointer_event(
    x: f32,
    y: f32,
    phase: inker::PointerPhase,
    button: Option<inker::MouseButton>,
    buttons: inker::PointerButtons,
    modifiers: inker::KeyboardModifiers,
) -> inker::PointerEvent {
    inker::PointerEvent {
        pointer_id: 1,
        pointer_type: inker::PointerType::Mouse,
        is_primary: true,
        phase,
        position: inker::PhysicalPosition { x, y },
        button,
        buttons,
        width: 1.0,
        height: 1.0,
        pressure: None,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        altitude_angle: None,
        azimuth_angle: None,
        modifiers,
    }
}
