// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's gestures over the graph (mer3ly site canvas plan, P2 parity),
//! and its motion preference.
//!
//! The Cambium web host claims every wheel notch and pointer on its canvas,
//! which would trap an embedding page's scroll. These listeners sit on the
//! mount root in the capture phase, ahead of the host's, and keep from it
//! what belongs to the page, by `graphshell::canvas_gestures`' policy: a
//! plain wheel over the graph scrolls the page and a Ctrl or Meta wheel
//! zooms; one finger on empty canvas scrolls the page and one on a node
//! drags it; two fingers pan and pinch. Outside the graph leaf (the toolbar,
//! the tools, the lists) the host handles everything, as before.
use std::collections::HashSet;

use cambium_genet_web_host::{WHEEL_LINE_PX, WHEEL_PAGE_PX, wheel_delta_from_dom};
use graphshell::canvas_gestures::{
    Fingers, TouchUse, WheelUse, touch_use, two_finger, wheel_use, wheel_zoom,
};
use web_sys::{AddEventListenerOptions, PointerEvent, TouchEvent, TouchList};

use super::*;

/// A page-px rectangle: left, top, width, height.
pub(super) type Rect = (f32, f32, f32, f32);

/// What the gestures share with the page.
#[derive(Default)]
pub(super) struct Gestures {
    /// The graph leaf's painted rect, from the last frame.
    pub(super) graph: Option<Rect>,
    /// What sits over the leaf and keeps the host's input: the tools
    /// overlay, the product panel.
    pub(super) overlays: Vec<Rect>,
    /// Touch pointers down on the graph.
    touches: Vec<i32>,
    /// Pointers the host never hears of.
    withheld: HashSet<i32>,
    /// The one touch the host drags with.
    host_touch: Option<i32>,
    /// The two fingers' last points, while they pan and pinch.
    two: Option<Fingers>,
    /// The last wheel over the graph and the last touch, for receipts.
    pub(super) last_wheel: Option<WheelUse>,
    pub(super) last_touch: Option<TouchUse>,
    /// What the last scenario gesture found (`gesture_steps`).
    pub(super) probe: String,
    /// The host's UI zoom, once mounted.
    pub(super) ui_zoom: Option<Rc<dyn Fn() -> f32>>,
}

/// What the gestures need of the mounted host.
pub(super) struct HostHandle {
    /// The host's UI zoom, which page px are divided by.
    pub(super) ui_zoom: Rc<dyn Fn() -> f32>,
    /// End the host's press, as a pointer release would.
    pub(super) release: Box<dyn Fn()>,
    pub(super) window: WebWindow,
}

