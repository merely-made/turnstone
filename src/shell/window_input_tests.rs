// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A surface the host spawned and routed itself, held in the same content as
//! a document: one command API reaches its web plane, and the host keeps the
//! producer for the work the content does not wrap.

use super::*;
use crate::surface::{Rect, SurfaceId, SurfaceKind};
use std::sync::{Arc, Mutex};

use inker::routing::{EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId};
use inker::{
    Cookie, FocusReason, KeyboardEvent, MouseEvent, SessionNavigationCommand, SessionSpawnRequest,
    SurfaceError, SurfaceFrame, SurfaceProducer, SurfaceSettings, WebSurface, WebSurfaceEvent,
};
use pelt_core::{PeltContent, PeltLane, PeltRoute, PeltRouteSource, PeltRouteState};

type Calls = Arc<Mutex<Vec<String>>>;

/// A web page that records every control it receives and reports one title.
struct Page {
    calls: Calls,
    pointers: Arc<Mutex<Vec<inker::PointerEvent>>>,
    acquisitions: Arc<std::sync::atomic::AtomicUsize>,
    title: Option<String>,
    fail_offset: Option<(i32, i32)>,
}

impl Page {
    fn record(&self, call: impl Into<String>) -> Result<(), SurfaceError> {
        self.calls.lock().unwrap().push(call.into());
        Ok(())
    }
}

impl SurfaceProducer for Page {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.record(format!("resize {width}x{height}"))
    }

    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        if self.fail_offset == Some((x, y)) {
            return Err(SurfaceError::InputFailed("offset rejected".into()));
        }
        Ok(())
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.acquisitions
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(None)
    }

    fn send_mouse_input(&mut self, _event: MouseEvent) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn send_pointer_input(&mut self, event: inker::PointerEvent) -> Result<(), SurfaceError> {
        self.pointers.lock().unwrap().push(event.clone());
        self.record(format!(
            "pointer {:?} {:?} ctrl={} at={},{}",
            event.phase, event.button, event.modifiers.ctrl, event.position.x, event.position.y
        ))
    }

    fn send_drag_input(&mut self, event: inker::DragEvent) -> Result<(), SurfaceError> {
        self.record(format!("drag {event:?}"))
    }

    fn send_wheel_input(&mut self, event: inker::SurfaceWheelEvent) -> Result<(), SurfaceError> {
        self.record(format!("wheel {event:?}"))
    }

    fn send_keyboard_input(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        self.record(format!("keyboard {event:?}"))
    }

    fn move_focus(&mut self, _reason: FocusReason) -> Result<(), SurfaceError> {
        self.record("focus")
    }

    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }

    fn apply_settings(&mut self, _settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn as_web_surface(&mut self) -> Option<&mut dyn WebSurface> {
        Some(self)
    }
}

impl WebSurface for Page {
    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.record(format!("navigate {url}"))
    }

    fn navigate_to_string(&mut self, _html: &str) -> Result<(), SurfaceError> {
        self.record("navigate string")
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.record("reload")
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.record("stop")
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.record("back")
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.record("forward")
    }

    fn can_go_back(&self) -> bool {
        true
    }

    fn can_go_forward(&self) -> bool {
        true
    }

    fn set_cookie(&mut self, _cookie: &Cookie) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.title
            .take()
            .map(|title| WebSurfaceEvent::TitleChanged { title })
    }
}

fn route() -> PeltRoute {
    PeltRoute {
        decision: EngineRouteDecision {
            engine_id: "fake.web".to_owned(),
            surface_contract: SurfaceContract {
                target: SurfaceTargetId::new("node-7"),
                mode: SurfaceContractMode::CompositedTexture,
            },
        },
        source: PeltRouteSource::Automatic,
        state: PeltRouteState::Surface,
    }
}

fn hosted(calls: &Calls) -> PeltContent<netrender::Scene> {
    PeltContent::from_surface(
        Box::new(Page {
            calls: calls.clone(),
            pointers: Default::default(),
            acquisitions: Default::default(),
            fail_offset: None,
            title: Some("Site".to_owned()),
        }),
        (640, 480),
        route(),
    )
}

