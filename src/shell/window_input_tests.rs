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

fn native_mouse(
    pool: &mut Pool,
    buttons: &mut HashMap<u64, PointerButtons>,
    window: u64,
    appearance: Option<SurfaceId>,
    phase: PointerPhase,
    button: Option<inker::MouseButton>,
) {
    if let Some((_, effect)) = native_mouse_input(
        pool,
        buttons,
        window,
        appearance,
        PhysicalPosition { x: 7., y: 9. },
        phase,
        button,
        Default::default(),
    ) {
        assert!(effect.navigation.is_none());
        assert!(effect.submission.is_none());
    }
}

fn cancelled_native_mouse_case(window: u64, replace: bool) {
    let (mut pool, old_events, _) = touch_pool();
    pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
    let mut buttons = HashMap::new();
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        None,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
    );
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
    );
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    assert_eq!(
        old_events.lock().unwrap()[0].buttons,
        PointerButtons::AUXILIARY
    );
    let new_events = if replace {
        let replacement = Arc::new(Mutex::new(Vec::new()));
        pool.insert(
            key(7),
            PeltContent::from_surface(
                Box::new(Page {
                    calls: Default::default(),
                    pointers: replacement.clone(),
                    acquisitions: Default::default(),
                    title: None,
                    fail_offset: None,
                }),
                (100, 100),
                route(),
            ),
        );
        replacement
    } else {
        pool.set_placements([]);
        pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
        old_events.clone()
    };
    assert_eq!(captured_mouse(&pool), None);
    assert_eq!(
        buttons[&window],
        PointerButtons::AUXILIARY,
        "pool Cancel is not a physical Middle release"
    );
    assert_eq!(
        old_events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        [PointerPhase::Down, PointerPhase::Cancel]
    );
    let appearance = captured_mouse(&pool);
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        appearance,
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
    );
    assert_eq!(
        old_events.lock().unwrap().len(),
        2,
        "retired gesture cannot receive late Up or activate"
    );
    if replace {
        assert!(
            new_events.lock().unwrap().is_empty(),
            "replacement cannot receive old terminal event"
        );
    }
    let start = new_events.lock().unwrap().len();
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        None,
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
    );
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
    );
    let appearance = captured_mouse(&pool);
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        appearance,
        PointerPhase::Move,
        None,
    );
    let appearance = captured_mouse(&pool);
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        appearance,
        PointerPhase::Up,
        Some(inker::MouseButton::Left),
    );
    let events = new_events.lock().unwrap();
    assert_eq!(
        events[start..]
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        [
            (
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                PointerButtons::PRIMARY
            ),
            (PointerPhase::Move, None, PointerButtons::PRIMARY),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                PointerButtons::NONE
            ),
        ],
        "native Middle Up without capture must clear the physical mask before a new Left gesture"
    );
    assert_eq!(buttons[&window], PointerButtons::NONE);
}

#[test]
fn review_mouse_boundary_primary_hide_then_physical_release() {
    cancelled_native_mouse_case(0, false);
}
#[test]
fn review_mouse_boundary_lens_hide_then_physical_release() {
    cancelled_native_mouse_case(1, false);
}
#[test]
fn review_mouse_boundary_primary_replacement_then_physical_release() {
    cancelled_native_mouse_case(0, true);
}
#[test]
fn review_mouse_boundary_lens_replacement_then_physical_release() {
    cancelled_native_mouse_case(1, true);
}

#[test]
fn review_mouse_boundary_unrelated_release_keeps_middle_custody() {
    for window in [0, 1] {
        let (mut pool, events, _) = touch_pool();
        pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
        let mut buttons = HashMap::new();
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            Some(SurfaceId(10)),
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
        );
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Up,
            Some(inker::MouseButton::Left),
        );
        assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
        assert_eq!(buttons[&window], PointerButtons::AUXILIARY);
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Move,
            None,
        );
        let appearance = captured_mouse(&pool);
        native_mouse(
            &mut pool,
            &mut buttons,
            window,
            appearance,
            PointerPhase::Up,
            Some(inker::MouseButton::Middle),
        );
        assert_eq!(
            events
                .lock()
                .unwrap()
                .iter()
                .map(|e| (e.phase, e.buttons))
                .collect::<Vec<_>>(),
            [
                (PointerPhase::Down, PointerButtons::AUXILIARY),
                (PointerPhase::Move, PointerButtons::AUXILIARY),
                (PointerPhase::Up, PointerButtons::NONE),
            ]
        );
    }
}

fn bookkeeping_with_existing_mouse_capture(window: u64) {
    let (mut pool, events, _) = touch_pool();
    pool.set_placements([(SurfaceId(10), placed(key(7), window, 0., false))]);
    let mut buttons = HashMap::new();
    let position = PhysicalPosition { x: 7., y: 9. };
    assert!(
        native_mouse_input(
            &mut pool,
            &mut buttons,
            window,
            None,
            position,
            PointerPhase::Down,
            Some(inker::MouseButton::Left),
            Default::default(),
        )
        .is_none()
    );
    let (source, effect) = content_mouse_input(
        &mut pool,
        &buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(7));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    events.lock().unwrap().clear();

    // Both native windows record the physical press before deciding whether
    // the host admits content delivery. Existing capture cannot admit it early.
    let bookkeeping = native_mouse_input(
        &mut pool,
        &mut buttons,
        window,
        None,
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        Default::default(),
    );
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(buttons[&window], both);
    assert!(
        bookkeeping.is_none(),
        "bookkeeping must emit no host effect"
    );
    assert!(
        events.lock().unwrap().is_empty(),
        "bookkeeping must not reach the producer"
    );
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));

    let (source, effect) = content_mouse_input(
        &mut pool,
        &buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Middle),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(7));
    assert!(effect.handled);
    assert!(effect.navigation.is_none());
    assert!(effect.submission.is_none());
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button, e.buttons, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        [(
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
            both,
            7.,
            9.
        )],
        "the admitted second button press must reach the producer exactly once"
    );

    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Up,
        Some(inker::MouseButton::Left),
    );
    assert_eq!(buttons[&window], PointerButtons::AUXILIARY);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(10)));
    native_mouse(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
    );
    assert_eq!(buttons[&window], PointerButtons::NONE);
    assert_eq!(captured_mouse(&pool), None);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        [
            (PointerPhase::Down, Some(inker::MouseButton::Middle), both),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                PointerButtons::AUXILIARY
            ),
            (
                PointerPhase::Up,
                Some(inker::MouseButton::Middle),
                PointerButtons::NONE
            ),
        ]
    );
    let (_, late) = native_mouse_input(
        &mut pool,
        &mut buttons,
        window,
        Some(SurfaceId(10)),
        position,
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
        Default::default(),
    )
    .unwrap();
    assert!(!late.handled);
    assert!(late.navigation.is_none());
    assert!(late.submission.is_none());
    assert_eq!(
        events.lock().unwrap().len(),
        3,
        "late release cannot reach retired capture"
    );
}

#[test]
fn review_bookkeeping_primary_second_button_is_delivered_once() {
    bookkeeping_with_existing_mouse_capture(0);
}

#[test]
fn review_bookkeeping_lens_second_button_is_delivered_once() {
    bookkeeping_with_existing_mouse_capture(1);
}

fn overlap_surface_pool() -> (
    Pool,
    [Arc<Mutex<Vec<inker::PointerEvent>>>; 2],
    Vec<Surface>,
) {
    let mut pool = Pool::new();
    let events = [
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    ];
    let plan = vec![
        Surface {
            id: SurfaceId(10),
            kind: SurfaceKind::Content(uuid::Uuid::from_u128(7)),
            rect: Rect::new(0., 0., 100., 100.),
        },
        Surface {
            id: SurfaceId(11),
            kind: SurfaceKind::Content(uuid::Uuid::from_u128(8)),
            rect: Rect::new(20., 0., 100., 100.),
        },
    ];
    for (node, pointers) in [(7, &events[0]), (8, &events[1])] {
        pool.insert(
            key(node),
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
    }
    pool.set_placements([
        (SurfaceId(10), placed(key(7), 0, 0., false)),
        (SurfaceId(11), placed(key(8), 0, 20., false)),
    ]);
    (pool, events, plan)
}

#[test]
fn review_overlap_native_mouse_matches_host_topmost_and_preserves_capture() {
    let (mut pool, events, plan) = overlap_surface_pool();
    let hit = crate::surface::hit_test(
        &plan,
        FocusTarget::Content {
            node: uuid::Uuid::from_u128(8),
            appearance: SurfaceId(11),
        },
        27.,
        9.,
    )
    .unwrap();
    assert_eq!(hit.id, SurfaceId(11));
    let mut buttons = HashMap::new();
    let (source, effect) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(hit.id),
        PhysicalPosition { x: 27., y: 9. },
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(8));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(11)));
    let (source, unrelated) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(SurfaceId(11)),
        PhysicalPosition { x: 207., y: 209. },
        PointerPhase::Up,
        Some(inker::MouseButton::Right),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(8));
    assert!(!unrelated.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(11)));
    for (phase, button) in [
        (PointerPhase::Move, None),
        (PointerPhase::Up, Some(inker::MouseButton::Left)),
    ] {
        let appearance = captured_mouse(&pool);
        let (source, effect) = native_mouse_input(
            &mut pool,
            &mut buttons,
            0,
            appearance,
            PhysicalPosition { x: 207., y: 209. },
            phase,
            button,
            Default::default(),
        )
        .unwrap();
        assert_eq!(source, key(8));
        assert!(effect.handled);
    }
    assert_eq!(captured_mouse(&pool), None);
    assert!(
        events[0].lock().unwrap().is_empty(),
        "covered producer must receive no gesture"
    );
    let upper = events[1].lock().unwrap();
    assert_eq!(
        upper
            .iter()
            .map(|e| (e.phase, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, 7., 9.),
            (PointerPhase::Move, 187., 209.),
            (PointerPhase::Up, 187., 209.)
        ]
    );
}

#[test]
fn review_overlap_native_touch_matches_host_topmost_and_preserves_capture() {
    let (mut pool, events, plan) = overlap_surface_pool();
    let mut adapter = TouchInput::default();
    for (phase, x) in [
        (PointerPhase::Down, 27.),
        (PointerPhase::Move, 207.),
        (PointerPhase::Up, 207.),
    ] {
        assert!(adapter.deliver(
            &mut pool,
            &plan,
            FocusTarget::Graph(crate::panes::PaneId(99)),
            0,
            80,
            phase,
            PhysicalPosition { x, y: 9. },
            Some(0.6),
            Some(0.7),
            Default::default()
        ));
        if phase == PointerPhase::Down {
            let id = events[1].lock().unwrap().last().map(|e| e.pointer_id);
            assert!(
                id.is_some(),
                "host-selected upper producer receives touch Down"
            );
            assert_eq!(pool.captured_placement(id.unwrap()), Some(&SurfaceId(11)));
        }
    }
    assert!(events[0].lock().unwrap().is_empty());
    let upper = events[1].lock().unwrap();
    assert_eq!(
        upper
            .iter()
            .map(|e| (e.phase, e.position.x, e.position.y))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, 7., 9.),
            (PointerPhase::Move, 187., 9.),
            (PointerPhase::Up, 187., 9.)
        ]
    );
    assert!(upper.iter().all(|e| e.pointer_type == PointerType::Touch));
    assert_eq!(pool.captured_placement(upper[0].pointer_id), None);
}

