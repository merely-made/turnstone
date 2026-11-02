// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn lens_interception_chord(none_hit: bool) {
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
    let hit = super::super::super::lens::lens_press_hit(
        &f.contents.pool,
        1,
        &f.plan(),
        graphfocus,
        position,
    )
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
