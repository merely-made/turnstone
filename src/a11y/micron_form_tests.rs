// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::{Action, Effect};
#[test]
fn micron_host_editor_exposes_current_field_and_prompt_without_secrets() {
    let mut app = App::test_stub();
    let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
    let source = "`[Submit`:/page/capture.mu`name|secret]\nName: `<name`seed>\nSecret: `<!|secret`mask-seed>\n";
    app.update(Action::OpenAddress(address.into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    app.content.note_fetched(
        member,
        address.into(),
        crate::content::FetchedDocument {
            bytes: source.as_bytes().to_vec(),
            content_type: None,
            body: source.into(),
            effective_url: Some(address.into()),
            acquired_at_ms: 1,
        },
        source.len(),
    );
    app.update(Action::ComposeFocusedMicronForm);
    let focus = host_input_focus(&app).expect("the active editor owns host focus");
    let text = app.omnibar.text.clone();
    assert!(focus_host_input(&mut app, focus));
    assert_eq!(
        app.omnibar.text, text,
        "Focus does not reset or submit the form"
    );
    let host = project_app(&app).to_tree_update(host_input_focus(&app));
    let mut publication = crate::foreign_a11y::ForeignA11y::new();
    let safe = publication.initial_host(host.clone(), &[]).unwrap();
    let consumer = accesskit_consumer::Tree::new(safe, true);
    assert_eq!(
        consumer.state().focus().unwrap().label().as_deref(),
        Some("name")
    );
    assert!(
        host.nodes
            .iter()
            .find(|(id, _)| *id == focus)
            .unwrap()
            .1
            .supports_action(accesskit::Action::Focus)
    );
    publication.frame(host, &[]).unwrap();
    assert!(
        a11y_lines(&app)
            .iter()
            .any(|line| line == "textinput: name = seed")
    );
    assert!(
        a11y_lines(&app)
            .iter()
            .any(|line| line.contains("Enter to continue"))
    );
    app.update(Action::OmnibarCommit);
    let tree = project_app(&app);
    let host = tree.to_tree_update(host_input_focus(&app));
    let safe = publication.initial_host(host.clone(), &[]).unwrap();
    let consumer = accesskit_consumer::Tree::new(safe, true);
    let focused = consumer.state().focus().unwrap();
    assert_eq!(focused.label().as_deref(), Some("secret"));
    assert_eq!(focused.role(), Role::PasswordInput);
    assert_ne!(focused.value().as_deref(), Some("mask-seed"));
    publication.frame(host, &[]).unwrap();
    assert!(
        tree.nodes
            .iter()
            .any(|(_, n)| n.role() == Role::PasswordInput && n.label() == Some("secret"))
    );
    assert!(!a11y_lines(&app).join("\n").contains("mask-seed"));
    app.update(Action::OmnibarCommit);
    assert!(
        a11y_lines(&app)
            .iter()
            .any(|line| line.contains("type send and press Enter"))
    );
    app.update(Action::OmnibarInsert("send".into()));
    let effects = app.update(Action::OmnibarCommit);
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::SubmitMicron { .. }))
            .count(),
        1
    );
}