struct OverlapDocument {
    inputs: Arc<Mutex<Vec<inker::SessionInput>>>,
    submit: bool,
}
impl inker::DocumentSession<netrender::Scene> for OverlapDocument {
    fn frame(&mut self, _width: u32, _height: u32) -> netrender::Scene {
        Default::default()
    }
    fn scroll_by(&mut self, _dx: f32, _dy: f32) -> bool {
        true
    }
    fn scroll_for_key(&mut self, _key: inker::SessionScrollKey) -> bool {
        true
    }
    fn click_at(&mut self, _x: f32, _y: f32) -> inker::SessionClick {
        inker::SessionClick::Miss
    }
    fn links(&self) -> Vec<inker::SessionLink> {
        Vec::new()
    }
    fn input(&mut self, input: inker::SessionInput) -> inker::SessionInputResult {
        use inker::{SessionButtonState, SessionEffect, SessionInput};
        self.inputs.lock().unwrap().push(input.clone());
        let effect = match input {
            SessionInput::PointerButton {
                state: SessionButtonState::Released,
                ..
            } if self.submit => SessionEffect::Submit(inker::SessionFormSubmission {
                action: "post".into(),
                method: inker::SessionFormMethod::Post,
                fields: vec![("producer".into(), "upper".into())],
            }),
            SessionInput::PointerButton {
                state: SessionButtonState::Released,
                ..
            } => SessionEffect::Navigate("next".into()),
            _ => SessionEffect::Handled,
        };
        inker::SessionInputResult {
            effect,
            cursor: None,
            capture: Some(true),
            editable: false,
        }
    }
    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn overlap_native_document_effect(submit: bool) {
    let (mut pool, _, plan) = overlap_surface_pool();
    let inputs = [
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    ];
    for (node, address, events) in [
        (7, "https://lower.test/index", &inputs[0]),
        (8, "https://upper.test/index", &inputs[1]),
    ] {
        let controller = pelt_core::PeltController::from_session_shared(
            Arc::new(inker::SessionRegistry::new()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            pelt_core::PeltControllerConfig::new("probe", address, (100, 100)).with_host_history(),
            super::super::ShellClock(std::time::Instant::now()),
            Box::new(OverlapDocument {
                inputs: events.clone(),
                submit,
            }),
        );
        pool.insert(key(node), PeltContent::from_controller(controller, route()));
    }
    for input in &inputs {
        input.lock().unwrap().clear();
    }
    let hit =
        crate::surface::hit_test(&plan, FocusTarget::Graph(crate::panes::PaneId(99)), 27., 9.)
            .unwrap();
    let mut buttons = HashMap::new();
    let (source, effect) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(hit.id),
        PhysicalPosition { x: 27., y: 9. },
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, key(8));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&pool), Some(SurfaceId(11)));
    // A hit supplied after the cursor moved onto the lower tile cannot replace
    // the pool's captured target or mislabel its outgoing host effect.
    let (source, effect) = native_mouse_input(
        &mut pool,
        &mut buttons,
        0,
        Some(SurfaceId(10)),
        PhysicalPosition { x: 5., y: 9. },
        PointerPhase::Up,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        source,
        key(8),
        "effect source must name the receiving captured content"
    );
    if submit {
        let submission = effect.submission.unwrap();
        assert_eq!(submission.action, "https://upper.test/post");
        assert_eq!(submission.fields, vec![("producer".into(), "upper".into())]);
        assert!(effect.navigation.is_none());
    } else {
        assert_eq!(
            effect.navigation.unwrap().request.address,
            "https://upper.test/next"
        );
        assert!(effect.submission.is_none());
    }
    assert_eq!(captured_mouse(&pool), None);
    assert!(inputs[0].lock().unwrap().is_empty());
    let delivered = inputs[1].lock().unwrap();
    assert!(matches!(
        delivered.as_slice(),
        [
            inker::SessionInput::PointerButton {
                x: 7.,
                y: 9.,
                state: inker::SessionButtonState::Pressed,
                ..
            },
            inker::SessionInput::PointerButton {
                x: -15.,
                y: 9.,
                state: inker::SessionButtonState::Released,
                ..
            }
        ]
    ));
}

#[test]
fn review_overlap_native_navigation_effect_names_actual_receiver() {
    overlap_native_document_effect(false);
}
#[test]
fn review_overlap_native_submission_effect_names_actual_receiver() {
    overlap_native_document_effect(true);
}

#[test]
fn review_overlap_native_reader_appearances_wheel_mutates_only_top_view() {
    use super::super::content_pool::{HostPlacement, NodeDocuments};
    let engines = Arc::new(super::super::standard_content_engines());
    let surface_engines = Arc::new(inker::SurfaceEngineRegistry::new());
    let epoch = std::time::Instant::now();
    let body = format!("<html><body><main><h1>Article</h1>{}</main></body></html>",
        "<p>This substantial article has enough words for independent retained Reader views and scrolling.</p>".repeat(150));
    let request = inker::SessionSpawnRequest::new("https://fixture.test/story")
        .with_body(body)
        .with_content_type("text/html")
        .with_viewport(640, 480);
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("overlap-reader-test"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: request.content_type.clone(),
        pinned_engine: Some(inker::routing::ENGINE_GENET_READER.into()),
    });
    let controller = pelt_core::PeltController::new_shared(
        engines.clone(),
        surface_engines.clone(),
        pelt_core::PeltControllerConfig::from_request(decision.engine_id.clone(), request)
            .with_host_history(),
        super::super::ShellClock(epoch),
    )
    .unwrap();
    let node = uuid::Uuid::from_u128(7);
    let mut contents = NodeDocuments::default();
    contents.insert(
        node,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    let plan = vec![
        Surface {
            id: SurfaceId(10),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(0., 0., 320., 200.),
        },
        Surface {
            id: SurfaceId(11),
            kind: SurfaceKind::Content(node),
            rect: Rect::new(20., 0., 320., 200.),
        },
    ];
    contents.sync_placements(
        plan.iter()
            .map(|surface| HostPlacement {
                surface: *surface,
                window: 0,
                focused: false,
                scale_factor: 1.,
            })
            .collect(),
        engines,
        surface_engines,
        epoch,
    );
    let keys = plan
        .iter()
        .map(|surface| contents.pool.placement(&surface.id).unwrap().content)
        .collect::<Vec<_>>();
    assert_ne!(keys[0], keys[1]);
    for surface in &plan {
        assert!(contents.pool.frame(&surface.id).is_some());
    }
    let reader = |key: &ContentKey| {
        contents
            .pool
            .content(key)
            .unwrap()
            .document()
            .unwrap()
            .session()
            .as_any_ref()
            .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
            .unwrap()
    };
    assert!(Arc::ptr_eq(
        &reader(&keys[0]).source_document(),
        &reader(&keys[1]).source_document()
    ));
    assert_eq!(
        crate::surface::hit_test(&plan, FocusTarget::Graph(crate::panes::PaneId(99)), 27., 9.)
            .unwrap()
            .id,
        SurfaceId(11)
    );
    assert!(
        contents
            .pool
            .input_at(
                0,
                PeltInput::Wheel {
                    position: PhysicalPosition { x: 27., y: 9. },
                    delta: (0., 120.),
                    modifiers: Default::default()
                }
            )
            .handled
    );
    let reader = |key: &ContentKey| {
        contents
            .pool
            .content(key)
            .unwrap()
            .document()
            .unwrap()
            .session()
            .as_any_ref()
            .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
            .unwrap()
    };
    assert_eq!(
        reader(&keys[0]).scroll_y(),
        0.,
        "covered Reader appearance retains its own state"
    );
    assert!(
        reader(&keys[1]).scroll_y() > 0.,
        "top Reader appearance receives wheel"
    );
}

