// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn modal_touch_terminal(window: u64, terminal: PointerPhase) {
    let mut f = CallerCaptureFixture::new(window, None);
    let lifetime = interception_page(&mut f, 0);
    let mut touch = TouchInput::default();
    // A second real appearance is admitted in another window, as in the
    // existing window-cancellation fixture. It must survive this gate.
    let own = f
        .contents
        .pool
        .placement(&f.surfaces[0].id)
        .unwrap()
        .clone();
    let mut foreign = f
        .contents
        .pool
        .placement(&f.surfaces[1].id)
        .unwrap()
        .clone();
    let foreign_window = if window == 0 { 1 } else { 0 };
    foreign.window = foreign_window;
    f.contents
        .pool
        .set_placements([(f.surfaces[0].id, own), (f.surfaces[1].id, foreign)]);
    assert!(modal_touch_delivery(
        &mut f,
        &mut touch,
        window,
        70,
        PointerPhase::Down,
        0
    ));
    assert!(modal_touch_delivery(
        &mut f,
        &mut touch,
        foreign_window,
        71,
        PointerPhase::Down,
        1
    ));
    let own_id = touch.contacts[&(window, 70)].pointer_id;
    let foreign_id = touch.contacts[&(foreign_window, 71)].pointer_id;
    assert!(own_id > 1 && foreign_id > 1 && own_id != foreign_id);
    let down = f.pointers[0].lock().unwrap()[0].clone();
    assert_eq!(down.pointer_id, own_id);
    let focus = f.app.focus;
    let request = modal_touch_open(&mut f);
    lifetime.lock().unwrap().clear();
    assert!(!modal_touch_delivery(
        &mut f, &mut touch, window, 70, terminal, 0
    ));
    assert_eq!(
        f.contents.pool.captured_placement(own_id),
        None,
        "held UA gate must retire the physically ended/cancelled nonzero contact"
    );
    assert!(
        !touch.contacts.contains_key(&(window, 70)),
        "no later physical Up is required to remove host contact custody"
    );
    assert_eq!(
        f.contents.pool.captured_placement(foreign_id),
        Some(&f.surfaces[1].id)
    );
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
    assert_eq!(
        *lifetime.lock().unwrap(),
        vec!["Cancel".to_string()],
        "modal touch rejection cancels without Up activation or focus loss"
    );
    assert_eq!(f.pointers[1].lock().unwrap().len(), 1);
    // Repeated terminal while blocked does not resend cancellation.
    assert!(!modal_touch_delivery(
        &mut f, &mut touch, window, 70, terminal, 0
    ));
    assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
    modal_touch_close(&mut f, request);
    // A stale move after withdrawal cannot revive the old contact; this is
    // intentionally not another physical Up used to repair the test state.
    assert!(!modal_touch_delivery(
        &mut f,
        &mut touch,
        window,
        70,
        PointerPhase::Move,
        0
    ));
    assert_eq!(f.pointers[0].lock().unwrap().len(), 2);
    assert!(modal_touch_delivery(
        &mut f,
        &mut touch,
        foreign_window,
        71,
        PointerPhase::Move,
        1
    ));
    assert!(modal_touch_delivery(
        &mut f,
        &mut touch,
        foreign_window,
        71,
        PointerPhase::Up,
        1
    ));
    assert_eq!(f.contents.pool.captured_placement(foreign_id), None);
    assert!(touch.contacts.is_empty());
    assert_eq!(
        f.pointers[1]
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![PointerPhase::Down, PointerPhase::Move, PointerPhase::Up]
    );
    drop(
        f.contents
            .pool
            .remove_content(&ContentKey::Node(f.nodes[0])),
    );
    let history = lifetime.lock().unwrap().clone();
    assert_eq!(
        history
            .iter()
            .filter(|event| event.as_str() == "Cancel")
            .count(),
        1
    );
    assert!(
        history.iter().position(|event| event == "Cancel").unwrap()
            < history.iter().position(|event| event == "drop").unwrap()
    );
}
