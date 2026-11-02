// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn button(
    label: impl Into<String>,
    callback: impl Fn(&mut AnswerState, PointerClick) + 'static,
) -> AnswerView {
    Box::new(clickable(
        el::<_, AnswerState, ()>("button", label.into()).attr("class", "answer-button"),
        callback,
    ))
}
