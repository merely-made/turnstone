// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn retained_semantics_bounds_and_pointer_visibility_share_ancestor_clips() {
    let mut dom = ScriptedDom::new();
    let document = dom.document();
    dom.set_inner_html(
        document,
        r#"<div class="clip"><button>Clipped control</button></div>"#,
    );
    let sheet = ".clip { position:absolute; left:20px; top:20px; width:80px; height:40px; border:4px solid red; overflow:hidden; transform:translate(10px,5px); } button { position:absolute; left:30px; top:30px; width:80px; height:30px; padding:0; border:0; }";
    let mut layout = RetainedLayout::new();
    let _ = layout.scene(&mut dom, sheet, 200, 120);
    let button = taproot::matching_estimated_fixture(&dom, &taproot::Selector::role("button"))[0];
    let painted = layout.painted_rect(&dom, button).unwrap();
    let visible = layout.visible_rect(&dom, button).unwrap();
    assert!(
        visible.2 < painted.2 && visible.3 < painted.3,
        "ancestor clips the control"
    );
    let projection = layout.document_projection(&dom, None).unwrap();
    let node = projection
        .nodes()
        .iter()
        .find(|node| node.id.get() == dom.opaque_id(button))
        .unwrap();
    let bounds = node.bounds.unwrap();
    assert_eq!((bounds.x, bounds.y, bounds.width, bounds.height), painted);
    let (tree, _) = layout.accessibility_tree(&dom, None).unwrap();
    let access = tree
        .nodes
        .iter()
        .find(|(id, _)| id.0 == dom.opaque_id(button))
        .unwrap();
    assert_eq!(
        access.1.bounds().unwrap(),
        accesskit::Rect::new(
            painted.0 as f64,
            painted.1 as f64,
            (painted.0 + painted.2) as f64,
            (painted.1 + painted.3) as f64
        )
    );
    let outside = (painted.0 + painted.2 - 1.0, painted.1 + painted.3 - 1.0);
    assert_ne!(
        layout.hit_test(&mut dom, sheet, 200, 120, outside.0, outside.1),
        Some(button)
    );
    assert_eq!(
        layout.hit_test(
            &mut dom,
            sheet,
            200,
            120,
            visible.0 + visible.2 / 2.0,
            visible.1 + visible.3 / 2.0
        ),
        Some(button)
    );
}

#[test]
fn css_hidden_control_is_an_authoritative_semantic_and_visible_miss() {
    let mut dom = ScriptedDom::new();
    let document = dom.document();
    dom.set_inner_html(document, "<button>Hidden control</button>");
    let mut layout = RetainedLayout::new();
    let _ = layout.scene(
        &mut dom,
        "button { visibility:hidden; width:80px; height:30px; }",
        200,
        120,
    );
    let button = taproot::matching_estimated_fixture(&dom, &taproot::Selector::role("button"))[0];
    assert!(layout.visible_rect(&dom, button).is_none());
    let projection = layout.document_projection(&dom, None).unwrap();
    assert!(
        taproot::matching_with_projection(
            &dom,
            &taproot::Selector::role("button").containing("Hidden control"),
            &projection
        )
        .is_empty()
    );
}

#[test]
fn retained_selector_prefers_the_accessible_name_and_its_painted_owner() {
    let mut dom = ScriptedDom::new();
    let document = dom.document();
    dom.set_inner_html(
            document,
            r#"<button style="width:80px;height:30px">Open selected in workbench</button><span id="label">Open selected</span><button aria-labelledby="label" aria-label="Raw label" style="width:120px;height:30px">Raw text</button>"#,
        );
    let mut layout = RetainedLayout::new();
    let _ = layout.scene(&mut dom, "", 400, 160);
    let selector = taproot::Selector::role("button").containing("Open selected");
    let matches = layout.matching_nodes(&dom, &selector);
    assert_eq!(matches.len(), 1, "the exact accessible name wins");
    let painted = layout.visible_rect(&dom, matches[0]).unwrap();
    assert_eq!(
        layout.selector_point(&dom, &selector).unwrap(),
        Some((painted.0 + painted.2 / 2.0, painted.1 + painted.3 / 2.0))
    );
    assert!(
        layout
            .matching_nodes(
                &dom,
                &taproot::Selector::role("button").containing("Raw label")
            )
            .is_empty(),
        "a superseded DOM label cannot retarget the click"
    );
}
