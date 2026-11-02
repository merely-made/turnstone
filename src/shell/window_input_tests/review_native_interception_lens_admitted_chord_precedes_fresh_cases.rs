// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
        super::super::super::lens::lens_press_hit(&f.contents.pool, 0, &plan, focus, position),
        fresh
    );
    assert!(
        super::super::super::lens::lens_press_hit(
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
        assert!(!modal_touch_delivery(
            &mut f,
            &mut touch,
            window,
            80,
            PointerPhase::Down,
            0
        ));
        assert!(touch.contacts.is_empty());
        assert!(!f.contents.pool.has_pointer_capture());
        assert!(
            f.pointers
                .iter()
                .all(|events| events.lock().unwrap().is_empty())
        );
        modal_touch_close(&mut f, request);
        assert!(modal_touch_delivery(
            &mut f,
            &mut touch,
            window,
            80,
            PointerPhase::Down,
            0
        ));
        assert!(modal_touch_delivery(
            &mut f,
            &mut touch,
            window,
            80,
            PointerPhase::Up,
            0
        ));
        assert!(touch.contacts.is_empty());
        assert!(!f.contents.pool.has_pointer_capture());
        assert_eq!(
            f.pointers[0]
                .lock()
                .unwrap()
                .iter()
                .map(|e| e.phase)
                .collect::<Vec<_>>(),
            vec![PointerPhase::Down, PointerPhase::Up]
        );
    }
}

#[test]
fn document_ime_admission_tracks_editable_focus_window_visibility_and_replacement() {
    struct Editor(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl inker::DocumentSession<netrender::Scene> for Editor {
        fn frame(&mut self, _: u32, _: u32) -> netrender::Scene {
            Default::default()
        }
        fn scroll_by(&mut self, _: f32, _: f32) -> bool {
            false
        }
        fn scroll_for_key(&mut self, _: inker::SessionScrollKey) -> bool {
            false
        }
        fn click_at(&mut self, _: f32, _: f32) -> inker::SessionClick {
            inker::SessionClick::Miss
        }
        fn links(&self) -> Vec<inker::SessionLink> {
            vec![]
        }
        fn editable_focus(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn focus_input(&mut self, focused: bool) {
            if !focused {
                self.0.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
        fn as_any_ref(&self) -> &dyn std::any::Any {
            self
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }
    let editable = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let controller = pelt_core::PeltController::from_session_shared(
        Arc::new(inker::SessionRegistry::new()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        pelt_core::PeltControllerConfig::new("ime", "https://fixture.test/ime", (100, 100)),
        super::super::super::ShellClock(std::time::Instant::now()),
        Box::new(Editor(editable.clone())),
    );
    let mut pool = Pool::new();
    let focus = FocusTarget::Content {
        node: uuid::Uuid::from_u128(7),
        appearance: SurfaceId(10),
    };
    pool.insert(key(7), PeltContent::from_controller(controller, route()));
    pool.set_placements([(SurfaceId(10), placed(key(7), 0, 0., true))]);
    assert!(!document_accepts_ime(&pool, focus, 0));
    editable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(document_accepts_ime(&pool, focus, 0));
    assert!(
        !document_accepts_ime(&pool, FocusTarget::Graph(crate::panes::PaneId(99)), 0),
        "an old placement cannot enable IME after host focus leaves content"
    );
    assert!(!document_accepts_ime(&pool, focus, 1));
    pool.set_placements([(SurfaceId(10), placed(key(7), 1, 0., true))]);
    // Focus loss retires editor composition; a new native editor focus admits
    // the lens only, even though it retains the same content identity.
    editable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(document_accepts_ime(&pool, focus, 1));
    assert!(!document_accepts_ime(&pool, focus, 0));
    let mut hidden = placed(key(7), 1, 0., true);
    hidden.visible = false;
    pool.set_placements([(SurfaceId(10), hidden)]);
    assert!(!document_accepts_ime(&pool, focus, 1));
    pool.set_placements([(SurfaceId(10), placed(key(7), 1, 0., true))]);
    editable.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(document_accepts_ime(&pool, focus, 1));
    pool.insert(key(7), hosted(&Calls::default()));
    assert!(
        !document_accepts_ime(&pool, focus, 1),
        "a browser cannot inherit the old retained editor"
    );
    pool.set_placements([]);
    assert!(!document_accepts_ime(&pool, focus, 1));
}
