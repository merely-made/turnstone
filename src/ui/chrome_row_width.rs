// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// What `text` measures when the chrome sheet lays it out as a suggestion
/// row, in px at zoom 1.
///
/// Ordinary rows are `white-space: nowrap; overflow: hidden`, so a row that
/// does not fit is not wrapped or ellipsized: it is silently cut off mid-word. A
/// character count cannot see that, because glyph advances differ (`i` and
/// `W` are one character each and nothing like one width), which is how a
/// review row naming a watch passed a `< 96`-char guard and still rendered as
/// "wakes on: fi". The review row now wraps as an explicit exception; this
/// still lays text out unconstrained and reports the extent it would need on
/// one line, so guards and receipts can compare it with [`ROW_TEXT_BUDGET`].
pub(crate) fn chrome_row_width(text: &str) -> f32 {
    let mut dom = ScriptedDom::new();
    let root = dom.document();
    let row = dom.create_element(qual("div"));
    dom.set_attribute(row, qual("class"), "omni-row");
    // Wide enough that nothing clips: the question is what the run WANTS,
    // and `overflow: hidden` would otherwise cull the answer.
    dom.set_attribute(row, qual("style"), "position: absolute; width: 8000px;");
    let t = dom.create_text(text);
    dom.append_child(row, t);
    dom.append_child(root, row);

    let plist = livery_paint_list(&dom, CHROME_SHEET, 8192, 600);
    let mut right = 0f32;
    for cmd in plist.commands() {
        if let paint_list_api::PaintCmd::DrawText(run) = cmd {
            for glyph in &run.glyphs {
                right = right.max(run.placement.bounds.min.x + glyph.point.x);
            }
        }
    }
    // The last glyph's ORIGIN is not its right edge; one em is a generous
    // stand-in for the advance the glyph list does not carry.
    let start = 8.0; // the row's own left padding, which the bounds include
    (right - start + 14.0).max(0.0)
}
