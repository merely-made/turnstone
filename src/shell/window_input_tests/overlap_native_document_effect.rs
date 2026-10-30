// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn overlap_native_document_effect(submit: bool) {
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
            super::super::super::ShellClock(std::time::Instant::now()),
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
