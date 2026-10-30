// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A DOM shaped like a real pane: a `.pane` root sized to the viewport
/// with more rows inside than fit.
///
/// The root element matters. CSS propagates the *root's* overflow to the
/// viewport, so a fixture that hangs rows straight off the document has a
/// row as its root and does not scroll at all — which is a property of the
/// fixture, not of the pane it was standing in for.
fn tall_dom() -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let panel = dom.create_element(qual("div"));
    dom.set_attribute(panel, qual("class"), "pane");
    dom.set_attribute(panel, qual("style"), "width: 400px; height: 300px;");
    dom.append_child(root, panel);
    for index in 0..80 {
        let row = dom.create_element(qual("div"));
        dom.set_attribute(row, qual("class"), "list-row");
        let text = dom.create_text(&format!("row {index}"));
        dom.append_child(row, text);
        dom.append_child(panel, row);
    }
    dom
}

#[test]
fn scrolling_past_the_end_clamps_to_the_content() {
    let dom = tall_dom();
    let mut scroll = PaneScroll::new();
    scroll.nudge(0.0, 100_000.0);
    let _ = scene_from_dom_scrolled(&dom, CAMBIUM_SHEET, 400, 300, &mut scroll);
    let (_, y) = scroll.offset();
    assert!(y > 0.0, "the pane scrolled at all, got {y}");
    assert!(
        y < 100_000.0,
        "the offset was clamped to the content, got {y}"
    );
}

/// Scrolling up from the top is a no-op rather than a negative offset.
#[test]
fn scrolling_above_the_top_stays_at_the_top() {
    let dom = tall_dom();
    let mut scroll = PaneScroll::new();
    scroll.nudge(0.0, -500.0);
    let _ = scene_from_dom_scrolled(&dom, CAMBIUM_SHEET, 400, 300, &mut scroll);
    assert_eq!(scroll.offset().1, 0.0);
}

/// A pane that has never been scrolled shows no bar at all; one just
/// scrolled shows it fully. This is the auto-hide, at its two ends.
#[test]
fn bars_are_hidden_until_scrolled() {
    let mut scroll = PaneScroll::new();
    assert_eq!(scroll.alpha(), 0.0, "an untouched pane shows no bar");
    scroll.nudge(0.0, 40.0);
    assert_eq!(scroll.alpha(), 1.0, "a just-scrolled pane shows its bar");
    assert!(scroll.bars_visible(), "and it is still worth repainting");
}

/// The bar is actually emitted into the paint list when the pane is
/// scrolled, and not when it is idle. Asserted on the command count rather
/// than on pixels, so a timing-sensitive screenshot is not the only proof.
#[test]
fn a_scrolled_pane_emits_its_bar() {
    let dom = tall_dom();
    let mut snapshot = build_livery_snapshot(&dom, CAMBIUM_SHEET, 400, 300, TextSystem::new());
    let plain = paint_snapshot(&dom, &mut snapshot, 400, 300, &HashMap::new(), (0.0, 200.0));
    let before = plain.commands().len();

    let mut with_bar = paint_snapshot(&dom, &mut snapshot, 400, 300, &HashMap::new(), (0.0, 200.0));
    append_scrollbars(
        &mut with_bar,
        snapshot.content_extent,
        (400, 300),
        (0.0, 200.0),
        1.0,
    );
    assert!(
        with_bar.commands().len() > before,
        "the bar adds paint commands: {} -> {}",
        before,
        with_bar.commands().len(),
    );

    let mut hidden = paint_snapshot(&dom, &mut snapshot, 400, 300, &HashMap::new(), (0.0, 200.0));
    append_scrollbars(
        &mut hidden,
        snapshot.content_extent,
        (400, 300),
        (0.0, 200.0),
        0.0,
    );
    assert_eq!(
        hidden.commands().len(),
        before,
        "a fully faded bar emits nothing at all",
    );
}

/// A hit lands on the row under the pointer, not the row that would be
/// there unscrolled. The regression this guards is a click activating the
/// wrong thing on any scrolled pane.
#[test]
fn hit_testing_follows_the_scroll() {
    let dom = tall_dom();
    let mut scroll = PaneScroll::new();
    let at_top = hit_test_scrolled(&dom, CAMBIUM_SHEET, 400, 300, 20.0, 40.0, &scroll);
    scroll.nudge(0.0, 200.0);
    let _ = scene_from_dom_scrolled(&dom, CAMBIUM_SHEET, 400, 300, &mut scroll);
    let scrolled = hit_test_scrolled(&dom, CAMBIUM_SHEET, 400, 300, 20.0, 40.0, &scroll);
    assert!(at_top.is_some() && scrolled.is_some(), "both hit a row");
    assert_ne!(
        at_top, scrolled,
        "the same point hits a different row once scrolled"
    );
}
