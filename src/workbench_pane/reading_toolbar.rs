// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn reading_toolbar(state: &WorkbenchState, index: usize) -> WbView {
    let cell = &state.cells[index];
    let Some(target) = cell.target else {
        return Box::new(el("div", ()));
    };
    let mut ui = cell.find.clone();
    ui.available = cell.find_enabled;
    let find = Some(&ui);
    let close = cambium::shell_control(
        "×",
        format!(
            "Close reading: {}",
            cell.labels
                .get(cell.strip.selected)
                .map(String::as_str)
                .unwrap_or("document")
        ),
        Default::default(),
        move |state: &mut WorkbenchState, _| {
            state.read_actions.push(crate::action::Action::Reading(
                crate::reading_view::ReadAction::Close(target),
            ))
        },
    );
    let toolbar = cambium::read_toolbar_with_trailing(
        cell.preferences.text_size,
        cell.preferences.column_em,
        cell.text_controls,
        find,
        if cell.tabs.w > 0.0 && cell.tabs.h > 0.0 {
            Vec::new()
        } else {
            vec![close]
        },
        move |state: &mut WorkbenchState, event| {
            use crate::reading_view::ReadAction;
            let action = match event {
                cambium::ReadEvent::Command(command) => ReadAction::Command(target, command),
                cambium::ReadEvent::Query(input) => ReadAction::Query(target, input),
            };
            state
                .read_actions
                .push(crate::action::Action::Reading(action));
        },
    );
    Box::new(el("div", toolbar).attr(
        "class",
        if cell.tabs.w <= 0.0 {
            "wb-single-reading"
        } else {
            "wb-tabbed-reading"
        },
    ))
}
