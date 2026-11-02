// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn keyboard_event(kind: native::KeyEventKind) -> native::KeyboardInput {
    native::KeyboardInput {
        kind,
        virtual_key_code: 0x41,
        characters: "A".into(),
        characters_ignoring_modifiers: "a".into(),
        modifiers: native::KeyModifierFlags {
            shift: true,
            control: true,
            alt: true,
            meta: true,
            caps_lock: false,
        },
        is_repeat: true,
    }
}

fn command_json(command: &str) -> serde_json::Value {
    serde_json::from_str(command).unwrap()
}

#[test]
fn scry_keyboard_preserves_down_char_up_modifiers_and_repeat() {
    let down = keyboard_event(native::KeyEventKind::Down);
    let commands = keyboard_commands(&down).unwrap();
    assert_eq!(commands.len(), 2);
    let key_down = command_json(&commands[0]);
    let character = command_json(&commands[1]);
    assert_eq!(key_down["type"], "rawKeyDown");
    assert!(key_down.get("text").is_none());
    assert_eq!(character["type"], "char");
    assert_eq!(character["text"], "A");
    assert_eq!(character["unmodifiedText"], "a");
    for command in [&key_down, &character] {
        assert_eq!(command["key"], "a");
        assert_eq!(command["code"], "KeyA");
        assert_eq!(command["windowsVirtualKeyCode"], 0x41);
        assert_eq!(command["nativeVirtualKeyCode"], 0x41);
        assert_eq!(command["modifiers"], 15);
        assert_eq!(command["autoRepeat"], true);
    }
    let up = keyboard_commands(&keyboard_event(native::KeyEventKind::Up)).unwrap();
    assert_eq!(up.len(), 1);
    let up = command_json(&up[0]);
    assert_eq!(up["type"], "keyUp");
    assert!(up.get("text").is_none());

    let mut plain = down;
    plain.characters.clear();
    assert_eq!(keyboard_commands(&plain).unwrap().len(), 1);
    plain.characters = "\"\\\nλ🎵".into();
    let commands = keyboard_commands(&plain).unwrap();
    assert_eq!(command_json(&commands[1])["text"], plain.characters);
}

#[test]
fn scry_keyboard_modifier_changes_keep_direction_and_named_keys() {
    for (code, key, modifier, bits) in [
        (
            0x10,
            "Shift",
            native::KeyModifierFlags {
                shift: true,
                ..Default::default()
            },
            8,
        ),
        (
            0x11,
            "Control",
            native::KeyModifierFlags {
                control: true,
                ..Default::default()
            },
            2,
        ),
        (
            0x12,
            "Alt",
            native::KeyModifierFlags {
                alt: true,
                ..Default::default()
            },
            1,
        ),
        (
            0x5B,
            "Meta",
            native::KeyModifierFlags {
                meta: true,
                ..Default::default()
            },
            4,
        ),
    ] {
        let mut event = keyboard_event(native::KeyEventKind::ModifiersChanged);
        event.virtual_key_code = code;
        event.modifiers = modifier;
        let down = keyboard_commands(&event).unwrap();
        assert_eq!(down.len(), 1);
        assert_eq!(command_json(&down[0])["type"], "rawKeyDown");
        assert_eq!(command_json(&down[0])["key"], key);
        assert_eq!(command_json(&down[0])["modifiers"], bits);
        event.modifiers = Default::default();
        assert_eq!(
            command_json(&keyboard_commands(&event).unwrap()[0])["type"],
            "keyUp"
        );
    }
    for (code, key, physical_code) in [
        (0x0D, "Enter", "Enter"),
        (0x08, "Backspace", "Backspace"),
        (0x09, "Tab", "Tab"),
        (0x1B, "Escape", "Escape"),
        (0x20, " ", "Space"),
        (0x37, "7", "Digit7"),
    ] {
        let mut event = keyboard_event(native::KeyEventKind::Down);
        event.virtual_key_code = code;
        let command = command_json(&keyboard_commands(&event).unwrap()[0]);
        assert_eq!(command["key"], key);
        assert_eq!(command["code"], physical_code);
    }
}

