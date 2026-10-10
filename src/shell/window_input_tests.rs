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
