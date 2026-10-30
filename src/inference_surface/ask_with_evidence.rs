// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn ask_with_evidence(state: &mut AnswerState, _: PointerClick) {
    ask_format(state, true);
}
