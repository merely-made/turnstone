// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn settings_view(state: &SettingsState) -> SettingsView {
    let body = composition::managed_rows(state);
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            (
                el::<_, SettingsState, ()>("div", "Application settings")
                    .attr("class", "list-section-title"),
                editor::editor_toolbar(state),
                editor::container_toggle_controls(state),
                arrangement_manager(state),
                el::<_, SettingsState, ()>("div", APPLICATION_REFERENCE)
                    .attr("class", "list-row muted"),
                status_note::<SettingsState, ()>(
                    "list-row muted",
                    state.provider.appearance_notice(),
                ),
                button(
                    "Edit themes",
                    |state: &mut SettingsState, _: cambium::PointerClick| {
                        state
                            .live_settings
                            .request_action(crate::action::Action::OpenThemeWorkshop);
                    },
                )
                .attr("class", "setting-apply")
                .attr("data-action", "edit-themes"),
                button(
                    "Discard edits and reload",
                    |state: &mut SettingsState, _: cambium::PointerClick| {
                        commands::reload_settings(state);
                    },
                )
                .attr("class", "setting-reload")
                .attr("data-action", "reload-settings"),
                el::<_, SettingsState, ()>("div", Keyed::new(vec![(state.reload_epoch, body)])),
                status_note::<SettingsState, ()>(
                    "list-row muted",
                    state
                        .live_settings
                        .notice()
                        .unwrap_or_else(|| state.status.clone()),
                ),
            ),
        )
        .attr("class", "pane")
        .attr(
            "style",
            format!(
                "width: {}px; height: {}px;",
                state.viewport_w, state.viewport_h
            ),
        ),
    )
}

fn arrangement_manager(state: &SettingsState) -> SettingsView {
    if !state.editor.enabled {
        return Box::new(el::<_, SettingsState, ()>("div", ()));
    }
    let rows = state
        .arrangement
        .accepted()
        .components
        .iter()
        .map(|descriptor| {
            let label = state
                .rows
                .get(&descriptor.key)
                .map(|row| row.session.snapshot().observed_spec.label)
                .unwrap_or_else(|| descriptor.kind.0.clone());
            (
                descriptor.key.clone(),
                Box::new(
                    el::<_, SettingsState, ()>(
                        "div",
                        (
                            el::<_, SettingsState, ()>("div", label),
                            editor::row_controls(state, &descriptor.key),
                        ),
                    )
                    .attr("class", "settings-arrangement-item"),
                ) as SettingsView,
            )
        })
        .collect::<Vec<_>>();
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            (
                editor::container_controls(state),
                el::<_, SettingsState, ()>("div", Keyed::new(rows)),
            ),
        )
        .attr("class", "settings-arrangement-manager"),
    )
}