struct CallerCaptureFixture {
    app: crate::app::App,
    contents: super::super::content_pool::NodeDocuments,
    window: u64,
    nodes: [uuid::Uuid; 2],
    surfaces: [Surface; 2],
    pointers: [Arc<Mutex<Vec<inker::PointerEvent>>>; 2],
    inputs: [Arc<Mutex<Vec<inker::SessionInput>>>; 2],
    buttons: HashMap<u64, PointerButtons>,
}
impl CallerCaptureFixture {
    fn new(window: u64, document: Option<bool>) -> Self {
        use crate::action::{Action, WbAxis};
        let mut app = crate::app::App::test_stub();
        app.viewport = (1200., 900.);
        let mut nodes = Vec::new();
        for address in ["https://upper.test/index", "https://lower.test/index"] {
            app.update(Action::OpenAddress(address.into()));
            nodes.push(app.graph_runtimes.focused_member().unwrap());
            app.update(Action::OpenInWorkbench);
        }
        app.update(Action::WorkbenchSplitBeside {
            dragged: nodes[1],
            target: nodes[0],
            axis: WbAxis::Row,
            after: true,
        });
        if window != 0 {
            app.update(Action::TearOutActivePane);
        }
        let nodes = [nodes[0], nodes[1]];
        let pointers = [
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        ];
        let inputs = [
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
        ];
        let mut contents = super::super::content_pool::NodeDocuments::default();
        for (index, node) in nodes.iter().enumerate() {
            let content = if let Some(submit) = document {
                let address = if index == 0 {
                    "https://upper.test/index"
                } else {
                    "https://lower.test/index"
                };
                let controller = pelt_core::PeltController::from_session_shared(
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    pelt_core::PeltControllerConfig::new("probe", address, (100, 100))
                        .with_host_history(),
                    super::super::ShellClock(std::time::Instant::now()),
                    Box::new(OverlapDocument {
                        inputs: inputs[index].clone(),
                        submit,
                    }),
                );
                PeltContent::from_controller(controller, route())
            } else {
                PeltContent::from_surface(
                    Box::new(Page {
                        calls: Default::default(),
                        pointers: pointers[index].clone(),
                        acquisitions: Default::default(),
                        title: None,
                        fail_offset: None,
                    }),
                    (100, 100),
                    route(),
                )
            };
            contents.insert(*node, content);
        }
        let plan = if window == 0 {
            super::super::primary_content_plan(&app, &contents, 1200, 900)
        } else {
            super::super::lens::lens_content_plan(&app, &contents, 0, 1200, 900)
        };
        let surfaces = nodes.map(|node| {
            *plan
                .iter()
                .find(|s| {
                    s.kind == SurfaceKind::Content(node)
                        && crate::surface::hit_test(
                            &plan,
                            app.focus,
                            s.rect.x + s.rect.w / 2.,
                            s.rect.y + s.rect.h / 2.,
                        )
                        .is_some_and(|hit| hit.id == s.id)
                })
                .expect("actual portable plan exposes both separate content appearances")
        });
        assert_ne!(surfaces[0].id, surfaces[1].id);
        assert_ne!(surfaces[0].rect, surfaces[1].rect);
        let mut fixture = Self {
            app,
            contents,
            window,
            nodes,
            surfaces,
            pointers,
            inputs,
            buttons: HashMap::new(),
        };
        fixture.sync(true);
        fixture
    }
    fn plan(&self) -> Vec<Surface> {
        if self.window == 0 {
            super::super::primary_content_plan(&self.app, &self.contents, 1200, 900)
        } else {
            super::super::lens::lens_content_plan(&self.app, &self.contents, 0, 1200, 900)
        }
    }
    fn sync(&mut self, active: bool) {
        let placements = self
            .plan()
            .into_iter()
            .map(|surface| {
                super::super::content_pool::HostPlacement::for_focus(
                    surface,
                    self.window,
                    active,
                    self.app.focus,
                    2.,
                )
            })
            .collect();
        self.contents.sync_placements(
            placements,
            Arc::new(inker::SessionRegistry::new()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            std::time::Instant::now(),
        );
    }
    fn position(&self, index: usize) -> PhysicalPosition {
        let r = self.surfaces[index].rect;
        PhysicalPosition {
            x: r.x + r.w / 2.,
            y: r.y + r.h / 2.,
        }
    }
    fn press(&mut self, index: usize, button: inker::MouseButton) -> (ContentKey, PeltHostEffect) {
        let position = self.position(index);
        assert!(
            native_mouse_input(
                &mut self.contents.pool,
                &mut self.buttons,
                self.window,
                None,
                position,
                PointerPhase::Down,
                Some(button),
                Default::default()
            )
            .is_none()
        );
        let plan = self.plan();
        let hit = crate::surface::hit_test(&plan, self.app.focus, position.x, position.y).unwrap();
        assert_eq!(hit.id, self.surfaces[index].id);
        let target = content_press_target(
            &mut self.app.focus,
            &self.contents.pool,
            self.window,
            position,
            hit,
        );
        // Primary content_input(Focus(true)) syncs first. Lens content_mouse
        // syncs directly. Both use the real shared placement construction.
        if self.window == 0 {
            self.sync(true);
            self.contents.pool.input(
                &target.id,
                PeltInput::Session(inker::SessionInput::Focus(true)),
            );
        }
        self.sync(true);
        let rect = self.contents.pool.placement(&target.id).unwrap().rect;
        content_mouse_input(
            &mut self.contents.pool,
            &self.buttons,
            self.window,
            Some(target.id),
            PhysicalPosition {
                x: target.local.0 + rect.x,
                y: target.local.1 + rect.y,
            },
            PointerPhase::Down,
            Some(button),
            Default::default(),
        )
        .unwrap()
    }
    fn pointer(
        &mut self,
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) -> (ContentKey, PeltHostEffect) {
        self.sync(true);
        let position = self.position(1);
        native_mouse_input(
            &mut self.contents.pool,
            &mut self.buttons,
            self.window,
            Some(self.surfaces[1].id),
            position,
            phase,
            button,
            Default::default(),
        )
        .unwrap()
    }
    fn assert_a_custody(&self) {
        assert_eq!(
            captured_mouse(&self.contents.pool),
            Some(self.surfaces[0].id),
            "admitted second Down must retain the first appearance capture through caller focus and sync"
        );
        assert_eq!(
            self.app.focus,
            FocusTarget::Content {
                node: self.nodes[0],
                appearance: self.surfaces[0].id
            }
        );
        assert!(
            self.contents
                .pool
                .placement(&self.surfaces[0].id)
                .unwrap()
                .focused
        );
        assert!(
            !self
                .contents
                .pool
                .placement(&self.surfaces[1].id)
                .unwrap()
                .focused
        );
    }
}
fn caller_capture_gesture(window: u64) {
    let mut f = CallerCaptureFixture::new(window, None);
    assert!(f.press(0, inker::MouseButton::Left).1.handled);
    f.assert_a_custody();
    let plan = f.plan();
    // Actual movement over B stays captured by A and leaves host focus alone.
    let position = f.position(1);
    assert!(
        native_mouse_input(
            &mut f.contents.pool,
            &mut f.buttons,
            window,
            Some(f.surfaces[0].id),
            position,
            PointerPhase::Move,
            None,
            Default::default()
        )
        .unwrap()
        .1
        .handled
    );
    assert!(f.press(1, inker::MouseButton::Middle).1.handled);
    f.assert_a_custody();
    assert_eq!(
        f.plan(),
        plan,
        "focus mutation must not be mistaken for changed appearance geometry"
    );
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(f.buttons[&window], both);
    let second = f.pointers[0].lock().unwrap().clone();
    assert_eq!(second.len(), 3);
    assert!(second.iter().all(|e| e.phase != PointerPhase::Cancel));
    assert_eq!(
        (second[2].phase, second[2].button, second[2].buttons),
        (PointerPhase::Down, Some(inker::MouseButton::Middle), both)
    );
    assert_eq!(
        (second[2].position.x, second[2].position.y),
        (
            position.x - f.surfaces[0].rect.x,
            position.y - f.surfaces[0].rect.y
        )
    );
    assert!(
        f.pointers[1].lock().unwrap().is_empty(),
        "fresh hit B receives no captured gesture"
    );
    assert!(
        !f.pointer(PointerPhase::Up, Some(inker::MouseButton::Right))
            .1
            .handled
    );
    f.assert_a_custody();
    assert!(
        f.pointer(PointerPhase::Up, Some(inker::MouseButton::Left))
            .1
            .handled
    );
    assert_eq!(f.buttons[&window], PointerButtons::AUXILIARY);
    f.assert_a_custody();
    assert!(
        f.pointer(PointerPhase::Up, Some(inker::MouseButton::Middle))
            .1
            .handled
    );
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert_eq!(f.buttons[&window], PointerButtons::NONE);
    assert_eq!(f.pointers[0].lock().unwrap().len(), 5);
    assert!(
        !f.pointer(PointerPhase::Up, Some(inker::MouseButton::Middle))
            .1
            .handled
    );
    assert_eq!(f.pointers[0].lock().unwrap().len(), 5);
    assert!(f.pointers[1].lock().unwrap().is_empty());
}
fn caller_capture_effect(window: u64, submit: bool) {
    let mut f = CallerCaptureFixture::new(window, Some(submit));
    f.press(0, inker::MouseButton::Left);
    let (source, effect) = f.press(1, inker::MouseButton::Middle);
    assert!(effect.handled);
    assert_eq!(
        source,
        ContentKey::Node(f.nodes[0]),
        "caller selects actual captured effect source A"
    );
    f.assert_a_custody();
    let (source, unrelated) = f.pointer(PointerPhase::Up, Some(inker::MouseButton::Right));
    assert_eq!(source, ContentKey::Node(f.nodes[0]));
    assert!(!unrelated.handled);
    assert!(unrelated.navigation.is_none() && unrelated.submission.is_none());
    f.assert_a_custody();
    for button in [inker::MouseButton::Left, inker::MouseButton::Middle] {
        let (source, effect) = f.pointer(PointerPhase::Up, Some(button));
        assert_eq!(source, ContentKey::Node(f.nodes[0]));
        if submit {
            assert_eq!(effect.submission.unwrap().action, "https://upper.test/post");
            assert!(effect.navigation.is_none());
        } else {
            assert_eq!(
                effect.navigation.unwrap().request.address,
                "https://upper.test/next"
            );
            assert!(effect.submission.is_none());
        }
        if button == inker::MouseButton::Left {
            f.assert_a_custody();
        }
    }
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert!(
        !f.inputs[0]
            .lock()
            .unwrap()
            .iter()
            .any(|input| matches!(input, inker::SessionInput::Cancel))
    );
    assert!(!f.inputs[1].lock().unwrap().iter().any(|input| matches!(
        input,
        inker::SessionInput::PointerButton { .. } | inker::SessionInput::PointerMoved { .. }
    )));
}
fn caller_capture_retirement(window: u64) {
    for retirement in 0..4 {
        let mut f = CallerCaptureFixture::new(window, None);
        f.press(0, inker::MouseButton::Left);
        f.assert_a_custody();
        match retirement {
            0 => f.sync(false), // Genuine native focus loss.
            1 => {
                let placements = f
                    .plan()
                    .into_iter()
                    .filter(|s| s.id != f.surfaces[0].id)
                    .map(|surface| {
                        super::super::content_pool::HostPlacement::for_focus(
                            surface,
                            window,
                            true,
                            f.app.focus,
                            1.,
                        )
                    })
                    .collect();
                f.contents.sync_placements(
                    placements,
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    std::time::Instant::now(),
                );
            },
            2 => {
                let placements = f
                    .plan()
                    .into_iter()
                    .map(|surface| {
                        let source_window = if surface.id == f.surfaces[0].id {
                            window + 1
                        } else {
                            window
                        };
                        super::super::content_pool::HostPlacement::for_focus(
                            surface,
                            source_window,
                            true,
                            f.app.focus,
                            1.,
                        )
                    })
                    .collect();
                f.contents.sync_placements(
                    placements,
                    Arc::new(inker::SessionRegistry::new()),
                    Arc::new(inker::SurfaceEngineRegistry::new()),
                    std::time::Instant::now(),
                );
            },
            _ => {
                f.contents.insert(f.nodes[0], hosted(&Default::default()));
            },
        }
        assert_eq!(
            captured_mouse(&f.contents.pool),
            None,
            "real retirement must still cancel custody"
        );
        assert_eq!(
            f.pointers[0]
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.phase)
                .collect::<Vec<_>>(),
            [PointerPhase::Down, PointerPhase::Cancel]
        );
        assert_eq!(
            f.buttons[&window],
            PointerButtons::PRIMARY,
            "Cancel is not physical release"
        );
        let position = f.position(1);
        assert!(
            native_mouse_input(
                &mut f.contents.pool,
                &mut f.buttons,
                window,
                None,
                position,
                PointerPhase::Up,
                Some(inker::MouseButton::Left),
                Default::default()
            )
            .is_none()
        );
        assert_eq!(f.buttons[&window], PointerButtons::NONE);
        assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
        let (source, effect) = f.press(1, inker::MouseButton::Middle);
        assert_eq!(source, ContentKey::Node(f.nodes[1]));
        assert!(effect.handled);
        assert_eq!(
            f.app.focus,
            FocusTarget::Content {
                node: f.nodes[1],
                appearance: f.surfaces[1].id
            }
        );
        assert_eq!(captured_mouse(&f.contents.pool), Some(f.surfaces[1].id));
        assert_eq!(f.pointers[1].lock().unwrap().len(), 1);
    }
}
#[test]
fn review_caller_capture_primary_second_down_preserves_custody() {
    caller_capture_gesture(0);
}
#[test]
fn review_caller_capture_lens_second_down_preserves_custody() {
    caller_capture_gesture(1);
}
#[test]
fn review_caller_capture_primary_navigation_names_first_receiver() {
    caller_capture_effect(0, false);
}
#[test]
fn review_caller_capture_lens_navigation_names_first_receiver() {
    caller_capture_effect(1, false);
}
#[test]
fn review_caller_capture_primary_submission_names_first_receiver() {
    caller_capture_effect(0, true);
}
#[test]
fn review_caller_capture_lens_submission_names_first_receiver() {
    caller_capture_effect(1, true);
}
#[test]
fn review_caller_capture_primary_actual_retirement_allows_new_receiver() {
    caller_capture_retirement(0);
}
#[test]
fn review_caller_capture_lens_actual_retirement_allows_new_receiver() {
    caller_capture_retirement(1);
}


