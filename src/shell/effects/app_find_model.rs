// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(in super::super) fn app_find_model(
    state: inker::DocumentFindState,
) -> crate::action::DocumentFindModel {
    crate::action::DocumentFindModel {
        count: state.count,
        matches: state
            .matches
            .into_iter()
            .map(|item| crate::action::DocumentFindMatch {
                label: item.label,
                role: item.role,
            })
            .collect(),
        current: state.current.filter(|index| *index < state.count),
        complete: state.complete,
    }
}
