// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The same Genet content input view in native and browser hosts.
use super::*;
use cambium::{GenetCtx, GenetElement, View, custom_leaf};
pub fn content_view<State: 'static, Action: 'static>(
    key: u64,
    slot: Rc<RefCell<ContentSlot>>,
    width: u32,
    height: u32,
) -> impl View<State, Action, GenetCtx, Element = GenetElement> {
    content_view_with_style(
        key,
        slot,
        width,
        height,
        format!("display:block;width:100%;height:{height}px;flex-shrink:0;"),
        false,
    )
}

/// Fill the space allocated by a column host, after its header and status.
/// The producer receives the actual layout size rather than a fixed minimum.
/// Wheel input stays in this bounded reader, including at its limits. The
/// fixed-size `content_view` remains available for hosts that want chaining.
pub fn filling_content_view<State: 'static, Action: 'static>(
    key: u64,
    slot: Rc<RefCell<ContentSlot>>,
) -> impl View<State, Action, GenetCtx, Element = GenetElement> {
    content_view_with_style(
        key,
        slot,
        0,
        0,
        "display:block;width:100%;height:0;min-height:0;flex-grow:1;flex-shrink:1;flex-basis:0px;"
            .into(),
        true,
    )
}

fn content_view_with_style<State: 'static, Action: 'static>(
    key: u64,
    slot: Rc<RefCell<ContentSlot>>,
    width: u32,
    height: u32,
    style: String,
    contain_wheel: bool,
) -> impl View<State, Action, GenetCtx, Element = GenetElement> {
    let pointer_slot = slot.clone();
    let wheel_slot = slot.clone();
    let key_slot = slot.clone();
    let focus_slot = slot.clone();
    let leaf = cambium::on_focus(
        cambium::on_key(
            cambium::on_wheel(
                cambium::on_pointer(
                    cambium::on_click(
                        custom_leaf::<State, Action>(key, width, height)
                            .attr("class", "pelt-reading-slot")
                            .attr("tabindex", "0")
                            .attr(
                                "aria-label",
                                format!("Retained document {}", slot.borrow().source().title),
                            )
                            .attr("style", style),
                        move |_: &mut State, click| {
                            // The tile's activation click must not replace the
                            // retained input target before pointer capture starts.
                            click.stop_propagation();
                        },
                    ),
                    move |_: &mut State, event| {
                        let input = match event.phase {
                            cambium::PointerPhase::Cancel => SessionInput::Cancel,
                            cambium::PointerPhase::Down | cambium::PointerPhase::Up => {
                                let button = match event.button {
                                    cambium::PointerButton::Primary => {
                                        SessionPointerButton::Primary
                                    },
                                    cambium::PointerButton::Secondary => {
                                        SessionPointerButton::Secondary
                                    },
                                    _ => return,
                                };
                                SessionInput::PointerButton {
                                    x: event.local.0,
                                    y: event.local.1,
                                    button,
                                    state: if event.phase == cambium::PointerPhase::Down {
                                        SessionButtonState::Pressed
                                    } else {
                                        SessionButtonState::Released
                                    },
                                    modifiers: SessionModifiers::default(),
                                }
                            },
                            cambium::PointerPhase::Move => SessionInput::PointerMoved {
                                x: event.local.0,
                                y: event.local.1,
                                modifiers: SessionModifiers::default(),
                            },
                        };
                        if pointer_slot.borrow_mut().input(input) {
                            event.prop.prevent_default();
                        }
                    },
                ),
                move |_: &mut State, event| {
                    let mut slot = wheel_slot.borrow_mut();
                    let changed = slot.scroll(event.delta.0, event.delta.1);
                    if changed || (contain_wheel && slot.is_visible()) {
                        event.prevent_default();
                    }
                },
            ),
            move |_: &mut State, event| {
                let key = match &event.key {
                    cambium::Key::Composition(composition) => {
                        let ime = match composition {
                            cambium::CompositionEvent::Enabled => SessionIme::Enabled,
                            cambium::CompositionEvent::Preedit { text, selection } => {
                                SessionIme::Preedit {
                                    text: text.clone(),
                                    selection: *selection,
                                }
                            },
                            cambium::CompositionEvent::Commit(text) => {
                                SessionIme::Commit(text.clone())
                            },
                            cambium::CompositionEvent::Disabled => SessionIme::Disabled,
                        };
                        if key_slot.borrow_mut().input(SessionInput::Ime(ime)) {
                            event.prevent_default();
                        }
                        return;
                    },
                    cambium::Key::Character(c) => SessionKey::Character(c.clone()),
                    cambium::Key::Named(n) => match n {
                        cambium::NamedKey::Enter => SessionKey::Enter,
                        cambium::NamedKey::Tab => SessionKey::Tab,
                        cambium::NamedKey::Backspace => SessionKey::Backspace,
                        cambium::NamedKey::Delete => SessionKey::Delete,
                        cambium::NamedKey::Escape => SessionKey::Escape,
                        cambium::NamedKey::Space => SessionKey::Space,
                        cambium::NamedKey::ArrowUp => SessionKey::ArrowUp,
                        cambium::NamedKey::ArrowDown => SessionKey::ArrowDown,
                        cambium::NamedKey::ArrowLeft => SessionKey::ArrowLeft,
                        cambium::NamedKey::ArrowRight => SessionKey::ArrowRight,
                        cambium::NamedKey::Home => SessionKey::Home,
                        cambium::NamedKey::End => SessionKey::End,
                        cambium::NamedKey::PageUp => SessionKey::PageUp,
                        cambium::NamedKey::PageDown => SessionKey::PageDown,
                        _ => SessionKey::Unidentified,
                    },
                };
                let modifiers = SessionModifiers {
                    shift: event.mods.shift,
                    control: event.mods.ctrl,
                    alt: event.mods.alt,
                    meta: event.mods.meta,
                };
                let mut slot = key_slot.borrow_mut();
                if let SessionKey::Character(text) = &key
                    && !modifiers.control
                    && !modifiers.alt
                    && !modifiers.meta
                {
                    if slot.input(SessionInput::Text(text.clone())) {
                        event.prevent_default();
                    }
                    return;
                }
                if slot.input(SessionInput::Key {
                    key: key.clone(),
                    state: SessionButtonState::Pressed,
                    modifiers,
                    repeat: false,
                }) {
                    event.prevent_default();
                } else if let Some(key) = match key {
                    SessionKey::PageDown => Some(SessionScrollKey::PageDown),
                    SessionKey::PageUp => Some(SessionScrollKey::PageUp),
                    SessionKey::Home => Some(SessionScrollKey::Home),
                    SessionKey::End => Some(SessionScrollKey::End),
                    _ => None,
                } {
                    if slot.scroll_key(key) {
                        event.prevent_default();
                    }
                }
            },
        ),
        move |_: &mut State, focused| {
            focus_slot.borrow_mut().input(SessionInput::Focus(
                focused.phase == cambium::FocusPhase::Gained,
            ));
        },
    );
    leaf
}