/// The primary native caller below its chrome/modal/context admission gates.
/// It uses the actual App plan, NodeDocuments and Pelt pool. Only renderer and
/// OS cursor/redraw effects are absent. In particular this fixture does not
/// call content_press_target directly or assume that every hit is Content.
struct PrimaryPaneCaptureFixture {
    content: CallerCaptureFixture,
    capture: Option<SurfaceKind>,
    delivered: Vec<(ContentKey, bool)>,
    cancellation_capture: Vec<Option<SurfaceKind>>,
}
impl PrimaryPaneCaptureFixture {
    fn new() -> Self {
        Self {
            content: CallerCaptureFixture::new(0, None),
            capture: None,
            delivered: Vec::new(),
            cancellation_capture: Vec::new(),
        }
    }
    fn pane_position(&self) -> PhysicalPosition {
        let plan = self.content.plan();
        for surface in &plan {
            if !matches!(surface.kind, SurfaceKind::Pane(_)) {
                continue;
            }
            // Find an exposed ordinary Pane in the real primary plan, not a
            // synthetic rectangle placed over content solely for this test.
            for dy in [1., 8., surface.rect.h / 2., surface.rect.h - 1.] {
                for dx in [1., 8., surface.rect.w / 2., surface.rect.w - 1.] {
                    let position = PhysicalPosition {
                        x: surface.rect.x + dx,
                        y: surface.rect.y + dy,
                    };
                    if crate::surface::hit_test(
                        &plan,
                        self.content.app.focus,
                        position.x,
                        position.y,
                    )
                    .is_some_and(|hit| hit.id == surface.id)
                    {
                        return position;
                    }
                }
            }
        }
        panic!("actual primary plan must expose an ordinary pane beside content");
    }
    fn press(&mut self, position: PhysicalPosition, button: winit::event::MouseButton) {
        use super::super::input::{
            PrimaryPointerHost, dispatch_primary_press, surface_mouse_button,
        };
        // This is Shell::deliver_press's native bookkeeping before admission.
        self.pointer_window_mouse(
            None,
            (position.x, position.y),
            PointerPhase::Down,
            surface_mouse_button(button),
        );
        let hit = crate::surface::hit_test(
            &self.content.plan(),
            self.content.app.focus,
            position.x,
            position.y,
        );
        // The shared production dispatch contains the unconditional capture
        // assignment AND the Content/non-Content decision implicated by I3.
        if let Some(hit) = dispatch_primary_press(self, hit, position.x, position.y, button) {
            let SurfaceKind::Pane(pane) = hit.kind else {
                panic!("this fixture exercises only admitted Content and ordinary Pane hits");
            };
            assert_eq!(button, winit::event::MouseButton::Middle);
            // Middle has no pane renderer action. Match its App effects from
            // the continuation in Shell::deliver_press.
            self.content.app.active_pane = Some(pane);
            self.content.app.raise_floating_pane(pane);
        }
    }
    fn move_pointer(&mut self, position: PhysicalPosition) {
        // At the Pane position, Shell's non-content fallthrough has no page
        // hover receiver. Captured-content movement must go through this gate.
        super::super::input::dispatch_primary_content_move(self, position.x, position.y);
    }
    fn release(&mut self, position: PhysicalPosition, button: winit::event::MouseButton) {
        let _ = super::super::input::dispatch_primary_release(self, position.x, position.y, button);
        // Ordinary Middle-pane continuation has no contributed renderer or
        // graph/divider/workbench drag to finish in this fixture.
    }
    fn focus_lost(&mut self) {
        // Actual pool focus-loss entry used by Shell::native_input_focus.
        // Window bookkeeping matches that caller; no synthetic Up is sent.
        TouchInput::default().focus_lost(&mut self.content.contents.pool, 0);
        self.content.buttons.remove(&0);
        self.capture = None;
        self.content.sync(false);
    }
    fn record(&mut self, routed: Option<(ContentKey, PeltHostEffect)>) {
        if let Some((receiver, effect)) = routed {
            self.delivered.push((receiver, effect.handled));
        }
    }
}
impl super::super::input::PrimaryPointerHost for PrimaryPaneCaptureFixture {
    fn pointer_state(&mut self) -> (&mut FocusTarget, &mut Pool, &mut Option<SurfaceKind>) {
        (
            &mut self.content.app.focus,
            &mut self.content.contents.pool,
            &mut self.capture,
        )
    }
    fn sync_pointer_placements(&mut self) {
        self.content.sync(true);
    }
    fn pointer_content_focus(&mut self, appearance: SurfaceId) {
        self.content.sync(true);
        self.content.contents.pool.input(
            &appearance,
            PeltInput::Session(inker::SessionInput::Focus(true)),
        );
    }
    fn pointer_content_mouse(
        &mut self,
        appearance: SurfaceId,
        local: (f32, f32),
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        self.content.sync(true);
        let placement = self
            .content
            .contents
            .pool
            .placement(&appearance)
            .unwrap()
            .clone();
        let routed = content_mouse_input(
            &mut self.content.contents.pool,
            &self.content.buttons,
            placement.window,
            Some(appearance),
            PhysicalPosition {
                x: local.0 + placement.rect.x,
                y: local.1 + placement.rect.y,
            },
            phase,
            button,
            Default::default(),
        );
        self.record(routed);
    }
    fn pointer_window_mouse(
        &mut self,
        appearance: Option<SurfaceId>,
        position: (f32, f32),
        phase: PointerPhase,
        button: Option<inker::MouseButton>,
    ) {
        let routed = native_mouse_input(
            &mut self.content.contents.pool,
            &mut self.content.buttons,
            0,
            appearance,
            PhysicalPosition {
                x: position.0,
                y: position.1,
            },
            phase,
            button,
            Default::default(),
        );
        self.record(routed);
    }
    fn pointer_user_agent_decision_open(&self) -> bool {
        self.content.app.user_agent_decision.is_open()
    }
    fn pointer_has_foreign_capture(&self) -> bool {
        captured_mouse(&self.content.contents.pool)
            .and_then(|id| self.content.contents.pool.placement(&id))
            .is_some_and(|placement| placement.window != 0)
    }
    fn pointer_cancel_primary_content(&mut self) {
        self.cancellation_capture.push(self.capture);
        TouchInput::default().focus_lost(&mut self.content.contents.pool, 0);
    }
    fn pointer_has_contributed_capture(&self) -> bool {
        false
    }
    fn pointer_release_contributed(&mut self, _: f32, _: f32) {
        panic!("ordinary Pane fixture cannot hold contributed renderer custody");
    }
    fn pointer_content_hovered(&mut self, _: uuid::Uuid) {}
    fn pointer_redraw(&mut self) {}
}

fn primary_pane_chord(first_release: winit::event::MouseButton) {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        Some(f.content.surfaces[0].id)
    );
    let pane = f.pane_position();
    assert!(matches!(
        crate::surface::hit_test(&f.content.plan(), f.content.app.focus, pane.x, pane.y)
            .unwrap()
            .kind,
        SurfaceKind::Pane(_)
    ));
    f.press(pane, Middle);
    f.move_pointer(pane);
    let second_release = if first_release == Left { Middle } else { Left };
    f.release(pane, first_release);
    let after_first = captured_mouse(&f.content.contents.pool);
    f.release(pane, second_release);

    // Check the terminal outcome before intermediate routing assertions, so
    // the existing caller fault reports stranded custody rather than a count.
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        None,
        "all physical releases must retire primary pool custody after a Pane chord"
    );
    assert_eq!(
        after_first,
        Some(f.content.surfaces[0].id),
        "the other admitted button still holds A"
    );
    assert_eq!(f.capture, None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.app.focus,
        FocusTarget::Content {
            node: f.content.nodes[0],
            appearance: f.content.surfaces[0].id
        }
    );
    assert!(
        f.content.pointers[1].lock().unwrap().is_empty(),
        "B must not receive A's gesture"
    );
    assert_eq!(
        f.delivered,
        vec![(ContentKey::Node(f.content.nodes[0]), true); 5]
    );
    let events = f.content.pointers[0].lock().unwrap().clone();
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    let (first, remaining, second) = if first_release == Left {
        (
            inker::MouseButton::Left,
            PointerButtons::AUXILIARY,
            inker::MouseButton::Middle,
        )
    } else {
        (
            inker::MouseButton::Middle,
            PointerButtons::PRIMARY,
            inker::MouseButton::Left,
        )
    };
    assert_eq!(
        events
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        vec![
            (
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                PointerButtons::PRIMARY
            ),
            (PointerPhase::Down, Some(inker::MouseButton::Middle), both),
            (PointerPhase::Move, None, both),
            (PointerPhase::Up, Some(first), remaining),
            (PointerPhase::Up, Some(second), PointerButtons::NONE),
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.pointer_id == 1 && e.pointer_type == inker::PointerType::Mouse)
    );
    for event in &events[1..] {
        assert_eq!(
            (event.position.x, event.position.y),
            (
                pane.x - f.content.surfaces[0].rect.x,
                pane.y - f.content.surfaces[0].rect.y
            )
        );
    }
    f.release(pane, second_release);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().len(),
        5,
        "duplicate terminal input must not re-enter A"
    );
}

#[test]
fn review_primary_pane_chord_left_up_then_middle_up_retires_actual_receiver() {
    primary_pane_chord(winit::event::MouseButton::Left);
}
#[test]
fn review_primary_pane_chord_middle_up_then_left_up_retires_actual_receiver() {
    primary_pane_chord(winit::event::MouseButton::Middle);
}
#[test]
fn review_primary_pane_chord_genuine_focus_loss_cancels_and_allows_new_receiver() {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    let pane = f.pane_position();
    f.press(pane, Middle);
    f.focus_lost();
    f.release(pane, Left);
    f.release(pane, Middle);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    let events = f.content.pointers[0].lock().unwrap().clone();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.phase == PointerPhase::Cancel)
            .count(),
        1
    );
    assert_eq!(events.last().unwrap().phase, PointerPhase::Cancel);
    assert!(!events.iter().any(|e| e.phase == PointerPhase::Up));
    assert!(f.content.pointers[1].lock().unwrap().is_empty());

    f.content.sync(true);
    let b = f.content.position(1);
    f.press(b, Left);
    f.release(b, Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), events.len());
    assert_eq!(
        f.content.pointers[1]
            .lock()
            .unwrap()
            .iter()
            .map(|e| (e.phase, e.button))
            .collect::<Vec<_>>(),
        vec![
            (PointerPhase::Down, Some(inker::MouseButton::Left)),
            (PointerPhase::Up, Some(inker::MouseButton::Left)),
        ]
    );
    assert_eq!(
        &f.delivered[f.delivered.len() - 2..],
        &[(ContentKey::Node(f.content.nodes[1]), true); 2]
    );
}

fn primary_no_hit_chord(first_release: winit::event::MouseButton) {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        Some(f.content.surfaces[0].id)
    );
    let pane = PhysicalPosition { x: -100.0, y: -100.0 };
    assert!(
        crate::surface::hit_test(&f.content.plan(), f.content.app.focus, pane.x, pane.y)
            .is_none(),
        "this real plan position must have no fresh hit"
    );
    f.press(pane, Middle);
    f.move_pointer(pane);
    let second_release = if first_release == Left { Middle } else { Left };
    f.release(pane, first_release);
    let after_first = captured_mouse(&f.content.contents.pool);
    f.release(pane, second_release);

    // Check the terminal outcome before intermediate routing assertions, so
    // the existing caller fault reports stranded custody rather than a count.
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        None,
        "all physical releases must retire primary pool custody after a no-hit chord"
    );
    assert_eq!(
        after_first,
        Some(f.content.surfaces[0].id),
        "the other admitted button still holds A"
    );
    assert_eq!(f.capture, None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.app.focus,
        FocusTarget::Content {
            node: f.content.nodes[0],
            appearance: f.content.surfaces[0].id
        }
    );
    assert!(
        f.content.pointers[1].lock().unwrap().is_empty(),
        "B must not receive A's gesture"
    );
    assert_eq!(
        f.delivered,
        vec![(ContentKey::Node(f.content.nodes[0]), true); 5]
    );
    let events = f.content.pointers[0].lock().unwrap().clone();
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    let (first, remaining, second) = if first_release == Left {
        (
            inker::MouseButton::Left,
            PointerButtons::AUXILIARY,
            inker::MouseButton::Middle,
        )
    } else {
        (
            inker::MouseButton::Middle,
            PointerButtons::PRIMARY,
            inker::MouseButton::Left,
        )
    };
    assert_eq!(
        events
            .iter()
            .map(|e| (e.phase, e.button, e.buttons))
            .collect::<Vec<_>>(),
        vec![
            (
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                PointerButtons::PRIMARY
            ),
            (PointerPhase::Down, Some(inker::MouseButton::Middle), both),
            (PointerPhase::Move, None, both),
            (PointerPhase::Up, Some(first), remaining),
            (PointerPhase::Up, Some(second), PointerButtons::NONE),
        ]
    );
    assert!(
        events
            .iter()
            .all(|e| e.pointer_id == 1 && e.pointer_type == inker::PointerType::Mouse)
    );
    for event in &events[1..] {
        assert_eq!(
            (event.position.x, event.position.y),
            (
                pane.x - f.content.surfaces[0].rect.x,
                pane.y - f.content.surfaces[0].rect.y
            )
        );
    }
    f.release(pane, second_release);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().len(),
        5,
        "duplicate terminal input must not re-enter A"
    );
}

