// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use tabard_desktop::harness;
use tabard_workshop::WorkshopState;
use taproot::Selector;

#[test]
fn native_text_input_updates_the_same_draft_and_discard_restores_its_baseline() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("themes.json");
    let mut host = harness(WorkshopState::load(&library).unwrap());
    let original = host.state().draft_theme().name.clone();

    assert!(host.click_on(&Selector::role("textbox").with_attr("data-field", "name")));
    host.key_injected(" authored");
    assert_ne!(host.state().draft_theme().name, original);
    assert!(host.state().draft_theme().name.contains("authored"));
    assert!(host.click_on(&Selector::role("button").with_attr("data-action", "discard")));
    assert_eq!(host.state().draft_theme().name, original);
    assert!(!library.exists(), "discarding a draft does not publish it");
}

#[test]
fn narrow_workshop_wheel_reaches_the_preview_specimens() {
    let mut host = harness(WorkshopState::in_memory());
    host.layout_at(640.0, 780.0);
    let chrome = host.with_dom(|dom| {
        taproot::matching(dom, &Selector::class("chrome-specimen"))
            .into_iter()
            .next()
            .expect("the shared surface has an application-chrome specimen")
    });
    let before = host.painted_rect(chrome).unwrap();
    assert!(
        before.1 >= 780.0,
        "the stacked preview starts below the editor"
    );
    host.move_to(320.0, 350.0);
    host.wheel(0.0, before.1 - 180.0);
    let after = host.painted_rect(chrome).unwrap();
    assert!(
        host.element_scroll_total() > 0.0,
        "the real host wheel default scrolled the body"
    );
    assert!(after.1 < before.1);
    assert!(
        after.1 < 780.0 && after.1 + after.3 > 100.0,
        "the preview enters the visible body"
    );
}
