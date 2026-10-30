// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) const KNOT_SHEET: &str = "\
    .knot-root { background-color: rgb(22, 27, 40); color: rgb(205, 212, 226); } \
    .knot-toolbar { display: flex; background-color: rgb(18, 22, 33); \
                    padding: 6px 10px; } \
    .knot-title { color: rgb(205, 212, 226); font-size: 12px; width: 35%; \
                  white-space: nowrap; overflow: hidden; } \
    .knot-status { color: rgb(140, 153, 176); font-size: 12px; width: 20%; \
                   white-space: nowrap; } \
    .knot-save { color: rgb(232, 150, 40); background-color: rgb(28, 34, 50); \
                 border: 1px solid rgb(52, 62, 86); padding: 3px 10px; } \
    .knot-effect { color: rgb(103, 184, 235); background-color: rgb(28, 34, 50); \
                   border: 1px solid rgb(52, 62, 86); padding: 3px 10px; } \
    .knot-citations { padding: 4px 10px; } \
    .knot-saved-citation { color: rgb(103, 184, 235); background-color: rgb(28, 34, 50); padding: 3px 8px; } \
    .knot-body { display: flex; } \
    .knot-editor-wrap { width: 70%; padding: 10px; } \
    .knot-editor-wrap textarea, .knot-editor-wrap [role=textbox][data-cambium-text-value] { color: rgb(218, 224, 236); \
                                 background-color: rgb(25, 30, 44); \
                                 font-size: 13px; white-space: pre-wrap; \
                                 padding: 10px; border: 1px solid rgb(52, 62, 86); } \
    .knot-readout { width: 30%; padding: 10px; color: rgb(172, 183, 204); \
                    font-size: 12px; white-space: pre-wrap; overflow: hidden; } \
    .syntax-heading { color: rgb(240, 179, 94); } \
    .syntax-link { color: rgb(103, 184, 235); } \
    .syntax-strong { color: rgb(235, 239, 247); } \
    .syntax-emphasis { color: rgb(194, 205, 224); } \
    .syntax-codeblock, .syntax-verbatim { color: rgb(141, 207, 160); } \
    .knot-selection { background-color: rgb(42, 76, 118); color: rgb(240, 245, 255); }";
