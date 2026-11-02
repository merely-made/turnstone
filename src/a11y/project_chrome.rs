// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The chrome subtree: the omnibar (a text input when open, with its live
/// text and caret-free honesty) and the at-rest caption.
pub(super) fn project_chrome(app: &App) -> UxTree {
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    for (path, label, _) in toolbar_actions(app) {
        let id = node_id_for_path(&path);
        let mut node = Node::new(Role::Button);
        node.set_label(label);
        node.add_action(accesskit::Action::Click);
        nodes.push((id, node));
        children.push(id);
    }
    if app.omnibar.open {
        let id = node_id_for_path("turnstone/chrome/omnibar");
        let mut n = Node::new(if app.omnibar.sensitive() {
            Role::PasswordInput
        } else {
            Role::TextInput
        });
        let label = match &app.omnibar.mode {
            crate::ui::OmnibarMode::MicronForm(prompt) => match prompt.stage {
                crate::ui::MicronFormStage::Field(index) => prompt
                    .form
                    .fields()
                    .get(index)
                    .map(|f| f.name())
                    .unwrap_or("Micron form"),
                crate::ui::MicronFormStage::Action => "Micron submit action",
                crate::ui::MicronFormStage::Confirm => "Micron submission confirmation",
            },
            _ => "omnibar",
        };
        n.set_label(label);
        n.add_action(accesskit::Action::Focus);
        n.set_value(app.omnibar.presented_text());
        nodes.push((id, n));
        children.push(id);
        for (index, suggestion) in app.omnibar.suggestions.iter().enumerate() {
            if matches!(
                suggestion,
                crate::ui::Suggestion::Prompt(_) | crate::ui::Suggestion::Hint(_)
            ) {
                let id = node_id_for_path(&format!("turnstone/chrome/prompt/{index}"));
                let mut prompt = Node::new(Role::Status);
                prompt.set_label(crate::chrome_view::row_text(suggestion));
                nodes.push((id, prompt));
                children.push(id);
            }
        }
        for (path, node, _) in omnibar_rows(app) {
            let id = node_id_for_path(&path);
            nodes.push((id, node));
            children.push(id);
        }
        if let Some(review) = app
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| crate::chrome_view::is_install_review(suggestion))
        {
            let id = node_id_for_path("turnstone/chrome/install-review");
            let mut n = Node::new(Role::Button);
            n.set_label(crate::chrome_view::row_text(review));
            nodes.push((id, n));
            children.push(id);
        }
    }
    if app.document_find.open {
        let input_id = node_id_for_path("turnstone/chrome/document-find/input");
        let mut input = Node::new(Role::SearchInput);
        input.set_label("Find in document");
        input.set_value(app.document_find.query());
        nodes.push((input_id, input));
        children.push(input_id);

        let status_id = node_id_for_path("turnstone/chrome/document-find/status");
        let mut status = Node::new(Role::Status);
        status.set_label(app.document_find.status());
        nodes.push((status_id, status));
        children.push(status_id);
    }
    if let Some(decision) = app.user_agent_decision.active() {
        let base = "turnstone/chrome/user-agent-decision";
        let mut decision_children = Vec::new();
        match decision {
            crate::user_agent_decision::PendingUserAgentDecision::Permission { .. } => {
                for (suffix, label) in [
                    ("allow-once", "Allow once"),
                    ("always-allow", "Always allow"),
                    ("deny", "Deny"),
                ] {
                    let id = node_id_for_path(&format!("{base}/{suffix}"));
                    let mut button = Node::new(Role::Button);
                    button.set_label(label);
                    nodes.push((id, button));
                    decision_children.push(id);
                }
            },
            crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. } => {
                let username_id = node_id_for_path(&format!("{base}/username"));
                let mut username = Node::new(Role::TextInput);
                username.set_label("Username");
                username.set_description("credential value withheld from observation");
                nodes.push((username_id, username));
                decision_children.push(username_id);

                let password_id = node_id_for_path(&format!("{base}/password"));
                let mut password = Node::new(Role::PasswordInput);
                password.set_label("Password");
                password.set_description("credential value withheld from observation");
                nodes.push((password_id, password));
                decision_children.push(password_id);

                let remember_id = node_id_for_path(&format!("{base}/remember"));
                let mut remember = Node::new(Role::CheckBox);
                remember.set_label(
                    if app.user_agent_decision.authentication.remember_for_process {
                        "Remember until Turnstone closes, on"
                    } else {
                        "Remember until Turnstone closes, off"
                    },
                );
                nodes.push((remember_id, remember));
                decision_children.push(remember_id);

                for (suffix, label) in [("sign-in", "Sign in"), ("cancel", "Cancel")] {
                    let id = node_id_for_path(&format!("{base}/{suffix}"));
                    let mut button = Node::new(Role::Button);
                    button.set_label(label);
                    nodes.push((id, button));
                    decision_children.push(id);
                }
            },
        }
        if let Some(error) = &app.user_agent_decision.error {
            let error_id = node_id_for_path(&format!("{base}/error"));
            let mut status = Node::new(Role::Status);
            status.set_label(error.clone());
            nodes.push((error_id, status));
            decision_children.push(error_id);
        }
        let id = node_id_for_path(base);
        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(decision.prompt());
        dialog.set_description(format!(
            "request {} of {}",
            1,
            app.user_agent_decision.len()
        ));
        dialog.set_children(decision_children);
        nodes.push((id, dialog));
        children.push(id);
    }
    if app.shell_chrome_config().projects_shellbar()
        && let Some(caption) = crate::app::navigation_caption(app)
    {
        let id = node_id_for_path("turnstone/chrome/caption");
        let mut n = Node::new(Role::Label);
        n.set_label(caption);
        nodes.push((id, n));
        children.push(id);
    }
    let root_id = node_id_for_path("turnstone/chrome");
    let mut root = Node::new(Role::Group);
    root.set_label("chrome");
    root.set_children(children);
    nodes.push((root_id, root));
    UxTree {
        root: root_id,
        nodes,
    }
}
