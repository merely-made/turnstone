// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A pane-shaped document: a root sized to the viewport with rows inside.
fn pane_dom(rows: usize) -> ScriptedDom {
    let mut dom = ScriptedDom::new();
    let root = dom.create_element(qual("div"));
    dom.set_attribute(root, qual("class"), "pane");
    dom.set_attribute(root, qual("style"), "width: 400px; height: 300px;");
    for i in 0..rows {
        let row = dom.create_element(qual("div"));
        dom.set_attribute(row, qual("class"), "grid-cell");
        let text = dom.create_text(&format!("row {i}"));
        dom.append_child(row, text);
        dom.append_child(root, row);
    }
    let document = dom.document();
    dom.append_child(document, root);
    dom
}

#[test]
fn repainting_an_unchanged_pane_does_not_rebuild_its_layout() {
    // The whole point, stated as a count. Every pane used to construct a
    // fresh layout state per paint, which measured 24 ms a frame on
    // the Device Receipts pane; a regression here is that cost returning.
    let mut dom = pane_dom(40);
    let mut retained = RetainedLayout::new();
    for _ in 0..10 {
        let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 400, 300);
    }
    assert_eq!(
        retained.rebuilds(),
        1,
        "ten identical frames must build one layout, not ten"
    );
}

#[test]
fn a_resized_pane_rebuilds_once_and_then_settles() {
    // Size is the one input the incremental path cannot absorb, so a
    // resize must reflow. It must also not keep reflowing afterwards.
    let mut dom = pane_dom(8);
    let mut retained = RetainedLayout::new();
    let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 400, 300);
    let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 800, 600);
    assert_eq!(retained.rebuilds(), 2, "a new size reflows");
    for _ in 0..5 {
        let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 800, 600);
    }
    assert_eq!(retained.rebuilds(), 2, "and then settles at the new size");
}

#[test]
fn a_structural_change_rebuilds_but_an_attribute_change_does_not() {
    // The split the incremental path rests on: adding a node reflows,
    // restyling one does not.
    let mut dom = pane_dom(4);
    let mut retained = RetainedLayout::new();
    let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 400, 300);
    assert_eq!(retained.rebuilds(), 1);

    let first_row = dom.all_with_class(dom.document(), "grid-cell")[0];
    dom.set_attribute(first_row, qual("class"), "grid-row-selected");
    let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 400, 300);
    assert_eq!(
        retained.rebuilds(),
        1,
        "an attribute batch restyles in place"
    );

    let root = dom.all_with_class(dom.document(), "pane")[0];
    let extra = dom.create_element(qual("div"));
    dom.set_attribute(extra, qual("class"), "grid-cell");
    dom.append_child(root, extra);
    let _ = retained.scene(&mut dom, CAMBIUM_SHEET, 400, 300);
    assert_eq!(retained.rebuilds(), 2, "a new node reflows");
}

#[test]
fn a_rebuild_keeps_the_pane_where_it_was_scrolled() {
    // A list pane's content changes on every refresh, which is a
    // structural batch and so a rebuild. Losing the offset there would
    // snap the reader back to the top each time the list updated.
    let mut dom = pane_dom(200);
    let mut retained = RetainedLayout::new();
    let mut scroll = PaneScroll::new();
    // Downwards. Nudging up from the top clamps to zero, which would make
    // this pass for the wrong reason: an offset of zero survives anything.
    scroll.nudge(0.0, 400.0);
    let _ = retained.scene_scrolled(&mut dom, CAMBIUM_SHEET, 400, 300, &mut scroll);
    let settled = scroll.offset();
    assert!(
        settled.1 > 0.0,
        "the fixture actually scrolled: {settled:?}"
    );

    let root = dom.all_with_class(dom.document(), "pane")[0];
    let extra = dom.create_element(qual("div"));
    dom.set_attribute(extra, qual("class"), "grid-cell");
    dom.append_child(root, extra);
    let _ = retained.scene_scrolled(&mut dom, CAMBIUM_SHEET, 400, 300, &mut scroll);
    assert_eq!(retained.rebuilds(), 2, "the new row forced a rebuild");
    assert_eq!(
        scroll.offset(),
        settled,
        "and the rebuild carried the offset across"
    );
}