#[test]
fn review_primary_no_hit_chord_left_up_then_middle_up_retires_actual_receiver() {
    primary_no_hit_chord(winit::event::MouseButton::Left);
}
#[test]
fn review_primary_no_hit_chord_middle_up_then_left_up_retires_actual_receiver() {
    primary_no_hit_chord(winit::event::MouseButton::Middle);
}

impl PrimaryPaneCaptureFixture {
    fn open_primary_decision(&mut self) -> crate::user_agent_decision::UserAgentRequestKey {
        use crate::user_agent_decision::{PendingUserAgentDecision, UserAgentRequestKey};
        let request = UserAgentRequestKey::new(self.content.nodes[0], inker::UserAgentRequestId::new(71));
        assert!(self.content.app.user_agent_decision.receive(
            PendingUserAgentDecision::Permission {
                request,
                origin: "https://upper.test".into(),
                descriptors: vec![inker::PermissionDescriptor::Camera],
            }
        ));
        request
    }
    fn press_primary_outside_decision_card(&mut self) {
        use super::super::input::{PrimaryPointerHost, reject_primary_modal_press};
        // Begin after chrome.click returned no intent. Use the actual shared
        // UA predicate/return branch; card geometry is a headed-test boundary.
        self.pointer_window_mouse(
            None,
            (-100.0, -100.0),
            PointerPhase::Down,
            Some(inker::MouseButton::Middle),
        );
        assert!(reject_primary_modal_press(self));
    }
}

#[test]
fn review_primary_modal_rejection_cancels_before_shell_clear_without_later_up() {
    use winit::event::MouseButton::{Left, Middle};
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, Left);
    let admitted = f.capture;
    assert_eq!(admitted, Some(SurfaceKind::Content(f.content.nodes[0])));
    let request = f.open_primary_decision();
    f.content.assert_a_custody();
    f.press_primary_outside_decision_card();
    assert_eq!(
        f.cancellation_capture,
        vec![admitted],
        "the actual gate must cancel A while its shell capture still exists"
    );
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    let both = PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::AUXILIARY.0);
    assert_eq!(f.content.buttons[&0], both, "interception is not physical Up");
    let phases = || f.content.pointers[0].lock().unwrap().iter()
        .map(|e| e.phase).collect::<Vec<_>>();
    assert_eq!(phases(), vec![PointerPhase::Down, PointerPhase::Cancel]);
    // No redraw/sync between rejection and actual terminal input.
    f.release(a, Left);
    assert_eq!(f.content.buttons[&0], PointerButtons::AUXILIARY);
    f.release(a, Middle);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Cancel]
    );
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
    assert!(f.content.app.user_agent_decision.remove(request));
    let b = f.content.position(1);
    f.press(b, Left);
    f.release(b, Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), 2);
    assert_eq!(
        f.content.pointers[1].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
}

#[test]
fn review_primary_modal_rejection_preserves_real_lens_custody() {
    let mut f = PrimaryPaneCaptureFixture {
        content: CallerCaptureFixture::new(1, None),
        capture: None,
        delivered: Vec::new(),
        cancellation_capture: Vec::new(),
    };
    assert!(f.content.press(0, inker::MouseButton::Left).1.handled);
    f.capture = Some(SurfaceKind::Content(f.content.nodes[0]));
    let admitted = f.capture;
    let appearance = f.content.surfaces[0].id;
    assert_eq!(f.content.contents.pool.placement(&appearance).unwrap().window, 1);
    f.open_primary_decision();
    f.press_primary_outside_decision_card();
    assert_eq!(captured_mouse(&f.content.contents.pool), Some(appearance));
    assert_eq!(f.capture, admitted, "a primary modal must preserve the lens shell mirror");
    assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    assert_eq!(f.content.buttons[&0], PointerButtons::AUXILIARY);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down]
    );
    // Release the rejected primary button without entering the lens receiver.
    assert!(native_mouse_input(
        &mut f.content.contents.pool, &mut f.content.buttons, 0, None,
        PhysicalPosition { x: -100.0, y: -100.0 }, PointerPhase::Up,
        Some(inker::MouseButton::Middle), Default::default(),
    ).is_none());
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(captured_mouse(&f.content.contents.pool), Some(appearance));
    let (source, effect) = f.content.pointer(PointerPhase::Up, Some(inker::MouseButton::Left));
    assert_eq!(source, ContentKey::Node(f.content.nodes[0]));
    assert!(effect.handled);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.buttons[&1], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
}

#[test]
fn review_primary_modal_rejection_closed_gate_preserves_content_custody() {
    let mut f = PrimaryPaneCaptureFixture::new();
    let a = f.content.position(0);
    f.press(a, winit::event::MouseButton::Left);
    let admitted = f.capture;
    assert!(!super::super::input::reject_primary_modal_press(&mut f));
    assert_eq!(f.capture, admitted);
    assert!(f.cancellation_capture.is_empty());
    f.content.assert_a_custody();
    f.release(a, winit::event::MouseButton::Left);
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(
        f.content.pointers[0].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Up]
    );
}

