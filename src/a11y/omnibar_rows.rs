// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The open omnibar's suggestion rows, each a button that commits it, with a
/// Keep or Drop button beside each command in the `>` lane. Both run through
/// the same actions a click does. The install review projects on its own.
pub(super) fn omnibar_rows(app: &App) -> Vec<(String, Node, Option<A11yRoute>)> {
    if !app.omnibar.open {
        return Vec::new();
    }
    let keep_states = app
        .omnibar
        .text
        .trim()
        .starts_with('>')
        .then(|| app.command_keep_states());
    let mut rows = Vec::new();
    for (index, suggestion) in app.omnibar.suggestions.iter().enumerate() {
        if crate::chrome_view::is_install_review(suggestion)
            || matches!(
                suggestion,
                crate::ui::Suggestion::Hint(_) | crate::ui::Suggestion::Prompt(_)
            )
        {
            continue;
        }
        let text = crate::chrome_view::row_text(suggestion);
        let path = format!("turnstone/chrome/omnibar/row/{index}");
        let mut row = Node::new(Role::Button);
        row.set_label(text);
        row.set_selected(index == app.omnibar.selected);
        rows.push((
            path.clone(),
            row,
            Some(A11yRoute::Action(crate::action::Action::OmnibarCommitRow(
                index,
            ))),
        ));
        if let crate::ui::Suggestion::Act { label, action } = suggestion
            && *action != crate::action::Action::OmnibarShowAllCommands
            && let Some(kept) = keep_states.as_ref().and_then(|states| states.of(label))
        {
            let mut toggle = Node::new(Role::Button);
            toggle.set_label(format!("{} {label}", if kept { "Drop" } else { "Keep" }));
            rows.push((
                format!("{path}/keep"),
                toggle,
                Some(A11yRoute::Action(
                    crate::action::Action::OmnibarToggleKeepRow(index),
                )),
            ));
        }
    }
    rows
}
