// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn hest_keyboard_command_reaches_the_reducer_and_reports_model_completion() {
    let mut app = crate::app::App::test_stub();
    let action = canvas_shortcut_action(&WinitKey::Character("i".into()), false, false).unwrap();
    let invocation = app.command_invocation(&action).unwrap();
    assert_eq!(invocation.id, "Toggle isometric view");
    let before = app.graph_runtimes.is_isometric();
    let (effects, outcome) = app.invoke_command(&invocation);
    assert_eq!(effects, vec![crate::action::Effect::Redraw]);
    assert_eq!(app.graph_runtimes.is_isometric(), !before);
    assert_eq!(outcome, Some(cambium::Outcome::Done));
}

/// The content scroll-key mapping: the page-scroll keys map, Space pages
/// (down, or up with Shift — the browser convention), and a non-scroll key
/// declines so the caller lets it fall through to the Action path.
#[test]
fn content_scroll_keys_map_page_navigation() {
    let named = |n| WinitKey::Named(n);
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::PageDown), false),
        Some(SessionScrollKey::PageDown)
    );
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::PageUp), false),
        Some(SessionScrollKey::PageUp)
    );
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::Home), false),
        Some(SessionScrollKey::Home)
    );
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::End), false),
        Some(SessionScrollKey::End)
    );
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::ArrowDown), false),
        Some(SessionScrollKey::LineDown)
    );
    // Space pages down; Shift+Space pages up.
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::Space), false),
        Some(SessionScrollKey::PageDown)
    );
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::Space), true),
        Some(SessionScrollKey::PageUp)
    );
    // A non-scroll key is not a content-scroll key: it must fall through to
    // become an Action (e.g. Delete forgets the node).
    assert_eq!(
        content_scroll_key(&named(WinitNamedKey::Delete), false),
        None
    );
    assert_eq!(
        content_scroll_key(&WinitKey::Character("i".into()), false),
        None
    );
}

/// The page-zoom chords, and the claim that keeps them off the page: a
/// focused surface must never also receive Ctrl+F or a zoom chord.
#[test]
fn page_zoom_chords_bind_the_browser_keys_and_are_claimed() {
    let member = uuid::Uuid::nil();
    let action =
        |value: &str| PageZoomChord::for_character(value).map(|chord| chord.action(member));
    assert_eq!(action("="), Some(Action::PageZoomIn { member }));
    assert_eq!(action("+"), Some(Action::PageZoomIn { member }));
    assert_eq!(action("-"), Some(Action::PageZoomOut { member }));
    assert_eq!(action("_"), Some(Action::PageZoomOut { member }));
    assert_eq!(action("0"), Some(Action::PageZoomReset { member }));
    assert_eq!(action("1"), None);
    assert_eq!(action("f"), None);

    for value in [
        "=", "+", "-", "_", "0", "f", "F", "l", "L", "k", "K", "r", "R",
    ] {
        assert!(
            is_claimed_ctrl_chord(&WinitKey::Character(value.into())),
            "ctrl+{value} is claimed before the page sees it"
        );
    }
    assert!(!is_claimed_ctrl_chord(&WinitKey::Character("1".into())));
    assert!(!is_claimed_ctrl_chord(&WinitKey::Named(
        WinitNamedKey::PageDown
    )));
}