/// An independent receiver with observable focus and Drop, used only by the
/// interception controls. The original native fixtures and tests stay intact.
struct InterceptionPage {
    page: Page,
    lifetime: Calls,
}
impl Drop for InterceptionPage {
    fn drop(&mut self) {
        self.lifetime.lock().unwrap().push("drop".into());
    }
}
impl SurfaceProducer for InterceptionPage {
    fn resize(&mut self, w: u32, h: u32) -> Result<(), SurfaceError> {
        self.page.resize(w, h)
    }
    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.page.set_offset(x, y)
    }
    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.page.acquire_frame()
    }
    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.page.send_mouse_input(ev)
    }
    fn send_pointer_input(&mut self, ev: inker::PointerEvent) -> Result<(), SurfaceError> {
        self.lifetime
            .lock()
            .unwrap()
            .push(format!("{:?}", ev.phase));
        self.page.send_pointer_input(ev)
    }
    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.page.send_keyboard_input(ev)
    }
    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        self.page.move_focus(reason)
    }
    fn set_focus(&mut self, focused: bool) -> Result<(), SurfaceError> {
        self.lifetime
            .lock()
            .unwrap()
            .push(format!("focus {focused}"));
        Ok(())
    }
    fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
        None
    }
    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.page.apply_settings(settings)
    }
}
fn interception_page(f: &mut CallerCaptureFixture, index: usize) -> Calls {
    let lifetime: Calls = Default::default();
    f.contents.insert(
        f.nodes[index],
        PeltContent::from_surface(
            Box::new(InterceptionPage {
                page: Page {
                    calls: Default::default(),
                    pointers: f.pointers[index].clone(),
                    acquisitions: Default::default(),
                    title: None,
                    fail_offset: None,
                },
                lifetime: lifetime.clone(),
            }),
            (100, 100),
            route(),
        ),
    );
    f.sync(true);
    lifetime
}
impl super::super::input::PrimaryPressAdmissionHost for PrimaryPaneCaptureFixture {
    fn pointer_press_action(&mut self, action: crate::action::Action) {
        let events = self.content.pointers[0].lock().unwrap();
        if self.content.window == 0 {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.phase == PointerPhase::Cancel)
                    .count(),
                1,
                "primary interception must retire the old receiver BEFORE executing its App action"
            );
            assert_eq!(
                self.cancellation_capture,
                vec![Some(SurfaceKind::Content(self.content.nodes[0]))],
                "retirement must observe the old shell capture before it is cleared"
            );
        } else {
            assert!(
                events.iter().all(|e| e.phase != PointerPhase::Cancel),
                "a primary action must preserve foreign Lens custody"
            );
        }
        drop(events);
        self.content.app.update(action);
    }
    fn pointer_omnibar_open(&self) -> bool {
        self.content.app.omnibar.open
    }
    fn pointer_retire_primary_pointers(&mut self) {
        self.cancellation_capture.push(self.capture);
        TouchInput::default().cancel_window(&mut self.content.contents.pool, 0);
    }
}
fn interception_fixture() -> (PrimaryPaneCaptureFixture, Calls) {
    let mut f = PrimaryPaneCaptureFixture::new();
    let lifetime = interception_page(&mut f.content, 0);
    f.press(f.content.position(0), winit::event::MouseButton::Left);
    assert_eq!(
        captured_mouse(&f.content.contents.pool),
        Some(f.content.surfaces[0].id)
    );
    lifetime.lock().unwrap().clear();
    (f, lifetime)
}
fn assert_interception_retired(
    f: &mut PrimaryPaneCaptureFixture,
    lifetime: &Calls,
    button: winit::event::MouseButton,
) {
    assert_eq!(captured_mouse(&f.content.contents.pool), None);
    assert_eq!(f.capture, None);
    assert_eq!(
        f.content.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Cancel]
    );
    assert!(
        !lifetime.lock().unwrap().iter().any(|e| e == "focus false"),
        "interception must not invent content or OS focus loss"
    );
    let physical = f.content.buttons[&0];
    assert!(physical.0 & PointerButtons::PRIMARY.0 != 0);
    assert!(
        physical.0
            & match button {
                winit::event::MouseButton::Middle => PointerButtons::AUXILIARY.0,
                winit::event::MouseButton::Right => PointerButtons::SECONDARY.0,
                _ => panic!("expected an intercepted chord button"),
            }
            != 0
    );
    // Immediate releases occur before a redraw/placement sync and cannot reach
    // the retired producer. Physical masks still follow the real OS envelope.
    f.release(f.content.position(0), winit::event::MouseButton::Left);
    f.release(f.content.position(0), button);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    assert_eq!(f.content.pointers[0].lock().unwrap().len(), 2);
    assert!(f.content.pointers[1].lock().unwrap().is_empty());
}
fn interception_bookkeeping(f: &mut PrimaryPaneCaptureFixture, button: winit::event::MouseButton) {
    use super::super::input::PrimaryPointerHost;
    f.pointer_window_mouse(
        None,
        (-100., -100.),
        PointerPhase::Down,
        super::super::input::surface_mouse_button(button),
    );
}
#[test]
fn review_native_interception_chrome_retires_before_action_and_drop() {
    let (mut f, lifetime) = interception_fixture();
    interception_bookkeeping(&mut f, winit::event::MouseButton::Middle);
    let node = f.content.nodes[0];
    assert!(super::super::input::dispatch_primary_chrome_press(
        &mut f,
        Some(crate::chrome_view::ChromeIntent::KeepNode(node))
    ));
    // Actual KeepNode preserves the content focus; it must stay logically
    // focused while the pressed gesture alone is retired.
    assert_eq!(
        f.content.app.focus,
        FocusTarget::Content {
            node,
            appearance: f.content.surfaces[0].id
        }
    );
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Middle);
    drop(f.content.contents.remove(&node).unwrap());
    assert_eq!(
        *lifetime.lock().unwrap(),
        vec![
            "Cancel".to_string(),
            "focus false".to_string(),
            "drop".to_string()
        ],
        "receiver Cancel precedes actual removal focus loss and producer Drop"
    );
    // The same primary Chrome admission must not clear a Lens-owned mirror.
    let mut foreign = PrimaryPaneCaptureFixture {
        content: CallerCaptureFixture::new(1, None),
        capture: None,
        delivered: Vec::new(),
        cancellation_capture: Vec::new(),
    };
    assert!(
        foreign
            .content
            .press(0, inker::MouseButton::Right)
            .1
            .handled
    );
    foreign.capture = Some(SurfaceKind::Content(foreign.content.nodes[0]));
    let mirror = foreign.capture;
    let node = foreign.content.nodes[0];
    interception_bookkeeping(&mut foreign, winit::event::MouseButton::Middle);
    assert!(super::super::input::dispatch_primary_chrome_press(
        &mut foreign,
        Some(crate::chrome_view::ChromeIntent::KeepNode(node))
    ));
    assert_eq!(foreign.capture, mirror);
    assert_eq!(
        captured_mouse(&foreign.content.contents.pool),
        Some(foreign.content.surfaces[0].id)
    );
    assert!(
        foreign
            .content
            .pointer(PointerPhase::Up, Some(inker::MouseButton::Right))
            .1
            .handled
    );
}
#[test]
fn review_native_interception_omnibar_swallow_retires_before_action() {
    let (mut f, lifetime) = interception_fixture();
    f.content
        .app
        .update(crate::action::Action::OmnibarOpen { command: false });
    assert!(f.content.app.omnibar.open);
    interception_bookkeeping(&mut f, winit::event::MouseButton::Middle);
    assert!(super::super::input::reject_primary_omnibar_press(&mut f));
    assert!(!f.content.app.omnibar.open);
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Middle);
}
#[test]
fn review_native_interception_context_swallow_retires_before_action() {
    let (mut f, lifetime) = interception_fixture();
    let pos = f.content.position(1);
    let hit = crate::surface::hit_test(&f.content.plan(), f.content.app.focus, pos.x, pos.y);
    assert!(matches!(hit.unwrap().kind, SurfaceKind::Content(_)));
    interception_bookkeeping(&mut f, winit::event::MouseButton::Right);
    assert!(super::super::input::dispatch_primary_context_press(
        &mut f,
        winit::event::MouseButton::Right,
        hit
    ));
    assert!(f.content.app.omnibar.open);
    assert_interception_retired(&mut f, &lifetime, winit::event::MouseButton::Right);
}
fn lens_fresh_pane(f: &CallerCaptureFixture) -> PhysicalPosition {
    let plan = f.plan();
    let focus = FocusTarget::Graph(f.app.default_graph_pane());
    for surface in &plan {
        if !matches!(surface.kind, SurfaceKind::Pane(_)) {
            continue;
        }
        for dy in [1., 8., surface.rect.h / 2., surface.rect.h - 1.] {
            for dx in [1., 8., surface.rect.w / 2., surface.rect.w - 1.] {
                let pos = PhysicalPosition {
                    x: surface.rect.x + dx,
                    y: surface.rect.y + dy,
                };
                if crate::surface::hit_test(&plan, focus, pos.x, pos.y)
                    .is_some_and(|h| h.id == surface.id)
                {
                    return pos;
                }
            }
        }
    }
    panic!("actual Lens plan exposes a Pane before any content routing");
}
fn lens_interception_chord(none_hit: bool) {
    let mut f = CallerCaptureFixture::new(1, None);
    assert!(f.press(0, inker::MouseButton::Right).1.handled);
    let position = if none_hit {
        PhysicalPosition {
            x: -1000.,
            y: -1000.,
        }
    } else {
        lens_fresh_pane(&f)
    };
    let graphfocus = FocusTarget::Graph(f.app.default_graph_pane());
    let fresh = crate::surface::hit_test(&f.plan(), graphfocus, position.x, position.y);
    if none_hit {
        assert!(fresh.is_none());
    } else {
        assert!(matches!(fresh.unwrap().kind, SurfaceKind::Pane(_)));
    }
    assert!(
        native_mouse_input(
            &mut f.contents.pool,
            &mut f.buttons,
            1,
            None,
            position,
            PointerPhase::Down,
            Some(inker::MouseButton::Left),
            Default::default()
        )
        .is_none()
    );
    // The production Lens resolver is called before its Pane/None branch,
    // against the actual App plan and focus-derived placement construction.
    f.sync(true);
    let hit =
        super::super::lens::lens_press_hit(&f.contents.pool, 1, &f.plan(), graphfocus, position)
            .expect("admitted Lens custody wins even when the fresh plan has no hit");
    assert_eq!(
        hit.kind,
        SurfaceKind::Content(f.nodes[0]),
        "admitted Lens custody must precede fresh Pane dispatch"
    );
    let hit = content_press_target(&mut f.app.focus, &f.contents.pool, 1, position, hit);
    f.sync(true);
    f.assert_a_custody();
    let (source, effect) = content_mouse_input(
        &mut f.contents.pool,
        &f.buttons,
        1,
        Some(hit.id),
        position,
        PointerPhase::Down,
        Some(inker::MouseButton::Left),
        Default::default(),
    )
    .unwrap();
    assert_eq!(source, ContentKey::Node(f.nodes[0]));
    assert!(effect.handled);
    let events = f.pointers[0].lock().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].phase, PointerPhase::Down);
    assert_eq!(events[1].button, Some(inker::MouseButton::Left));
    assert_eq!(
        events[1].buttons,
        PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0)
    );
    assert_eq!(
        events[1].position,
        PhysicalPosition {
            x: position.x - f.surfaces[0].rect.x,
            y: position.y - f.surfaces[0].rect.y
        }
    );
    assert!(f.pointers[1].lock().unwrap().is_empty());
    // An unrelated release preserves both admitted buttons and receiver.
    let unrelated = native_mouse_input(
        &mut f.contents.pool,
        &mut f.buttons,
        1,
        Some(hit.id),
        position,
        PointerPhase::Up,
        Some(inker::MouseButton::Middle),
        Default::default(),
    )
    .unwrap();
    assert!(!unrelated.1.handled);
    assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
    for button in [inker::MouseButton::Right, inker::MouseButton::Left] {
        let (source, effect) = native_mouse_input(
            &mut f.contents.pool,
            &mut f.buttons,
            1,
            Some(hit.id),
            position,
            PointerPhase::Up,
            Some(button),
            Default::default(),
        )
        .unwrap();
        assert_eq!(source, ContentKey::Node(f.nodes[0]));
        assert!(effect.handled);
    }
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert_eq!(f.buttons[&1], PointerButtons::NONE);
    assert_eq!(
        f.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![
            PointerPhase::Down,
            PointerPhase::Down,
            PointerPhase::Up,
            PointerPhase::Up
        ]
    );
    assert!(f.press(1, inker::MouseButton::Middle).1.handled);
    assert_eq!(captured_mouse(&f.contents.pool), Some(f.surfaces[1].id));
}
#[test]
fn review_native_interception_lens_admitted_chord_precedes_fresh_pane() {
    lens_interception_chord(false);
}
#[test]
fn review_native_interception_lens_admitted_chord_precedes_none_hit() {
    lens_interception_chord(true);
}
#[test]
fn review_native_interception_foreign_lens_capture_is_not_selected_or_cancelled() {
    let mut f = CallerCaptureFixture::new(1, None);
    assert!(f.press(0, inker::MouseButton::Right).1.handled);
    let position = lens_fresh_pane(&f);
    let focus = FocusTarget::Graph(f.app.default_graph_pane());
    let plan = f.plan();
    let fresh = crate::surface::hit_test(&plan, focus, position.x, position.y);
    assert_eq!(
        super::super::lens::lens_press_hit(&f.contents.pool, 0, &plan, focus, position),
        fresh
    );
    assert!(
        super::super::lens::lens_press_hit(
            &f.contents.pool,
            0,
            &plan,
            focus,
            PhysicalPosition {
                x: -1000.,
                y: -1000.
            }
        )
        .is_none()
    );
    TouchInput::default().cancel_window(&mut f.contents.pool, 0);
    f.assert_a_custody();
    assert_eq!(f.pointers[0].lock().unwrap().len(), 1);
    assert!(
        f.pointer(PointerPhase::Up, Some(inker::MouseButton::Right))
            .1
            .handled
    );
    assert_eq!(captured_mouse(&f.contents.pool), None);
}
#[test]
fn review_native_interception_window_cancel_preserves_focus_and_foreign_touch() {
    let mut f = CallerCaptureFixture::new(0, None);
    let lifetime = interception_page(&mut f, 0);
    assert!(f.press(0, inker::MouseButton::Left).1.handled);
    let mut touch = TouchInput::default();
    let plan = f.plan();
    let pos = f.position(0);
    assert!(touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        0,
        70,
        PointerPhase::Down,
        pos,
        Some(0.6),
        None,
        Default::default()
    ));
    // Give a second real admitted appearance its own foreign native window.
    let mut foreign = f
        .contents
        .pool
        .placement(&f.surfaces[1].id)
        .unwrap()
        .clone();
    foreign.window = 1;
    let own = f
        .contents
        .pool
        .placement(&f.surfaces[0].id)
        .unwrap()
        .clone();
    f.contents.pool.set_placements([
        (f.surfaces[0].id, own.clone()),
        (f.surfaces[1].id, foreign.clone()),
    ]);
    let foreignpos = f.position(1);
    assert!(touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        1,
        71,
        PointerPhase::Down,
        foreignpos,
        Some(0.7),
        None,
        Default::default()
    ));
    let admitted = f.pointers[0].lock().unwrap().clone();
    assert_eq!(touch.contacts.len(), 2);
    lifetime.lock().unwrap().clear();
    touch.cancel_window(&mut f.contents.pool, 0);
    assert_eq!(touch.contacts.len(), 1);
    assert!(touch.contacts.contains_key(&(1, 71)));
    let retired = f.pointers[0].lock().unwrap().clone();
    for old in admitted {
        let cancel = retired
            .iter()
            .find(|event| event.pointer_id == old.pointer_id && event.phase == PointerPhase::Cancel)
            .expect("each owned pointer is retired once");
        assert_eq!(cancel.position, old.position);
        assert_eq!(cancel.pressure, old.pressure);
        assert_eq!(cancel.pointer_type, old.pointer_type);
        assert_eq!(cancel.buttons, PointerButtons::NONE);
        assert_eq!(cancel.button, None);
        assert_eq!(
            retired
                .iter()
                .filter(|event| event.pointer_id == old.pointer_id
                    && event.phase == PointerPhase::Cancel)
                .count(),
            1
        );
    }
    assert!(
        !lifetime.lock().unwrap().iter().any(|e| e == "focus false"),
        "window pointer retirement must preserve the receiver's actual focus"
    );
    assert_eq!(
        f.contents.pool.focused_content(0),
        Some(&ContentKey::Node(f.nodes[0]))
    );
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert_eq!(f.buttons[&0], PointerButtons::PRIMARY);
    assert!(!touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        0,
        70,
        PointerPhase::Up,
        pos,
        None,
        None,
        Default::default()
    ));
    assert!(touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        1,
        71,
        PointerPhase::Move,
        foreignpos,
        Some(0.8),
        None,
        Default::default()
    ));
    assert!(touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        1,
        71,
        PointerPhase::Up,
        foreignpos,
        None,
        None,
        Default::default()
    ));
    assert_eq!(
        f.pointers[0]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![
            PointerPhase::Down,
            PointerPhase::Down,
            PointerPhase::Cancel,
            PointerPhase::Cancel
        ]
    );
    assert_eq!(
        f.pointers[1]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Move, PointerPhase::Up]
    );
    // Unchanged placement sync must not silently repair an invented blur; no
    // extra focus event is required because cancellation preserved focus.
    f.contents
        .pool
        .set_placements([(f.surfaces[0].id, own), (f.surfaces[1].id, foreign)]);
    assert!(
        !lifetime
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.starts_with("focus "))
    );
}
#[test]
fn review_native_interception_real_focus_loss_still_blurs_and_retires() {
    let mut f = CallerCaptureFixture::new(0, None);
    let lifetime = interception_page(&mut f, 0);
    assert!(f.press(0, inker::MouseButton::Left).1.handled);
    lifetime.lock().unwrap().clear();
    TouchInput::default().focus_lost(&mut f.contents.pool, 0);
    assert_eq!(
        *lifetime.lock().unwrap(),
        vec!["Cancel".to_string(), "focus false".to_string()]
    );
    assert_eq!(captured_mouse(&f.contents.pool), None);
    assert_eq!(f.buttons[&0], PointerButtons::PRIMARY);
}

