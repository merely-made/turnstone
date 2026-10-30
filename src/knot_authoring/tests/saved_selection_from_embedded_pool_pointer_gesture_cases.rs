// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn saved_selection_from_embedded_pool_pointer_gesture() {
    use pelt_core::{
        PeltContent, PeltController, PeltControllerConfig, PeltControllerPool, PeltInput,
        PeltPlacement, PeltRoute, PeltRouteSource, PeltRouteState, WorkspaceRect,
    };
    struct Clock;
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            0.0
        }
    }
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("drag.djot");
    let source = "private prefix;\ncafé\nprivate suffix";
    fs::write(&path, source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory {
            root: root.path().into(),
        },
        None,
        None,
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let session = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (507, 508),
        false,
        false,
        false,
    );
    let engines = Arc::new(inker::SessionRegistry::new());
    let surfaces = Arc::new(inker::SurfaceEngineRegistry::new());
    let controller = PeltController::from_session_shared(
        engines,
        surfaces,
        PeltControllerConfig::new(ENGINE_ID, &address, (507, 508)),
        Clock,
        Box::new(session),
    );
    let decision = mere::routing::route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("embedded-knot-drag"),
        view: None,
        node: None,
        address,
        content_type: None,
        pinned_engine: Some(ENGINE_ID.into()),
    });
    let mut pool = PeltControllerPool::<u8, u8, Scene>::new();
    pool.insert(
        1,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    let placement = PeltPlacement {
        content: 1,
        window: 0,
        rect: WorkspaceRect::new(517.0, 92.0, 507.0, 508.0),
        visible: true,
        focused: true,
        scale_factor: 1.0,
    };
    pool.set_placements([(1, placement.clone())]);
    let _ = pool.frame(&1).unwrap();

    // Find the actual retained glyph row and caret positions for `café`.
    // The pool accepts window coordinates, so translate these content-local
    // points by the placement below. This keeps the gesture tied to the
    // rendered text instead of assuming a particular line height or font.
    let (start, end) = {
        let session = pool
            .content_mut(&1)
            .unwrap()
            .document_mut()
            .unwrap()
            .session_as_any_mut()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        let field = find_text_control(&session.dom.borrow(), session.runner.root()).unwrap();
        let (left, top, width, height) = session
            .layout
            .painted_rect(&session.dom.borrow(), field)
            .unwrap();
        let caret_byte = |session: &mut KnotDocumentSession, x, y| {
            session
                .layout
                .text_caret_at(&mut session.dom.borrow_mut(), field, x, y)
                .map(|position| position.byte)
        };
        let start_byte = source.find("café").unwrap();
        let end_byte = start_byte + "café".len();
        let center_x = left + width / 2.0;
        let line_y = (0..height.ceil() as usize)
            .map(|row| top + row as f32 + 0.5)
            .find(|&y| {
                caret_byte(session, center_x, y)
                    .is_some_and(|byte| (start_byte..=end_byte).contains(&byte))
            })
            .expect("retained text geometry contains the café row");
        let point_for_byte = |session: &mut KnotDocumentSession, target| {
            (0..width.ceil() as usize).find_map(|column| {
                let x = left + column as f32 + 0.5;
                (caret_byte(session, x, line_y) == Some(target)).then_some(x)
            })
        };
        let start_x = point_for_byte(session, start_byte)
            .expect("retained text geometry exposes café's starting caret");
        let end_x = point_for_byte(session, end_byte)
            .expect("retained text geometry exposes café's ending caret");
        assert_eq!(caret_byte(session, start_x, line_y), Some(start_byte));
        assert_eq!(caret_byte(session, end_x, line_y), Some(end_byte));
        (
            (placement.rect.x + start_x, placement.rect.y + line_y),
            (placement.rect.x + end_x, placement.rect.y + line_y),
        )
    };
    let pointer = |(x, y), phase| {
        PeltInput::Pointer(inker::PointerEvent {
            pointer_id: 1,
            pointer_type: inker::PointerType::Mouse,
            is_primary: true,
            phase,
            position: inker::PhysicalPosition { x, y },
            button: (phase != inker::PointerPhase::Move).then_some(inker::MouseButton::Left),
            buttons: if phase == inker::PointerPhase::Up {
                inker::PointerButtons::NONE
            } else {
                inker::PointerButtons::PRIMARY
            },
            modifiers: Default::default(),
            width: 1.0,
            height: 1.0,
            pressure: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
            altitude_angle: None,
            azimuth_angle: None,
        })
    };
    for (x, phase) in [
        (start, inker::PointerPhase::Down),
        (
            ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0),
            inker::PointerPhase::Move,
        ),
        (end, inker::PointerPhase::Move),
        (end, inker::PointerPhase::Up),
    ] {
        pool.set_placements([(1, placement.clone())]);
        let effect = pool.input_at(0, pointer(x, phase));
        if phase != inker::PointerPhase::Move {
            assert!(effect.handled, "{phase:?} at {x:?}");
        }
        if phase != inker::PointerPhase::Up {
            assert_eq!(pool.captured_placement(1), Some(&1));
        }
    }
    let session = pool
        .content(&1)
        .unwrap()
        .document()
        .unwrap()
        .session()
        .as_any_ref()
        .downcast_ref::<KnotDocumentSession>()
        .unwrap();
    let offer = session
        .retained_selection()
        .expect("embedded pool gesture selects source");
    assert_eq!((offer.start, offer.end), (16, 21));
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn saved_selection_from_pointer_drag_survives_reverse_release_and_cancel() {
    use inker::{SessionInput, SessionPointerButton};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("drag.djot");
    let source = "private prefix;\ncafé\nprivate suffix";
    fs::write(&path, source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory {
            root: root.path().into(),
        },
        None,
        None,
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let mut session = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (900, 600),
        false,
        false,
        false,
    );
    session.frame(900, 600);
    let field = find_text_control(&session.dom.borrow(), session.runner.root()).unwrap();
    let (x, y, _, _) = session
        .layout
        .painted_rect(&session.dom.borrow(), field)
        .unwrap();
    let (start, end, line) = (x + 10.0, x + 60.0, y + 31.0);
    let button = |x, state| SessionInput::PointerButton {
        x,
        y: line,
        button: SessionPointerButton::Primary,
        state,
        modifiers: Default::default(),
    };
    let movement = |x| SessionInput::PointerMoved {
        x,
        y: line,
        modifiers: Default::default(),
    };
    assert_eq!(
        session
            .input(button(start, SessionButtonState::Pressed))
            .capture,
        Some(true)
    );
    session.input(movement(end));
    session.input(button(end, SessionButtonState::Released));
    let offer = session
        .retained_selection()
        .expect("drag selects saved text");
    assert_eq!((offer.start, offer.end), (16, 21));
    assert_eq!(
        session.selected_retained_export(&offer).unwrap().0["text"],
        "café"
    );
    let selected = session.runner.state().editor.selection();
    session.input(movement(start));
    assert_eq!(
        session.runner.state().editor.selection(),
        selected,
        "release ends capture"
    );

    session.frame(900, 600);
    session.input(button(end, SessionButtonState::Pressed));
    session.input(movement(start));
    session.input(button(start, SessionButtonState::Released));
    let reverse = session.runner.state().editor.selection();
    assert_eq!((reverse.anchor.byte, reverse.focus.byte), (21, 16));
    assert_eq!(session.retained_selection(), Some(offer.clone()));

    session.input(button(start, SessionButtonState::Pressed));
    session.input(movement(end));
    session.input(SessionInput::Cancel);
    let cancelled = session.runner.state().editor.selection();
    session.input(button(start, SessionButtonState::Released));
    session.input(movement(start));
    assert_eq!(
        session.runner.state().editor.selection(),
        cancelled,
        "cancel prevents trailing selection updates"
    );
    session.input(button(end, SessionButtonState::Pressed));
    session.input(SessionInput::Focus(false));
    let blurred = session.runner.state().editor.selection();
    session.input(movement(start));
    assert_eq!(session.runner.state().editor.selection(), blurred);
    assert!(
        session.selected_retained_export(&offer).is_err(),
        "changed selection invalidates a prior offer"
    );
    // A captured drag continues outside the field and across painted lines.
    session.frame(900, 600);
    session.input(button(end, SessionButtonState::Pressed));
    session.input(movement(-100.0));
    assert_eq!(session.retained_selection(), Some(offer.clone()));
    session.pointer_move(x + 200.0, y + 47.0);
    session.pointer_up(x + 200.0, y + 47.0);
    let multiline = session.retained_selection().unwrap();
    assert_eq!((multiline.start, multiline.end), (21, source.len()));

    // Pressing a toolbar control while the editor has focus must not start
    // a text drag, and its release must not activate it a second time.
    session.frame(900, 600);
    let save = session
        .layout
        .first_selector_point(
            &session.dom.borrow(),
            &taproot::Selector::role("button").containing("Save"),
        )
        .unwrap();
    session.pointer_down(save.0, save.1);
    assert!(session.text_drag.is_none());
    let selection = session.runner.state().editor.selection();
    session.pointer_move(start, line);
    session.pointer_up(start, line);
    assert_eq!(session.runner.state().editor.selection(), selection);
    assert!(session.runner.state().requests.is_empty());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn saved_selection_from_neutral_keyboard_input_keeps_exact_unicode_bytes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("selection.djot");
    let source = "private prefix;\ncafé\nprivate suffix";
    fs::write(&path, source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory {
            root: root.path().into(),
        },
        None,
        None,
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let mut session = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (900, 600),
        false,
        false,
        false,
    );
    focus_editor(&mut session);
    session.key_input(
        SessionKey::Character("a".into()),
        SessionButtonState::Pressed,
        SessionModifiers {
            control: true,
            ..Default::default()
        },
        false,
    );
    session.key_input(
        SessionKey::ArrowLeft,
        SessionButtonState::Pressed,
        Default::default(),
        false,
    );
    for _ in 0..16 {
        session.key_input(
            SessionKey::ArrowRight,
            SessionButtonState::Pressed,
            Default::default(),
            false,
        );
    }
    for _ in 0..4 {
        session.key_input(
            SessionKey::ArrowRight,
            SessionButtonState::Pressed,
            SessionModifiers {
                shift: true,
                ..Default::default()
            },
            false,
        );
    }
    let offer = session.retained_selection().expect("saved selected range");
    assert_eq!((offer.start, offer.end), (16, 21));
    let (export, backing) = session.selected_retained_export(&offer).unwrap();
    assert_eq!(export["text"], "café");
    assert_eq!(backing, source.as_bytes());
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn retained_selection_is_bound_to_the_saved_editor_offer() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("selection.knot");
    let source = "private café source";
    fs::write(&path, source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory {
            root: root.path().into(),
        },
        None,
        None,
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let mut session = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (900, 600),
        false,
        false,
        false,
    );
    assert!(session.retained_selection().is_none());
    session.runner.update(|state| {
        state.editor.apply(cambium::TextCommand::SelectAll);
    });
    let offer = session.retained_selection().expect("saved selected source");
    let (export, backing) = session.selected_retained_export(&offer).unwrap();
    assert_eq!(export["text"], source);
    assert_eq!(backing, source.as_bytes());
    focus_editor(&mut session);
    type_text(&mut session, "changed source");
    assert!(session.retained_selection().is_none());
    assert!(session.selected_retained_export(&offer).is_err());
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        source,
        "retention never writes the source owner"
    );
}
