// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn row(text: impl Into<String>) -> AnswerView {
    Box::new(el::<_, AnswerState, ()>("div", text.into()))
}