/// Lens caller below native bookkeeping, with a recording retained pane
/// receiver. App plans, content pool and both production admission/terminal
/// seams are real; only the contributed renderer and native Window are absent.
struct LensPaneCustodyFixture {
    content: CallerCaptureFixture,
    admitted: Option<super::super::input::ContributedPointer>,
    capture: Option<SurfaceKind>,
    pane_events: Vec<(PointerPhase, u64, (f32, f32))>,
    generation: u64,
}
impl LensPaneCustodyFixture {
    fn new(foreign_primary: bool, extra_lens_pane: bool) -> Self {
        let mut content = CallerCaptureFixture::new(1, None);
        if extra_lens_pane || foreign_primary {
            if foreign_primary {
                content.app.active_pane = Some(content.app.default_graph_pane());
            }
            content.app.update(crate::action::Action::SummonPane(
                crate::panes::PaneKindId::new(crate::panes::kind::ROSTER),
            ));
            let plan = content.plan();
            content.surfaces = content.nodes.map(|node| {
                *plan
                    .iter()
                    .find(|surface| surface.kind == SurfaceKind::Content(node))
                    .expect("actual Lens content remains exposed")
            });
        }
        let owner_window = if foreign_primary { 0 } else { 1 };
        let owner_plan = if foreign_primary {
            super::super::primary_content_plan(&content.app, &content.contents, 1200, 900)
        } else {
            content.plan()
        };
        let surface = *owner_plan
            .iter()
            .find(|surface| matches!(surface.kind, SurfaceKind::Pane(_)))
            .expect("actual owner window exposes a retained pane");
        let SurfaceKind::Pane(pane) = surface.kind else {
            unreachable!()
        };
        content.app.focus = FocusTarget::Pane(pane);
        content.sync(true);
        let position = PhysicalPosition {
            x: surface.rect.x + 1.,
            y: surface.rect.y + 1.,
        };
        assert!(
            native_mouse_input(
                &mut content.contents.pool,
                &mut content.buttons,
                owner_window,
                None,
                position,
                PointerPhase::Down,
                Some(inker::MouseButton::Left),
                Default::default()
            )
            .is_none()
        );
        let admitted = super::super::input::ContributedPointer {
            surface,
            generation: 77,
            window: owner_window,
        };
        Self {
            content,
            admitted: Some(admitted),
            capture: Some(surface.kind),
            pane_events: vec![(PointerPhase::Down, owner_window, (1., 1.))],
            generation: 77,
        }
    }
    fn actual_owner_plan(&self, window: u64) -> Vec<Surface> {
        if window == 0 {
            super::super::primary_content_plan(&self.content.app, &self.content.contents, 1200, 900)
        } else {
            self.content.plan()
        }
    }
    fn press(
        &mut self,
        window: u64,
        position: PhysicalPosition,
        button: winit::event::MouseButton,
    ) -> bool {
        assert!(
            native_mouse_input(
                &mut self.content.contents.pool,
                &mut self.content.buttons,
                window,
                None,
                position,
                PointerPhase::Down,
                super::super::input::surface_mouse_button(button),
                Default::default()
            )
            .is_none()
        );
        if super::super::lens::reject_lens_pane_press(self) {
            return true;
        }
        let plan = self.content.plan();
        let graphfocus = FocusTarget::Graph(self.content.app.default_graph_pane());
        let Some(hit) = super::super::lens::lens_press_hit(
            &self.content.contents.pool,
            window,
            &plan,
            graphfocus,
            position,
        ) else {
            return false;
        };
        match hit.kind {
            SurfaceKind::Content(_) => {
                self.content.sync(true);
                let hit = content_press_target(
                    &mut self.content.app.focus,
                    &self.content.contents.pool,
                    window,
                    position,
                    hit,
                );
                self.capture = Some(hit.kind);
                self.content.sync(true);
                let routed = content_mouse_input(
                    &mut self.content.contents.pool,
                    &self.content.buttons,
                    window,
                    Some(hit.id),
                    position,
                    PointerPhase::Down,
                    super::super::input::surface_mouse_button(button),
                    Default::default(),
                );
                assert!(routed.is_some_and(|(_, effect)| effect.handled));
                true
            },
            SurfaceKind::Pane(pane) if button == winit::event::MouseButton::Left => {
                // Renderer adapter boundary in the actual Lens Pane arm: it
                // retains the exact real-plan surface and receiver generation.
                self.content.app.focus = FocusTarget::Pane(pane);
                self.capture = Some(hit.kind);
                self.admitted = Some(super::super::input::ContributedPointer {
                    surface: *plan.iter().find(|surface| surface.id == hit.id).unwrap(),
                    generation: self.generation,
                    window,
                });
                self.pane_events
                    .push((PointerPhase::Down, window, hit.local));
                true
            },
            _ => false,
        }
    }
    fn release(
        &mut self,
        window: u64,
        position: PhysicalPosition,
        button: winit::event::MouseButton,
    ) -> bool {
        // Same production Lens prelude: while a pane owns the mouse, terminal
        // bookkeeping is physical only; the pane terminal seam owns delivery.
        let appearance = if self.admitted.is_none() {
            captured_mouse(&self.content.contents.pool)
        } else {
            None
        };
        native_mouse_input(
            &mut self.content.contents.pool,
            &mut self.content.buttons,
            window,
            appearance,
            position,
            PointerPhase::Up,
            super::super::input::surface_mouse_button(button),
            Default::default(),
        );
        super::super::lens::dispatch_lens_pane_release(
            self,
            window,
            (position.x, position.y),
            button,
        )
    }
    fn assert_retained(&self, original: super::super::input::ContributedPointer) {
        assert_eq!(self.admitted.unwrap().surface, original.surface);
        assert_eq!(self.admitted.unwrap().generation, original.generation);
        assert_eq!(self.admitted.unwrap().window, original.window);
        assert_eq!(self.capture, Some(original.surface.kind));
        let SurfaceKind::Pane(pane) = original.surface.kind else {
            unreachable!()
        };
        assert_eq!(self.content.app.focus, FocusTarget::Pane(pane));
        assert_eq!(
            self.pane_events.len(),
            1,
            "a chord cannot admit a second retained pane Down"
        );
        assert_eq!(
            captured_mouse(&self.content.contents.pool),
            None,
            "a pane-owned chord cannot admit a competing content receiver"
        );
        assert!(
            self.content
                .pointers
                .iter()
                .all(|events| events.lock().unwrap().is_empty())
        );
    }
    fn fresh_content_after_terminal(&mut self) {
        let pos = self.content.position(0);
        assert!(self.press(1, pos, winit::event::MouseButton::Right));
        assert!(self.admitted.is_none());
        assert_eq!(
            captured_mouse(&self.content.contents.pool),
            Some(self.content.surfaces[0].id)
        );
        assert!(!self.release(1, pos, winit::event::MouseButton::Right));
        assert_eq!(captured_mouse(&self.content.contents.pool), None);
        assert_eq!(self.content.buttons[&1], PointerButtons::NONE);
        assert_eq!(
            self.content.pointers[0]
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.phase)
                .collect::<Vec<_>>(),
            vec![PointerPhase::Down, PointerPhase::Up]
        );
    }
}
impl super::super::lens::LensPanePointerHost for LensPaneCustodyFixture {
    fn lens_pane_admission(&self) -> Option<super::super::input::ContributedPointer> {
        self.admitted
    }
    fn lens_pane_admission_current(
        &self,
        admitted: super::super::input::ContributedPointer,
    ) -> bool {
        admitted.is_current(
            self.actual_owner_plan(admitted.window)
                .into_iter()
                .find(|surface| surface.id == admitted.surface.id),
            Some(self.generation),
            admitted.window,
        )
    }
    fn lens_cancel_stale_admission(&mut self) {
        panic!("these controls qualify valid custody, not existing global stale cleanup");
    }
    fn lens_release_admission(&mut self, window: u64, x: f32, y: f32) {
        let Some(admitted) = self.admitted else {
            return;
        };
        if admitted.window != window {
            return;
        }
        assert!(
            super::super::lens::LensPanePointerHost::lens_pane_admission_current(self, admitted)
        );
        self.admitted = None;
        self.capture = None;
        self.pane_events.push((
            PointerPhase::Up,
            window,
            (x - admitted.surface.rect.x, y - admitted.surface.rect.y),
        ));
    }
}
#[test]
fn review_lens_pane_custody_chord_content_and_none_preserve_receiver() {
    let mut f = LensPaneCustodyFixture::new(false, false);
    let admitted = f.admitted.unwrap();
    let position = f.content.position(0);
    assert!(f.press(1, position, winit::event::MouseButton::Right));
    f.assert_retained(admitted);
    assert_eq!(
        f.content.buttons[&1],
        PointerButtons(PointerButtons::PRIMARY.0 | PointerButtons::SECONDARY.0)
    );
    assert!(f.press(
        1,
        PhysicalPosition {
            x: -1000.,
            y: -1000.
        },
        winit::event::MouseButton::Middle
    ));
    f.assert_retained(admitted);
    assert!(f.release(1, position, winit::event::MouseButton::Middle));
    f.assert_retained(admitted);
    assert!(f.release(1, position, winit::event::MouseButton::Right));
    f.assert_retained(admitted);
    assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    assert!(f.release(1, position, winit::event::MouseButton::Left));
    assert_eq!(
        f.pane_events,
        vec![
            (PointerPhase::Down, 1, (1., 1.)),
            (
                PointerPhase::Up,
                1,
                (
                    position.x - admitted.surface.rect.x,
                    position.y - admitted.surface.rect.y
                )
            )
        ]
    );
    assert_eq!(f.content.buttons[&1], PointerButtons::NONE);
    f.fresh_content_after_terminal();
}
#[test]
fn review_lens_pane_custody_second_left_cannot_replace_pane() {
    let mut f = LensPaneCustodyFixture::new(false, true);
    let admitted = f.admitted.unwrap();
    let plan = f.content.plan();
    let mut exposed = None;
    for surface in &plan {
        if !matches!(surface.kind, SurfaceKind::Pane(_)) || surface.id == admitted.surface.id {
            continue;
        }
        for dy in [1., 8., surface.rect.h / 2., surface.rect.h - 1.] {
            for dx in [1., 8., surface.rect.w / 2., surface.rect.w - 1.] {
                let position = PhysicalPosition {
                    x: surface.rect.x + dx,
                    y: surface.rect.y + dy,
                };
                if crate::surface::hit_test(
                    &plan,
                    FocusTarget::Graph(f.content.app.default_graph_pane()),
                    position.x,
                    position.y,
                )
                .is_some_and(|hit| hit.id == surface.id)
                {
                    exposed = Some(position);
                }
            }
        }
    }
    let position = exposed.expect("actual second Lens pane exposes a fresh Pane hit");
    assert!(f.press(1, position, winit::event::MouseButton::Left));
    f.assert_retained(admitted);
    assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    assert!(f.release(1, position, winit::event::MouseButton::Left));
    assert_eq!(f.pane_events.len(), 2);
    f.fresh_content_after_terminal();
}
#[test]
fn review_lens_pane_custody_foreign_primary_receiver_preserved() {
    let mut f = LensPaneCustodyFixture::new(true, false);
    let admitted = f.admitted.unwrap();
    assert_eq!(admitted.window, 0);
    let position = f.content.position(0);
    assert!(f.press(1, position, winit::event::MouseButton::Right));
    f.assert_retained(admitted);
    assert_eq!(f.content.buttons[&0], PointerButtons::PRIMARY);
    assert_eq!(f.content.buttons[&1], PointerButtons::SECONDARY);
    assert!(f.release(1, position, winit::event::MouseButton::Right));
    f.assert_retained(admitted);
    assert!(f.release(1, position, winit::event::MouseButton::Left));
    f.assert_retained(admitted);
    assert_eq!(f.content.buttons[&0], PointerButtons::PRIMARY);
    assert_eq!(f.content.buttons[&1], PointerButtons::NONE);
    assert!(f.release(0, position, winit::event::MouseButton::Left));
    assert_eq!(f.pane_events.len(), 2);
    assert_eq!(f.content.buttons[&0], PointerButtons::NONE);
    f.fresh_content_after_terminal();
}
#[test]
fn review_lens_pane_custody_wrong_button_release_preserves_admitted_left() {
    let mut f = LensPaneCustodyFixture::new(false, false);
    let admitted = f.admitted.unwrap();
    let position = f.content.position(0);
    for button in [
        winit::event::MouseButton::Right,
        winit::event::MouseButton::Middle,
    ] {
        assert!(f.release(1, position, button));
        f.assert_retained(admitted);
        assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    }
    assert!(f.release(0, position, winit::event::MouseButton::Left));
    f.assert_retained(admitted);
    assert_eq!(f.content.buttons[&1], PointerButtons::PRIMARY);
    assert!(f.release(1, position, winit::event::MouseButton::Left));
    assert_eq!(f.pane_events.len(), 2);
    assert_eq!(f.content.buttons[&1], PointerButtons::NONE);
    f.fresh_content_after_terminal();
}

