// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn selected(state: &AnswerState) -> Option<&RunV1> {
    state
        .active
        .as_ref()
        .and_then(|id| state.snapshot.runs.iter().find(|r| &r.run == id))
        .or_else(|| state.snapshot.runs.last())
}
