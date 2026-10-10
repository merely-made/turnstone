// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Scenario steps for the page's gestures: synthetic browser events through
//! the listeners a real wheel or touch reaches (`gestures`).
//!
//! Each step is dispatched from the event loop, as `reader-click` is, since
//! the step runs while the host is busy with its frame. What happened lands
//! in the `gesture` snapshot field: whether the host heard of the event
//! (`withheld`), and whether the page's default was taken (`kept` for a
//! wheel, `prevented` for a touch). A synthetic wheel never scrolls a page,
//! so `kept` is the receipt that a real one would.
use web_sys::{
    PointerEventInit, Touch, TouchEvent, TouchEventInit, TouchInit, WheelEvent, WheelEventInit,
};

use super::*;

/// A gesture to dispatch, and what it found.
type Gesture = Box<dyn FnOnce(&Shared) -> Option<String>>;

/// A scenario's pointer ids, apart from any a real device uses.
const FIRST_FINGER: i32 = 801;
const SECOND_FINGER: i32 = 802;

fn canvas() -> Option<Element> {
    web_sys::window()?
        .document()?
        .get_element_by_id("graphshell-canvas")
}

/// A graph-local point in client px.
fn client_point(shared: &Shared, canvas: &Element, at: (f32, f32)) -> Option<(i32, i32)> {
    let (left, top, _, _) = shared.gestures.borrow().graph?;
    let zoom = shared
        .gestures
        .borrow()
        .ui_zoom
        .as_ref()
        .map_or(1.0, |zoom| zoom());
    let rect = canvas.get_bounding_client_rect();
    Some((
        (rect.x() + f64::from((left + at.0) * zoom)).round() as i32,
        (rect.y() + f64::from((top + at.1) * zoom)).round() as i32,
    ))
}

fn note(shared: &Shared, text: String) {
    shared.gestures.borrow_mut().probe = text;
}

/// Run `step` from the event loop.
fn later(step: impl FnOnce() + 'static) -> Result<(), String> {
    let step = Closure::once_into_js(step);
    web_sys::window()
        .ok_or("no window")?
        .set_timeout_with_callback_and_timeout_and_arguments_0(step.unchecked_ref(), 0)
        .map(|_| ())
        .map_err(|_| "could not queue the gesture".into())
}

fn pointer(canvas: &Element, kind: &str, id: i32, (x, y): (i32, i32)) -> Option<bool> {
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_pointer_id(id);
    init.set_pointer_type("touch");
    init.set_is_primary(id == FIRST_FINGER);
    init.set_client_x(x);
    init.set_client_y(y);
    let event = web_sys::PointerEvent::new_with_event_init_dict(kind, &init).ok()?;
    canvas.dispatch_event(&event).ok()?;
    Some(event.cancel_bubble())
}

/// Dispatch a touch event with `fingers` down; whether its default was
/// prevented.
fn touch(canvas: &Element, kind: &str, fingers: &[(i32, (i32, i32))]) -> Option<bool> {
    let touches = js_sys::Array::new();
    for &(id, (x, y)) in fingers {
        let init = TouchInit::new(id, canvas);
        init.set_client_x(x);
        init.set_client_y(y);
        let touch: JsValue = Touch::new(&init).ok()?.into();
        touches.push(&touch);
    }
    let init = TouchEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_touches(&touches);
    let event = TouchEvent::new_with_event_init_dict(kind, &init).ok()?;
    Some(!canvas.dispatch_event(&event).ok()?)
}

/// One finger's tap at graph-local `at`: down, then up.
fn tap(shared: &Shared, at: (f32, f32)) -> Option<String> {
    let canvas = canvas()?;
    let point = client_point(shared, &canvas, at)?;
    let withheld = pointer(&canvas, "pointerdown", FIRST_FINGER, point)?;
    let prevented = touch(&canvas, "touchstart", &[(FIRST_FINGER, point)])?;
    touch(&canvas, "touchend", &[])?;
    pointer(&canvas, "pointerup", FIRST_FINGER, point)?;
    Some(format!("touch withheld {withheld} prevented {prevented}"))
}

