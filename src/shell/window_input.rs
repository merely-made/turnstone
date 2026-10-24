// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Native input seams shared with the host's deterministic input fixtures.
use super::content_pool::ContentKey;
use crate::surface::{FocusTarget, Surface, SurfaceId, SurfaceKind};
use inker::{
    KeyboardModifiers, PhysicalPosition, PointerButtons, PointerEvent, PointerPhase, PointerType,
};
use pelt_core::{PeltControllerPool, PeltHostEffect, PeltInput};
use std::collections::HashMap;

pub(super) type Pool = PeltControllerPool<ContentKey, SurfaceId, netrender::Scene>;

/// OS IME admission follows the live retained editor in this window. A hidden
/// placement, a different window or a browser without retained composition
/// cannot inherit another document's editable focus.
pub(super) fn document_accepts_ime(pool: &Pool, focus: FocusTarget, window: u64) -> bool {
    let FocusTarget::Content { node, appearance } = focus else { return false; };
    let Some(placement) = pool.placement(&appearance) else { return false; };
    let same_node = match placement.content {
        ContentKey::Node(owner) | ContentKey::Reader { node: owner, .. } => owner == node,
    };
    same_node && placement.window == window && placement.visible && placement.focused
        && pool.focused_content(window) == Some(&placement.content)
        && pool.content(&placement.content)
            .and_then(pelt_core::PeltContent::document)
            .is_some_and(|controller| controller.session().editable_focus())
}

/// Current published caret, mapped once from producer logical viewport to this
/// native window's physical coordinates. Placement custody rejects geometry
/// from another appearance, hidden receiver, replacement or window.
pub(super) fn document_text_input_area(pool: &Pool, focus: FocusTarget, window: u64) -> Option<crate::surface::Rect> {
    let FocusTarget::Content { node, appearance } = focus else { return None; };
    let placement = pool.placement(&appearance)?;
    let owner = match placement.content {
        ContentKey::Node(owner) | ContentKey::Reader { node: owner, .. } => owner,
    };
    if owner != node || placement.window != window || !placement.focused
        || pool.focused_content(window) != Some(&placement.content)
        || !pool.admits(&appearance, pool.placement_generation(&appearance)?)
    {
        return None;
    }
    let caret = pool.text_input_state(&appearance)?.caret;
    let scale = f64::from(placement.scale_factor);
    let rect = [
        (f64::from(placement.rect.x) + caret.x) * scale,
        (f64::from(placement.rect.y) + caret.y) * scale,
        caret.width * scale, caret.height * scale,
    ];
    if !rect.into_iter().all(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX))
        || rect[2] <= 0.0 || rect[3] <= 0.0
    {
        return None;
    }
    Some(crate::surface::Rect::new(rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32))
}

