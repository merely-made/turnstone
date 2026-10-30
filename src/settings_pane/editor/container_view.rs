// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn container_toggle_controls(state: &SettingsState) -> SettingsView {
    let controls = containers(&state.arrangement.accepted().placement)
        .into_iter()
        .map(|container| {
            let expanded = !state.collapsed.contains(&container);
            let label = format!(
                "{} {}",
                if expanded { "Collapse" } else { "Reveal" },
                container_label(&container)
            );
            let action_container = container.clone();
            let view = el::<_, SettingsState, ()>("button", label.clone())
                .attr("type", "button")
                .attr("data-action", "toggle-container")
                .attr("data-component", "")
                .attr(
                    "data-container-kind",
                    container_kind_name(&container).to_ascii_lowercase(),
                )
                .attr("data-container-key", container.key.clone())
                .attr("aria-label", label)
                .attr("aria-expanded", if expanded { "true" } else { "false" });
            let view = clickable(view, move |state: &mut SettingsState, _: PointerClick| {
                state
                    .editor
                    .enqueue(EditorIntent::ToggleContainer(action_container.clone()));
            });
            Box::new(view) as SettingsView
        })
        .collect::<Vec<_>>();
    Box::new(
        el::<_, SettingsState, ()>("div", controls).attr("class", "settings-container-toggles"),
    )
}
