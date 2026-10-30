// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Lay out the same padded, bordered card the retained chrome renders. A
/// detached row has a different containing width and misses the input's size.
pub(super) fn chrome_card_row_metrics(
    text: &str,
    review: bool,
    appearance: &crate::shell_services::AppearanceConfig,
) -> (f32, f32) {
    use cambium::View;
    let dom: cambium::DomHandle = std::rc::Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
    let mut ctx = cambium::GenetCtx::new(dom.clone());
    let input = cambium::el::<_, (), ()>(
        "div",
        cambium::caret_field_children::<(), ()>(&cambium::TextInput::new("x"), &[]),
    )
    .attr("class", "omni-input");
    let row_view = cambium::el::<_, (), ()>("div", text.to_owned()).attr(
        "class",
        if review {
            "omni-row omni-row-review"
        } else {
            "omni-row"
        },
    );
    let view = cambium::el::<_, (), ()>("div", (input, row_view))
        .attr("class", "omni")
        .attr("style", format!("width: {CARD_W}px;"));
    let (card, _state) = view.build(&mut ctx, &mut ());
    let mut dom = dom.borrow_mut();
    let root = dom.document();
    dom.append_child(root, card.node);
    let row = dom.all_with_class(root, "omni-row")[0];
    // The caret is an inline span with its own glyph/font metrics. A plain
    // text stand-in underestimates the live input line and the entire card.
    let snapshot = build_livery_snapshot(
        &*dom,
        &chrome_sheet(appearance),
        CARD_W as u32,
        16_384,
        TextSystem::new(),
    );
    let row_height = snapshot
        .fragments
        .get(row)
        .expect("chrome row is laid out")
        .height;
    let card_height = snapshot
        .fragments
        .get(card.node)
        .expect("chrome card is laid out")
        .height;
    (row_height, card_height - row_height)
}
