// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The same semantic view and tool verbs remain available to assistive input
/// without depending on whether the responsive toolbar currently has room.
pub(super) fn toolbar_actions(app: &App) -> Vec<(String, String, crate::action::Action)> {
    use crate::action::Action;
    if !app.shell_chrome_config().projects_shellbar() {
        return Vec::new();
    }
    let mut actions = vec![
        (
            "turnstone/chrome/read".into(),
            "Read document".into(),
            Action::SetWorkspaceMap(false),
        ),
        (
            "turnstone/chrome/map".into(),
            "Reveal document in map".into(),
            Action::SetWorkspaceMap(true),
        ),
    ];
    for (_, kind) in crate::panes::pane_palette_entries() {
        if matches!(
            kind.as_str(),
            crate::panes::kind::GRAPH | crate::panes::kind::WORKBENCH
        ) {
            continue;
        }
        if let Some(definition) = crate::panes::pane_definition(kind.as_str()) {
            actions.push((
                format!("turnstone/chrome/panes/{}", definition.id),
                format!("Toggle {} pane", definition.display_name),
                Action::ToggleToolPane(kind),
            ));
        }
    }
    actions
}