fn key(n: u128) -> ContentKey {
    ContentKey::Node(uuid::Uuid::from_u128(n))
}
fn appearance(n: u64, x: f32) -> Surface {
    Surface {
        id: SurfaceId(n),
        kind: SurfaceKind::Content(uuid::Uuid::from_u128(7)),
        rect: Rect::new(x, 0., 100., 100.),
    }
}
fn placed(
    content: ContentKey,
    window: u64,
    x: f32,
    focused: bool,
) -> pelt_core::PeltPlacement<ContentKey> {
    pelt_core::PeltPlacement {
        content,
        window,
        rect: pelt_core::WorkspaceRect::new(x, 0., 100., 100.),
        visible: true,
        focused,
        scale_factor: 1.,
    }
}
fn touch_pool() -> (Pool, Arc<Mutex<Vec<inker::PointerEvent>>>, Vec<Surface>) {
    let pointers = Arc::new(Mutex::new(Vec::new()));
    let mut pool = Pool::new();
    pool.insert(
        key(7),
        PeltContent::from_surface(
            Box::new(Page {
                calls: Default::default(),
                pointers: pointers.clone(),
                acquisitions: Default::default(),
                title: None,
                fail_offset: None,
            }),
            (100, 100),
            route(),
        ),
    );
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(7), 0, 200., false)),
    ]);
    (
        pool,
        pointers,
        vec![appearance(10, 0.), appearance(11, 200.)],
    )
}
fn touch(
    adapter: &mut TouchInput,
    pool: &mut Pool,
    plan: &[Surface],
    id: u64,
    phase: PointerPhase,
    x: f32,
) {
    adapter.deliver(
        pool,
        plan,
        FocusTarget::Graph(crate::panes::PaneId(99)),
        0,
        id,
        phase,
        PhysicalPosition { x, y: 9. },
        Some(0.6),
        Some(0.7),
        KeyboardModifiers::default(),
    );
}
#[test]
fn review_input_touch_second_appearance_coordinates_and_sensors() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 207.);
    let events = events.lock().unwrap();
    let event = events.last().unwrap();
    assert_eq!((event.position.x, event.position.y), (7., 9.));
    assert_eq!(
        (event.pressure, event.altitude_angle),
        (Some(0.6), Some(0.7))
    );
    assert_eq!(
        pool.captured_placement(event.pointer_id),
        Some(&SurfaceId(11))
    );
}
#[test]
fn review_input_touch_distinct_ids_and_independent_terminals() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Down, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Move, 9.);
    let e = events.lock().unwrap();
    assert_ne!(e[0].pointer_id, e[1].pointer_id);
    assert_eq!(e[1].pointer_id, e[3].pointer_id);
    assert_eq!(
        pool.captured_placement(e[3].pointer_id),
        Some(&SurfaceId(10))
    );
}
#[test]
fn review_input_touch_hide_reopen_retires_old_contact() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    pool.set_placements([]);
    pool.set_placements([(SurfaceId(10), placed(key(7), 0, 0., false))]);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Move, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 8.);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        [PointerPhase::Down, PointerPhase::Cancel]
    );
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    let e = events.lock().unwrap();
    assert_ne!(e[0].pointer_id, e.last().unwrap().pointer_id);
}
#[test]
fn review_input_touch_provider_replacement_does_not_receive_old_up() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    let replacement = Calls::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    pool.insert(key(7), hosted(&replacement));
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Move, 8.);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 8.);
    assert!(
        !replacement
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("pointer"))
    );
    assert_eq!(
        events.lock().unwrap().last().unwrap().phase,
        PointerPhase::Cancel
    );
}
#[test]
fn review_input_touch_focus_loss_cancels_every_contact() {
    let (mut pool, events, plan) = touch_pool();
    let mut input = TouchInput::default();
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Down, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Down, 8.);
    input.focus_lost(&mut pool, 0);
    touch(&mut input, &mut pool, &plan, 80, PointerPhase::Up, 7.);
    touch(&mut input, &mut pool, &plan, 81, PointerPhase::Up, 8.);
    let e = events.lock().unwrap();
    assert_eq!(
        e.iter().filter(|e| e.phase == PointerPhase::Cancel).count(),
        2
    );
    assert!(!e.iter().any(|e| e.phase == PointerPhase::Up));
}
fn window_keyboard_case(event: PeltInput) {
    let mut pool = Pool::new();
    let primary = Calls::default();
    let lens = Calls::default();
    pool.insert(key(7), hosted(&primary));
    pool.insert(key(8), hosted(&lens));
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(8), 1, 0., true)),
    ]);
    primary.lock().unwrap().clear();
    lens.lock().unwrap().clear();
    focused_input(&mut pool, 1, SurfaceId(10), event);
    assert!(
        !primary
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.starts_with("keyboard")),
        "titlebar focus must not mutate other window"
    );
    assert_eq!(
        lens.lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("keyboard"))
            .count(),
        1,
        "originating focused lens receives once"
    );
}
fn keyboard(code: u32, text: Option<&str>) -> PeltInput {
    PeltInput::Keyboard(KeyboardEvent {
        key_code: code,
        scan_code: 0,
        modifiers: Default::default(),
        pressed: true,
        text: text.map(str::to_owned),
    })
}
#[test]
fn review_input_source_window_text() {
    window_keyboard_case(keyboard(0, Some("text")));
}
#[test]
fn review_input_source_window_enter() {
    window_keyboard_case(keyboard(13, None));
}
#[test]
fn review_input_source_window_ime() {
    window_keyboard_case(PeltInput::Session(inker::SessionInput::Ime(
        inker::SessionIme::Commit("composed".into()),
    )));
}
#[test]
fn review_input_lens_auxiliary_and_mouse_capture_id() {
    for button in [inker::MouseButton::Middle, inker::MouseButton::Right] {
        assert!(
            lens_button(button),
            "lens content accepts auxiliary buttons"
        );
    }
}
#[test]
fn review_input_mouse_capture_uses_actual_pointer_id() {
    let (mut pool, _, _) = touch_pool();
    let event = super::super::input::mouse_pointer_event(
        7.,
        9.,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        PointerButtons::AUXILIARY,
        Default::default(),
    );
    pool.input_at(0, PeltInput::Pointer(event));
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
}
#[test]
fn review_input_window_button_masks_and_unrelated_release() {
    let middle = Some(inker::MouseButton::Middle);
    for window in [0, 1] {
        let down = mouse_buttons(window, PointerButtons::NONE, PointerPhase::Down, middle);
        assert_eq!(down, PointerButtons::AUXILIARY);
        let unrelated = mouse_buttons(
            window,
            down,
            PointerPhase::Up,
            Some(inker::MouseButton::Left),
        );
        assert_eq!(unrelated, down);
        assert_eq!(
            mouse_buttons(window, unrelated, PointerPhase::Up, middle),
            PointerButtons::NONE
        );
    }
}
#[test]
fn review_input_lens_control_wheel_matches_primary_zoom_policy() {
    assert!(wheel_zoom(0, true, 1., true));
    assert!(wheel_zoom(1, true, 1., true));
    assert!(!wheel_zoom(1, true, 1., false));
}
#[test]
fn review_input_plain_wheel_does_not_zoom() {
    for window in [0, 1] {
        assert!(!wheel_zoom(window, false, 1., true));
        assert!(!wheel_zoom(window, true, 0., true));
    }
}
#[test]
fn review_input_click_slop_parser_is_configurable_and_finite() {
    assert_eq!(click_slop(None), 4.);
    assert_eq!(click_slop(Some("8.5")), 8.5);
    assert_eq!(click_slop(Some("0")), 0.);
    for value in ["NaN", "inf", "-1", "bad"] {
        assert_eq!(click_slop(Some(value)), 4.);
    }
}