#[derive(Clone, Copy)]
struct Contact {
    pointer_id: i32,
    primary: bool,
}
#[derive(Default)]
pub(super) struct TouchInput {
    contacts: HashMap<(u64, u64), Contact>,
    next_id: i32,
}
impl TouchInput {
    pub(super) fn deliver(
        &mut self,
        pool: &mut Pool,
        plan: &[Surface],
        focus: FocusTarget,
        window: u64,
        host_id: u64,
        phase: PointerPhase,
        position: PhysicalPosition,
        pressure: Option<f32>,
        altitude_angle: Option<f32>,
        modifiers: KeyboardModifiers,
    ) -> bool {
        self.contacts
            .retain(|_, contact| pool.captured_placement(contact.pointer_id).is_some());
        let key = (window, host_id);
        let mut hit_id = None;
        let contact = if phase == PointerPhase::Down {
            let Some(hit) = crate::surface::hit_test(plan, focus, position.x, position.y) else {
                return false;
            };
            let SurfaceKind::Content(node) = hit.kind else {
                return false;
            };
            if pool.placement(&hit.id).is_none_or(|p| p.window != window)
                || pool
                    .content(&ContentKey::Node(node))
                    .is_none_or(|content| content.lane() != pelt_core::PeltLane::Surface)
            {
                return false;
            }
            hit_id = Some(hit.id);
            if let Some(previous) = self.contacts.remove(&key) {
                if let Some(placement) = pool.captured_placement(previous.pointer_id).copied() {
                    let mut event = touch_event(
                        previous,
                        PointerPhase::Cancel,
                        position,
                        pressure,
                        altitude_angle,
                        modifiers,
                    );
                    event.buttons = PointerButtons::NONE;
                    let mut input = PeltInput::Pointer(event);
                    if let Some(scale) = pool.placement(&placement).map(|p| p.scale_factor) {
                        input.to_layout(scale);
                    }
                    pool.input(&placement, input);
                }
            }
            let pointer_id = loop {
                let candidate = self.next_id.max(2);
                self.next_id = candidate.checked_add(1).unwrap_or(2);
                if self
                    .contacts
                    .values()
                    .all(|contact| contact.pointer_id != candidate)
                {
                    break candidate;
                }
            };
            let contact = Contact {
                pointer_id,
                primary: self.contacts.is_empty(),
            };
            self.contacts.insert(key, contact);
            contact
        } else {
            let Some(contact) = self.contacts.get(&key).copied() else {
                return false;
            };
            if pool.captured_placement(contact.pointer_id).is_none() {
                self.contacts.remove(&key);
                return false;
            }
            contact
        };
        // Placements are in layout pixels; the contact is physical.
        let scale = pool
            .captured_placement(contact.pointer_id)
            .or(hit_id.as_ref())
            .and_then(|id| pool.placement(id))
            .map_or(1.0, |placement| placement.scale_factor);
        let mut input = PeltInput::Pointer(touch_event(
            contact,
            phase,
            position,
            pressure,
            altitude_angle,
            modifiers,
        ));
        input.to_layout(scale);
        let effect = pool.input_at(window, input);
        if matches!(phase, PointerPhase::Up | PointerPhase::Cancel)
            || pool.captured_placement(contact.pointer_id).is_none()
        {
            self.contacts.remove(&key);
        }
        effect.handled
    }

    /// Actual native UA touch admission gate, before placement/receiver work.
    pub(super) fn reject_modal(&mut self, pool: &mut Pool, window: u64, open: bool) -> bool {
        if !open {
            return false;
        }
        // A held prompt rejects page activation but must still retire the
        // interrupted window's contacts, including physical Up/Cancel.
        self.cancel_window(pool, window);
        true
    }

    /// Retire only this native window's pointers, preserving logical focus.
    pub(super) fn cancel_window(&mut self, pool: &mut Pool, window: u64) {
        pool.cancel_window_pointers(window);
        self.contacts.retain(|(source, _), _| *source != window);
    }

    pub(super) fn focus_lost(&mut self, pool: &mut Pool, window: u64) {
        pool.input_at(
            window,
            PeltInput::Session(inker::SessionInput::Focus(false)),
        );
        self.contacts.retain(|(source, _), _| *source != window);
    }
}
fn touch_event(
    contact: Contact,
    phase: PointerPhase,
    position: PhysicalPosition,
    pressure: Option<f32>,
    altitude_angle: Option<f32>,
    modifiers: KeyboardModifiers,
) -> PointerEvent {
    PointerEvent {
        pointer_id: contact.pointer_id,
        pointer_type: PointerType::Touch,
        is_primary: contact.primary,
        phase,
        position,
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(inker::MouseButton::Left),
        buttons: if matches!(phase, PointerPhase::Down | PointerPhase::Move) {
            PointerButtons::PRIMARY
        } else {
            PointerButtons::NONE
        },
        width: 1.,
        height: 1.,
        pressure,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        altitude_angle,
        azimuth_angle: None,
        modifiers,
    }
}

pub(super) fn focused_input(
    pool: &mut Pool,
    window: u64,
    _appearance: SurfaceId,
    input: PeltInput,
) -> PeltHostEffect {
    pool.input_at(window, input)
}
pub(super) fn captured_mouse(pool: &Pool) -> Option<SurfaceId> {
    pool.captured_placement(1).copied()
}

/// A current retained mouse receiver in the event's own native window.
/// This is independent of whether the new coordinates have a fresh plan hit.
pub(super) fn captured_content_hit(
    pool: &Pool,
    window: u64,
    position: PhysicalPosition,
) -> Option<crate::surface::HitResult> {
    let id = captured_mouse(pool)?;
    let placement = pool.placement(&id)?;
    if placement.window != window
        || !pool.placement_generation(&id).is_some_and(|generation| pool.admits(&id, generation))
    {
        return None;
    }
    let node = match placement.content {
        ContentKey::Node(node) | ContentKey::Reader { node, .. } => node,
    };
    Some(crate::surface::HitResult {
        id,
        kind: SurfaceKind::Content(node),
        local: (
            position.x - placement.rect.x * placement.scale_factor,
            position.y - placement.rect.y * placement.scale_factor,
        ),
    })
}