/// Whether the page asks for reduced motion: the root's
/// `data-reduced-motion` when it says, else the browser's
/// `prefers-reduced-motion`.
pub(super) fn reduced_motion(root: &Element) -> bool {
    match root.get_attribute("data-reduced-motion").as_deref() {
        Some("true") => return true,
        Some("false") => return false,
        _ => {},
    }
    web_sys::window()
        .and_then(|window| window.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .is_some_and(|query| query.matches())
}

/// The graph leaf's rect and the overlays over it, from a laid-out frame.
pub(super) fn layout_rects(ctx: &AppCtx<'_, TreePage, Logic, Child>) -> (Option<Rect>, Vec<Rect>) {
    let first = |class: &str| {
        let dom = ctx.runner.dom();
        let dom = dom.borrow();
        taproot::matching(&dom, &Selector::class(class))
            .into_iter()
            .next()
    };
    let graph = first("tree-canvas").and_then(|leaf| ctx.painted_rect(leaf));
    let overlays = ["tools-overlay", "tree-product"]
        .into_iter()
        .filter_map(|class| first(class).and_then(|node| ctx.painted_rect(node)))
        .collect();
    (graph, overlays)
}

fn inside((left, top, width, height): Rect, (x, y): (f32, f32)) -> bool {
    x >= left && y >= top && x < left + width && y < top + height
}

struct Listeners {
    shared: Rc<Shared>,
    canvas: HtmlCanvasElement,
    host: HostHandle,
}

impl Listeners {
    /// A client point as graph-local px, when it is on the graph and not
    /// under an overlay.
    fn graph_point(&self, client_x: i32, client_y: i32) -> Option<(f32, f32)> {
        let rect = self.canvas.get_bounding_client_rect();
        let zoom = (self.host.ui_zoom)();
        let at = (
            (client_x as f64 - rect.x()) as f32 / zoom,
            (client_y as f64 - rect.y()) as f32 / zoom,
        );
        let gestures = self.shared.gestures.borrow();
        let graph = gestures.graph?;
        if !inside(graph, at) || gestures.overlays.iter().any(|rect| inside(*rect, at)) {
            return None;
        }
        Some((at.0 - graph.0, at.1 - graph.1))
    }

    fn redraw(&self) {
        self.shared.dirty.set(true);
        self.host.window.request_redraw();
    }

    fn on_node(&self, at: (f32, f32)) -> bool {
        self.shared
            .canvas
            .borrow()
            .node_at_screen(at.0, at.1)
            .is_some()
    }

    /// The first two touches as graph-local points, when both are on it.
    fn fingers(&self, touches: &TouchList) -> Option<Fingers> {
        let point = |index| {
            let touch = touches.get(index)?;
            self.graph_point(touch.client_x(), touch.client_y())
        };
        Some([point(0)?, point(1)?])
    }

    fn wheel(&self, event: web_sys::WheelEvent) {
        let Some(at) = self.graph_point(event.client_x(), event.client_y()) else {
            return;
        };
        let wheel = wheel_use(event.ctrl_key(), event.meta_key());
        self.shared.gestures.borrow_mut().last_wheel = Some(wheel);
        // The host never hears of it: unclaimed, the page scrolls.
        event.stop_propagation();
        if wheel == WheelUse::Page {
            return;
        }
        event.prevent_default();
        if self.shared.selection_locked.get() {
            return;
        }
        let (_, dy) = wheel_delta_from_dom(&event, WHEEL_LINE_PX, WHEEL_PAGE_PX);
        wheel_zoom(&mut self.shared.canvas.borrow_mut(), at, dy);
        self.redraw();
    }

    fn pointer_down(&self, event: PointerEvent) {
        if event.pointer_type() != "touch" {
            return;
        }
        let Some(at) = self.graph_point(event.client_x(), event.client_y()) else {
            return;
        };
        let id = event.pointer_id();
        let on_node = self.on_node(at);
        let mut gestures = self.shared.gestures.borrow_mut();
        gestures.touches.push(id);
        let touch = touch_use(gestures.touches.len(), on_node);
        if touch == TouchUse::Drag && gestures.two.is_none() {
            gestures.host_touch = Some(id);
        } else {
            gestures.withheld.insert(id);
            event.stop_propagation();
        }
    }

    fn pointer_other(&self, event: PointerEvent, ends: bool) {
        let id = event.pointer_id();
        let release = {
            let mut gestures = self.shared.gestures.borrow_mut();
            if gestures.withheld.contains(&id) {
                event.stop_propagation();
            }
            if !ends {
                return;
            }
            gestures.withheld.remove(&id);
            gestures.touches.retain(|touch| *touch != id);
            let hosts = gestures.host_touch == Some(id);
            if hosts {
                gestures.host_touch = None;
            }
            // The host has no cancel: a cancelled drag is released.
            hosts && event.type_() == "pointercancel"
        };
        if release {
            (self.host.release)();
            self.redraw();
        }
    }

    fn touch_start(&self, event: TouchEvent) {
        let touches = event.touches();
        if touches.length() >= 2 {
            let Some(fingers) = self.fingers(&touches) else {
                return;
            };
            event.prevent_default();
            let host_touch = {
                let mut gestures = self.shared.gestures.borrow_mut();
                gestures.two = Some(fingers);
                gestures.last_touch = Some(TouchUse::TwoFinger);
                let touches = gestures.touches.clone();
                gestures.withheld.extend(touches);
                gestures.host_touch.take()
            };
            // A drag the first finger began ends where it is.
            if host_touch.is_some() {
                (self.host.release)();
                self.redraw();
            }
            return;
        }
        let Some(touch) = touches.get(0) else {
            return;
        };
        let Some(at) = self.graph_point(touch.client_x(), touch.client_y()) else {
            return;
        };
        let touch = touch_use(1, self.on_node(at));
        self.shared.gestures.borrow_mut().last_touch = Some(touch);
        // A finger on a node drags it, so the page must not scroll.
        if touch == TouchUse::Drag {
            event.prevent_default();
        }
    }

    fn touch_move(&self, event: TouchEvent) {
        let Some(from) = self.shared.gestures.borrow().two else {
            return;
        };
        if event.cancelable() {
            event.prevent_default();
        }
        let Some(to) = self.fingers(&event.touches()) else {
            return;
        };
        two_finger(&mut self.shared.canvas.borrow_mut(), from, to);
        self.shared.gestures.borrow_mut().two = Some(to);
        self.redraw();
    }

    fn touch_end(&self, event: TouchEvent) {
        if event.touches().length() < 2 {
            self.shared.gestures.borrow_mut().two = None;
        }
    }
}

/// Listen on `root`, ahead of the host's listeners on `canvas`. The canvas's
/// `touch-action` (set where it is made) lets one finger scroll the page and
/// leaves pinching to these listeners.
pub(super) fn install(
    root: &Element,
    canvas: HtmlCanvasElement,
    shared: Rc<Shared>,
    host: HostHandle,
) -> Result<(), String> {
    shared.gestures.borrow_mut().ui_zoom = Some(host.ui_zoom.clone());
    let listeners = Rc::new(Listeners {
        shared,
        canvas,
        host,
    });
    let options = AddEventListenerOptions::new();
    options.set_capture(true);
    options.set_passive(false);
    macro_rules! listen {
        ($name:literal, $ty:ty, $handler:expr) => {{
            let listeners = listeners.clone();
            let handler: fn(&Listeners, $ty) = $handler;
            let closure =
                Closure::<dyn FnMut($ty)>::new(move |event: $ty| handler(&listeners, event));
            root.add_event_listener_with_callback_and_add_event_listener_options(
                $name,
                closure.as_ref().unchecked_ref(),
                &options,
            )
            .map_err(|_| concat!("could not listen for ", $name).to_string())?;
            // The page lives as long as its mount, as the host's do.
            closure.forget();
        }};
    }
    listen!("wheel", web_sys::WheelEvent, |l, e| l.wheel(e));
    listen!("pointerdown", PointerEvent, |l, e| l.pointer_down(e));
    listen!("pointermove", PointerEvent, |l, e| l
        .pointer_other(e, false));
    listen!("pointerup", PointerEvent, |l, e| l.pointer_other(e, true));
    listen!("pointercancel", PointerEvent, |l, e| l
        .pointer_other(e, true));
    listen!("touchstart", TouchEvent, |l, e| l.touch_start(e));
    listen!("touchmove", TouchEvent, |l, e| l.touch_move(e));
    listen!("touchend", TouchEvent, |l, e| l.touch_end(e));
    listen!("touchcancel", TouchEvent, |l, e| l.touch_end(e));
    Ok(())
}