#[test]
fn scry_keyboard_queue_waits_for_each_completion_before_next_command() {
    let mut queue = KeyboardDispatchQueue::default();
    queue
        .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Down)).unwrap())
        .unwrap();
    queue
        .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Up)).unwrap())
        .unwrap();
    let now = Instant::now();
    let mut submitted = Vec::new();
    submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
    // Repeated host polls cannot send char or keyUp before rawKeyDown's
    // completion. Completion itself does not dispatch anything.
    assert!(queue.next(now).unwrap().is_none());
    assert_eq!(queue.pending.len(), 2);
    queue.complete(Ok(()));
    assert_eq!(queue.pending.len(), 2);
    submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
    assert!(queue.next(now).unwrap().is_none());
    queue.complete(Ok(()));
    submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
    assert!(queue.next(now).unwrap().is_none());
    queue.complete(Ok(()));
    assert!(queue.next(now).unwrap().is_none());
    assert_eq!(submitted, vec!["rawKeyDown", "char", "keyUp"]);
}

#[test]
fn scry_keyboard_queue_failure_and_timeout_retire_following_commands() {
    for failure in [Some("fixture rejected keyboard CDP"), None] {
        let mut queue = KeyboardDispatchQueue::default();
        queue
            .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Down)).unwrap())
            .unwrap();
        let now = Instant::now();
        assert!(queue.next(now).unwrap().is_some());
        if let Some(reason) = failure {
            queue.complete(Err(reason.into()));
        }
        let result = queue.next(now + KEYBOARD_COMPLETION_TIMEOUT);
        assert!(matches!(result, Err(native::WebSurfaceError::Platform(_))));
        assert!(queue.pending.is_empty());
        assert!(queue.in_flight.is_none());
        // Late completions and newly typed keys cannot resume a retired
        // stream; the shell observes this failure through acquire/send.
        queue.complete(Ok(()));
        assert!(queue.enqueue(vec!["late key".into()]).is_err());
        assert!(queue.next(now).is_err());
    }
    assert!(keyboard_completion(Err("native error".into()), "{}").is_err());
    assert!(keyboard_completion(Ok(()), r#"{"error":{"message":"refused"}}"#).is_err());
    assert!(keyboard_completion(Ok(()), "malformed").is_err());
    assert!(keyboard_completion(Ok(()), "{}").is_ok());
}

#[test]
fn scry_keyboard_completion_does_not_retain_retired_producer_state() {
    let queue = Rc::new(RefCell::new(KeyboardDispatchQueue::default()));
    let weak = Rc::downgrade(&queue);
    queue
        .borrow_mut()
        .enqueue(vec!["pending command".into()])
        .unwrap();
    assert!(queue.borrow_mut().next(Instant::now()).unwrap().is_some());
    drop(queue);
    assert!(weak.upgrade().is_none());
}

fn mouse_event(
    phase: PointerPhase,
    button: Option<MouseButton>,
    buttons: PointerButtons,
) -> PointerEvent {
    PointerEvent {
        pointer_id: 1,
        pointer_type: PointerType::Mouse,
        is_primary: true,
        phase,
        button,
        buttons,
        position: inker::PhysicalPosition { x: 12.0, y: 34.0 },
        width: 1.0,
        height: 1.0,
        pressure: None,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        altitude_angle: None,
        azimuth_angle: None,
        modifiers: inker::KeyboardModifiers {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
        },
    }
}

#[test]
fn scry_mouse_buttons_keep_changed_button_and_post_event_state() {
    let right = mouse_pointer_input(&mouse_event(
        PointerPhase::Down,
        Some(MouseButton::Right),
        PointerButtons::PRIMARY | PointerButtons::SECONDARY,
    ))
    .unwrap();
    assert_eq!(right.kind, native::MouseEventKind::RightButtonDown);
    assert_eq!(right.point, (12, 34));
    assert!(right.virtual_keys.left_button && right.virtual_keys.right_button);
    assert!(right.virtual_keys.shift && right.virtual_keys.control);

    let middle_up = mouse_pointer_input(&mouse_event(
        PointerPhase::Up,
        Some(MouseButton::Middle),
        PointerButtons::SECONDARY,
    ))
    .unwrap();
    assert_eq!(middle_up.kind, native::MouseEventKind::MiddleButtonUp);
    assert!(!middle_up.virtual_keys.middle_button);
    assert!(middle_up.virtual_keys.right_button);
    assert!(middle_up.virtual_keys.shift && middle_up.virtual_keys.control);

    let left_up = mouse_pointer_input(&mouse_event(
        PointerPhase::Up,
        Some(MouseButton::Left),
        PointerButtons::NONE,
    ))
    .unwrap();
    assert_eq!(left_up.kind, native::MouseEventKind::LeftButtonUp);
    assert!(!left_up.virtual_keys.left_button);
    assert!(
        mouse_pointer_input(&mouse_event(PointerPhase::Down, None, PointerButtons::NONE)).is_err()
    );
}

#[test]
fn scry_mouse_move_leave_and_auxiliary_buttons_use_native_mouse_route() {
    let moved = mouse_pointer_input(&mouse_event(
        PointerPhase::Move,
        None,
        PointerButtons::PRIMARY,
    ))
    .unwrap();
    assert_eq!(moved.kind, native::MouseEventKind::Move);
    assert!(moved.virtual_keys.left_button);
    let leave = mouse_pointer_input(&mouse_event(
        PointerPhase::Cancel,
        None,
        PointerButtons::NONE,
    ))
    .unwrap();
    assert_eq!(leave.kind, native::MouseEventKind::Leave);
    assert_eq!(leave.point, (12, 34));
    for (button, index, mask) in [
        (MouseButton::Back, 1, PointerButtons::BACK),
        (MouseButton::Forward, 2, PointerButtons::FORWARD),
    ] {
        let down =
            mouse_pointer_input(&mouse_event(PointerPhase::Down, Some(button), mask)).unwrap();
        assert_eq!(down.kind, native::MouseEventKind::XButtonDown);
        assert_eq!(down.mouse_data, index);
        assert_eq!(down.virtual_keys.x_button1, index == 1);
        assert_eq!(down.virtual_keys.x_button2, index == 2);
        let up = mouse_pointer_input(&mouse_event(
            PointerPhase::Up,
            Some(button),
            PointerButtons::NONE,
        ))
        .unwrap();
        assert_eq!(up.kind, native::MouseEventKind::XButtonUp);
        assert_eq!(up.mouse_data, index);
        assert!(!up.virtual_keys.x_button1 && !up.virtual_keys.x_button2);
    }
}

#[test]
fn scry_profile_requires_explicit_absolute_path() {
    assert!(profile_path("").is_err());
    assert!(profile_path("relative-profile").is_err());
    let path = PathBuf::from(r"C:\turnstone-data\scry\profiles\node-7");
    assert_eq!(profile_path(path.to_str().unwrap()).unwrap(), path);
}

#[test]
fn scry_factory_refuses_other_threads_before_native_construction() {
    let owner = std::thread::current().id();
    assert!(require_owner_thread(owner).is_ok());
    let result = std::thread::spawn(move || require_owner_thread(owner))
        .join()
        .unwrap();
    assert!(matches!(result, Err(SurfaceError::SpawnFailed(reason))
            if reason.contains("UI thread")));
}

#[test]
fn scry_host_capabilities_refuse_unforwarded_adapter_commands() {
    let mut caps = WebSurfaceCapabilities::default();
    caps.document.find_in_page = WebFeatureStatus::Supported;
    caps.document.page_zoom = WebFeatureStatus::Supported;
    caps.auth = WebFeatureStatus::Supported;
    caps.permissions = WebFeatureStatus::Supported;
    caps.pdf = WebFeatureStatus::Supported;
    caps.ime_observability = WebFeatureStatus::Supported;
    caps.accessibility = WebFeatureStatus::Supported;
    caps.script.result = WebFeatureStatus::Supported;
    caps.cookie.read = WebFeatureStatus::Supported;
    let caps = host_capabilities(caps);
    for status in [
        caps.document.find_in_page,
        caps.document.page_zoom,
        caps.document.page_capture,
        caps.auth,
        caps.permissions,
        caps.pdf,
        caps.ime_observability,
        caps.accessibility,
    ] {
        assert!(matches!(status, WebFeatureStatus::Unsupported { reason }
                if !reason.is_empty()));
    }
    assert_eq!(caps.script.result, WebFeatureStatus::Supported);
    assert_eq!(caps.cookie.read, WebFeatureStatus::Supported);
}

#[test]
fn scry_host_capabilities_preserve_refusal_and_partial_event_limits() {
    let mut caps = WebSurfaceCapabilities::default();
    caps.downloads = WebFeatureStatus::unsupported("backend denied downloads");
    caps.popups = WebFeatureStatus::Partial {
        detail: "backend popup limit".into(),
    };
    caps.context_menus = WebFeatureStatus::Supported;
    let caps = host_capabilities(caps);
    assert!(
        matches!(caps.downloads, WebFeatureStatus::Unsupported { reason }
            if reason == "backend denied downloads")
    );
    assert!(matches!(caps.popups, WebFeatureStatus::Partial { detail }
            if detail.contains("backend popup limit") && detail.contains("popup-widget")));
    assert!(
        matches!(caps.context_menus, WebFeatureStatus::Partial { detail }
            if detail.contains("menu items and decisions"))
    );
}