/// Resolve an admitted content press after current placements are revalidated.
pub(super) fn content_press_target(
    focus: &mut FocusTarget,
    pool: &Pool,
    window: u64,
    position: PhysicalPosition,
    hit: crate::surface::HitResult,
) -> crate::surface::HitResult {
    let hit = captured_content_hit(pool, window, position).unwrap_or(hit);
    if let SurfaceKind::Content(node) = hit.kind {
        *focus = FocusTarget::Content {
            node,
            appearance: hit.id,
        };
    }
    hit
}

/// The shared native button boundary and its optional content admission.
pub(super) fn native_mouse_input(
    pool: &mut Pool,
    window_buttons: &mut HashMap<u64, PointerButtons>,
    window: u64,
    appearance: Option<SurfaceId>,
    position: PhysicalPosition,
    phase: PointerPhase,
    button: Option<inker::MouseButton>,
    modifiers: KeyboardModifiers,
) -> Option<(ContentKey, PeltHostEffect)> {
    // Capture retirement sends pool Cancel, not a physical button release.
    // Record native Down/Up even when this event has no content admission.
    if matches!(phase, PointerPhase::Down | PointerPhase::Up) {
        let previous = window_buttons
            .get(&window)
            .copied()
            .unwrap_or(PointerButtons::NONE);
        let buttons = mouse_buttons(window, previous, phase, button);
        window_buttons.insert(window, buttons);
    }
    content_mouse_input(
        pool,
        window_buttons,
        window,
        appearance,
        position,
        phase,
        button,
        modifiers,
    )
}

pub(super) fn content_mouse_input(
    pool: &mut Pool,
    window_buttons: &HashMap<u64, PointerButtons>,
    window: u64,
    appearance: Option<SurfaceId>,
    position: PhysicalPosition,
    phase: PointerPhase,
    button: Option<inker::MouseButton>,
    modifiers: KeyboardModifiers,
) -> Option<(ContentKey, PeltHostEffect)> {
    let appearance = appearance?;
    // Pool custody may outlive a new host hit. Name the actual captured
    // receiver when returning its navigation/submission effects.
    let appearance = captured_mouse(pool).unwrap_or(appearance);
    let placement = pool.placement(&appearance).cloned()?;
    if placement.window != window {
        return None;
    }
    let buttons = window_buttons
        .get(&window)
        .copied()
        .unwrap_or(PointerButtons::NONE);
    let event = super::input::mouse_pointer_event(
        position.x, position.y, phase, button, buttons, modifiers,
    );
    // The pool's placements are in layout pixels; the position is physical.
    let mut input = PeltInput::Pointer(event);
    input.to_layout(placement.scale_factor);
    let effect = pool.input_at(window, input);
    Some((placement.content, effect))
}

pub(super) fn lens_button(button: inker::MouseButton) -> bool {
    matches!(
        button,
        inker::MouseButton::Left | inker::MouseButton::Right | inker::MouseButton::Middle
    )
}
pub(super) fn mouse_buttons(
    _window: u64,
    previous: PointerButtons,
    phase: PointerPhase,
    button: Option<inker::MouseButton>,
) -> PointerButtons {
    let bit = match button {
        Some(inker::MouseButton::Left) => PointerButtons::PRIMARY.0,
        Some(inker::MouseButton::Right) => PointerButtons::SECONDARY.0,
        Some(inker::MouseButton::Middle) => PointerButtons::AUXILIARY.0,
        Some(inker::MouseButton::Back) => PointerButtons::BACK.0,
        Some(inker::MouseButton::Forward) => PointerButtons::FORWARD.0,
        _ => 0,
    };
    PointerButtons(match phase {
        PointerPhase::Down => previous.0 | bit,
        PointerPhase::Up => previous.0 & !bit,
        PointerPhase::Cancel => 0,
        _ => previous.0,
    })
}
pub(super) fn wheel_zoom(_window: u64, control: bool, delta_y: f32, offered: bool) -> bool {
    control && delta_y != 0.0 && offered
}

pub(crate) fn click_slop(value: Option<&str>) -> f32 {
    value
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(4.0)
}

#[cfg(test)]
#[path = "window_input_tests.rs"]
mod tests;