/// Two fingers 80 px apart about the graph's centre, spread by `scale` and
/// moved by `(dx, dy)` over four moves.
fn pinch(shared: &Shared, scale: f32, (dx, dy): (f32, f32)) -> Option<String> {
    let canvas = canvas()?;
    let (width, height) = shared.size.get();
    let centre = (width as f32 / 2.0, height as f32 / 2.0);
    let fingers = |t: f32| {
        let half = 40.0 * (1.0 + (scale - 1.0) * t);
        let (x, y) = (centre.0 + dx * t, centre.1 + dy * t);
        Some([
            (FIRST_FINGER, client_point(shared, &canvas, (x - half, y))?),
            (SECOND_FINGER, client_point(shared, &canvas, (x + half, y))?),
        ])
    };
    let start = fingers(0.0)?;
    for (id, point) in start {
        pointer(&canvas, "pointerdown", id, point)?;
    }
    let prevented = touch(&canvas, "touchstart", &start)?;
    let mut last = start;
    for step in 1..=4 {
        last = fingers(step as f32 / 4.0)?;
        touch(&canvas, "touchmove", &last)?;
    }
    touch(&canvas, "touchend", &[])?;
    for (id, point) in last {
        pointer(&canvas, "pointerup", id, point)?;
    }
    Some(format!("pinch prevented {prevented}"))
}

/// One wheel notch of `dy` at the graph's centre.
fn wheel(shared: &Shared, dy: f64, ctrl: bool, meta: bool) -> Option<String> {
    let canvas = canvas()?;
    let (width, height) = shared.size.get();
    let (x, y) = client_point(shared, &canvas, (width as f32 / 2.0, height as f32 / 2.0))?;
    let init = WheelEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_client_x(x);
    init.set_client_y(y);
    init.set_delta_y(dy);
    init.set_ctrl_key(ctrl);
    init.set_meta_key(meta);
    let event = WheelEvent::new_with_event_init_dict("wheel", &init).ok()?;
    let kept = canvas.dispatch_event(&event).ok()?;
    Some(format!("wheel kept {kept}"))
}

fn numbers(rest: &str, want: usize, usage: &str) -> Result<Vec<f32>, String> {
    let values: Vec<f32> = rest
        .split_whitespace()
        .take(want)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| usage.to_string())?;
    if values.len() == want {
        Ok(values)
    } else {
        Err(usage.to_string())
    }
}

/// The gesture verbs, or `None` for any other.
///
/// - `wheel-graph <dy> [ctrl|meta]`: a wheel notch at the graph's centre;
/// - `touch-graph <x> <y>`: a one-finger tap at a graph-local point;
/// - `touch-node <url>`: the same tap on a node;
/// - `pinch <scale> [<dx> <dy>]`: two fingers spread and moved.
pub(super) fn step(shared: &Rc<Shared>, verb: &str, rest: &str) -> Option<Result<(), String>> {
    let rest = rest.trim();
    let gesture: Gesture = match verb {
        "wheel-graph" => {
            let mut words = rest.split_whitespace();
            let dy = match words.next().map(str::parse::<f64>) {
                Some(Ok(dy)) => dy,
                _ => return Some(Err("wheel-graph wants '<dy> [ctrl|meta]'".into())),
            };
            let modifier = words.next().unwrap_or("");
            let (ctrl, meta) = (modifier == "ctrl", modifier == "meta");
            Box::new(move |shared| wheel(shared, dy, ctrl, meta))
        },
        "touch-graph" => match numbers(rest, 2, "touch-graph wants '<x> <y>'") {
            Ok(at) => Box::new(move |shared| tap(shared, (at[0], at[1]))),
            Err(error) => return Some(Err(error)),
        },
        "touch-node" => {
            let canvas = shared.canvas.borrow();
            let at = canvas
                .graph()
                .get_node_by_url(rest)
                .and_then(|(key, _)| canvas.screen_position_of(key));
            match at {
                Some(at) => Box::new(move |shared| tap(shared, at)),
                None => return Some(Err(format!("touch-node {rest}: not on screen"))),
            }
        },
        "pinch" => {
            let words = rest.split_whitespace().count();
            let usage = "pinch wants '<scale> [<dx> <dy>]'";
            match numbers(rest, if words >= 3 { 3 } else { 1 }, usage) {
                Ok(values) => {
                    let shift = (
                        values.get(1).copied().unwrap_or(0.0),
                        values.get(2).copied().unwrap_or(0.0),
                    );
                    Box::new(move |shared| pinch(shared, values[0], shift))
                },
                Err(error) => return Some(Err(error)),
            }
        },
        _ => return None,
    };
    note(shared, "pending".into());
    let shared = shared.clone();
    Some(later(move || {
        let text = gesture(&shared).unwrap_or_else(|| "failed".into());
        note(&shared, text);
    }))
}