/// Uses the exact UA gate called by Shell before retained touch delivery.
/// App admission, NodeDocuments, TouchInput and producer receivers are real;
/// native Window creation and OS event decoding remain outside this fixture.
fn modal_touch_open(f: &mut CallerCaptureFixture) -> crate::user_agent_decision::UserAgentRequestKey {
    let focus = f.app.focus;
    let effects = f.app.apply_update(crate::action::Update::PermissionRequested {
        node: f.nodes[0],
        id: inker::UserAgentRequestId::new(901),
        origin: "https://upper.test".into(),
        descriptors: vec![inker::PermissionDescriptor::Geolocation],
    });
    assert!(effects.iter().any(|effect| matches!(effect, crate::action::Effect::Redraw)));
    assert_eq!(f.app.focus, focus, "retained UA admission is not native focus loss");
    f.app.user_agent_decision.active_request().expect("actual App update opened the UA gate")
}
fn modal_touch_close(f: &mut CallerCaptureFixture, request: crate::user_agent_decision::UserAgentRequestKey) {
    f.app.apply_update(crate::action::Update::UserAgentRequestWithdrawn {
        request, kind: "permission", reason: "test-withdrawal",
    });
    assert!(!f.app.user_agent_decision.is_open());
}
fn modal_touch_delivery(
    f: &mut CallerCaptureFixture, touch: &mut TouchInput, window: u64,
    host_id: u64, phase: PointerPhase, index: usize,
) -> bool {
    if touch.reject_modal(&mut f.contents.pool, window, f.app.user_agent_decision.is_open()) {
        return false;
    }
    // Placement construction is already held by the real-plan fixture; the
    // rejected branch must return before any geometry, focus or page delivery.
    let plan = f.plan();
    let position = f.position(index);
    touch.deliver(&mut f.contents.pool, &plan, f.app.focus, window, host_id,
        phase, position, Some(0.6), Some(0.4), Default::default())
}
fn modal_touch_terminal(window: u64, terminal: PointerPhase) {
    let mut f = CallerCaptureFixture::new(window, None);
    let lifetime = interception_page(&mut f, 0);
    let mut touch = TouchInput::default();
    // A second real appearance is admitted in another window, as in the
    // existing window-cancellation fixture. It must survive this gate.
    let own = f.contents.pool.placement(&f.surfaces[0].id).unwrap().clone();
    let mut foreign = f.contents.pool.placement(&f.surfaces[1].id).unwrap().clone();
    let foreign_window = if window == 0 { 1 } else { 0 };
    foreign.window = foreign_window;
    f.contents.pool.set_placements([(f.surfaces[0].id, own), (f.surfaces[1].id, foreign)]);
    assert!(modal_touch_delivery(&mut f, &mut touch, window, 70, PointerPhase::Down, 0));
    assert!(modal_touch_delivery(&mut f, &mut touch, foreign_window, 71, PointerPhase::Down, 1));
    let own_id = touch.contacts[&(window, 70)].pointer_id;
    let foreign_id = touch.contacts[&(foreign_window, 71)].pointer_id;
    assert!(own_id > 1 && foreign_id > 1 && own_id != foreign_id);
    let down = f.pointers[0].lock().unwrap()[0].clone();
    assert_eq!(down.pointer_id, own_id);
    let focus = f.app.focus;
    let request = modal_touch_open(&mut f);
    lifetime.lock().unwrap().clear();
    assert!(!modal_touch_delivery(&mut f, &mut touch, window, 70, terminal, 0));
    assert_eq!(f.contents.pool.captured_placement(own_id), None,
        "held UA gate must retire the physically ended/cancelled nonzero contact");
    assert!(!touch.contacts.contains_key(&(window, 70)),
        "no later physical Up is required to remove host contact custody");
    assert_eq!(f.contents.pool.captured_placement(foreign_id), Some(&f.surfaces[1].id));
    assert!(touch.contacts.contains_key(&(foreign_window, 71)));
    assert_eq!(f.app.focus, focus);
    let own_events = f.pointers[0].lock().unwrap().clone();
    assert_eq!(own_events.len(), 2);
    assert_eq!(own_events[1].phase, PointerPhase::Cancel);
    assert_eq!(own_events[1].pointer_id, own_id);
    assert_eq!(own_events[1].position, down.position);
    assert_eq!(own_events[1].pressure, down.pressure);
    assert_eq!(own_events[1].altitude_angle, down.altitude_angle);
    assert_eq!(own_events[1].pointer_type, inker::PointerType::Touch);
    assert_eq!(own_events[1].buttons, PointerButtons::NONE);
    assert_eq!(own_events[1].button, None);
    assert_eq!(*lifetime.lock().unwrap(), vec!["Cancel".to_string()],
        "modal touch rejection cancels without Up activation or focus loss");
    assert_eq!(f.pointers[1].lock().unwrap().len(), 1);
    // Repeated terminal while blocked does not resend cancellation.
    assert!(!modal_touch_delivery(&mut f, &mut touch, window, 70, terminal, 0));
    assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
    modal_touch_close(&mut f, request);
    // A stale move after withdrawal cannot revive the old contact; this is
    // intentionally not another physical Up used to repair the test state.
    assert!(!modal_touch_delivery(&mut f, &mut touch, window, 70, PointerPhase::Move, 0));
    assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
    assert!(modal_touch_delivery(&mut f, &mut touch, foreign_window, 71, PointerPhase::Move, 1));
    assert!(modal_touch_delivery(&mut f, &mut touch, foreign_window, 71, PointerPhase::Up, 1));
    assert_eq!(f.contents.pool.captured_placement(foreign_id), None);
    assert!(touch.contacts.is_empty());
    assert_eq!(f.pointers[1].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Move, PointerPhase::Up]);
    drop(f.contents.pool.remove_content(&ContentKey::Node(f.nodes[0])));
    let history = lifetime.lock().unwrap().clone();
    assert_eq!(history.iter().filter(|event| event.as_str() == "Cancel").count(), 1);
    assert!(history.iter().position(|event| event == "Cancel").unwrap()
        < history.iter().position(|event| event == "drop").unwrap());
}
#[test]
fn review_modal_touch_primary_ended_retires_without_up_activation() {
    modal_touch_terminal(0, PointerPhase::Up);
}
#[test]
fn review_modal_touch_primary_cancelled_retires_without_later_up() {
    modal_touch_terminal(0, PointerPhase::Cancel);
}
#[test]
fn review_modal_touch_lens_ended_preserves_foreign_primary_contact() {
    modal_touch_terminal(1, PointerPhase::Up);
}
#[test]
fn review_modal_touch_lens_cancelled_preserves_foreign_primary_contact() {
    modal_touch_terminal(1, PointerPhase::Cancel);
}
#[test]
fn review_modal_touch_fresh_down_is_blocked_then_new_gesture_is_admitted() {
    for window in [0, 1] {
        let mut f = CallerCaptureFixture::new(window, None);
        let mut touch = TouchInput::default();
        let request = modal_touch_open(&mut f);
        assert!(!modal_touch_delivery(&mut f, &mut touch, window, 80, PointerPhase::Down, 0));
        assert!(touch.contacts.is_empty());
        assert!(!f.contents.pool.has_pointer_capture());
        assert!(f.pointers.iter().all(|events| events.lock().unwrap().is_empty()));
        modal_touch_close(&mut f, request);
        assert!(modal_touch_delivery(&mut f, &mut touch, window, 80, PointerPhase::Down, 0));
        assert!(modal_touch_delivery(&mut f, &mut touch, window, 80, PointerPhase::Up, 0));
        assert!(touch.contacts.is_empty());
        assert!(!f.contents.pool.has_pointer_capture());
        assert_eq!(f.pointers[0].lock().unwrap().iter().map(|e| e.phase).collect::<Vec<_>>(),
            vec![PointerPhase::Down, PointerPhase::Up]);
    }
}
