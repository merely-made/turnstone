// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn incomplete_tree_waits_and_invalid_host_frame_keeps_publication_pending() {
    let mut foreign = ForeignA11y::new();
    foreign.admit(surface(1), tree(101)).unwrap();
    foreign
        .apply(surface(1), wrapper(tree(101), tree(102)))
        .unwrap();
    assert!(!foreign.ready(surface(1)));
    // A late delta after upstream reactivation is not an initial tree.
    // AAC can carry Some(Tree) while sending only changed nodes; another
    // producer can omit Tree entirely. Neither should poison admission.
    for tree_data in [None, Some(Tree::new(NodeId(0)))] {
        assert!(
            foreign
                .apply(
                    surface(1),
                    TreeUpdate {
                        nodes: vec![(NodeId(1), button("unusable late delta"))],
                        tree: tree_data,
                        tree_id: tree(102),
                        focus: NodeId(1),
                    }
                )
                .is_err()
        );
        assert!(foreign.surfaces.contains_key(&surface(1)));
        assert!(!foreign.ready(surface(1)));
        assert_eq!(foreign.published_counts(), (0, 0, 0));
        assert!(
            foreign
                .action_target(&request(tree(102), 1, Action::Click))
                .is_err()
        );
    }
    assert!(
        foreign
            .graft(surface(1), Rect::new(0.0, 0.0, 100.0, 100.0))
            .is_none()
    );
    let visible = [surface(1)];
    let initial = foreign
        .initial_host(host(&foreign, &visible, 0), &visible)
        .unwrap();
    let mut consumer = ConsumerTree::new(initial, true);
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 0), &visible)
            .unwrap(),
    );
    assert!(consumer.state().subtree_root(tree(101)).is_none());
    foreign
        .apply(surface(1), document(tree(102), "now ready"))
        .unwrap();
    assert!(foreign.needs_publish());
    // A host cannot publish a guest without the matching graft.
    assert!(foreign.frame(host(&foreign, &[], 0), &visible).is_err());
    assert!(foreign.needs_publish());
    assert!(
        foreign
            .action_target(&request(tree(102), 1, Action::Click))
            .is_err()
    );
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert!(consumer.state().subtree_root(tree(102)).is_some());
}

#[test]
fn retirement_and_new_root_reject_old_generation_actions_and_updates() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    foreign.retire(surface(1));
    foreign.admit(surface(1), tree(401)).unwrap();
    assert!(
        foreign
            .apply(surface(1), document(tree(103), "old generation"))
            .is_err()
    );
    foreign
        .apply(surface(1), document(tree(401), "new generation"))
        .unwrap();
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    assert_eq!(
        foreign
            .action_target(&request(tree(401), 1, Action::Click))
            .unwrap(),
        surface(1)
    );
    foreign.clear();
    assert!(!foreign.ready(surface(1)));
    assert!(
        foreign
            .action_target(&request(tree(401), 1, Action::Click))
            .is_err()
    );
}

#[test]
fn publication_diagnostic_uses_emitted_frame_not_pending_supplier_delta() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    assert_eq!(foreign.published_counts(), (1, 3, 6));
    let diagnostic = foreign.publication_diagnostic(true);
    assert_eq!(
        diagnostic["surfaces"][0]["root_tree_id"],
        tree(101).0.to_string()
    );
    let trees = diagnostic["surfaces"][0]["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 3);
    assert!(trees.iter().all(|tree| tree["root_node_id"] == 0));
    assert!(trees.iter().all(|tree| tree["focus_node_id"] == 1));
    foreign
        .apply(
            surface(1),
            TreeUpdate {
                nodes: vec![(NodeId(1), button("pending supplier label"))],
                tree: None,
                tree_id: tree(103),
                focus: NodeId(1),
            },
        )
        .unwrap();
    assert!(!foreign.published_text().contains("pending supplier label"));
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert!(foreign.published_text().contains("pending supplier label"));
    assert_eq!(foreign.published_counts(), (1, 3, 6));
}

#[test]
fn publication_diagnostic_withdraws_hidden_retired_malformed_and_reset_trees() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    send(
        &mut consumer,
        foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
    );
    assert_eq!(foreign.published_counts(), (0, 0, 0));
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert_eq!(foreign.published_counts(), (1, 3, 6));
    foreign.reset_publication();
    assert_eq!(foreign.published_counts(), (0, 0, 0));
    let _ = activate(&mut foreign, &visible, 10);
    assert_eq!(foreign.published_counts(), (1, 3, 6));
    let mut invalid = Node::new(Role::Document);
    invalid.set_children(vec![NodeId(999)]);
    assert!(
        foreign
            .apply(
                surface(1),
                TreeUpdate {
                    nodes: vec![(NodeId(0), invalid)],
                    tree: None,
                    tree_id: tree(103),
                    focus: NodeId(0),
                }
            )
            .is_err()
    );
    assert_eq!(foreign.published_counts(), (0, 0, 0));
    assert!(
        foreign.publication_diagnostic(true)["surfaces"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    foreign.admit(surface(1), tree(501)).unwrap();
    foreign
        .apply(surface(1), document(tree(501), "replacement"))
        .unwrap();
    let _ = activate(&mut foreign, &visible, 10);
    assert_eq!(foreign.published_counts(), (1, 1, 2));
    foreign.retire(surface(1));
    assert_eq!(foreign.published_counts(), (0, 0, 0));
}

#[test]
fn fixture_text_optin_never_exposes_password_nodes_or_descendants() {
    let mut foreign = ForeignA11y::new();
    foreign.admit(surface(1), tree(501)).unwrap();
    let mut root = Node::new(Role::Document);
    root.set_label("public fixture marker");
    root.set_children(vec![NodeId(1)]);
    let mut password = Node::new(Role::PasswordInput);
    password.set_label("secret label");
    password.set_value("secret value");
    password.add_action(Action::SetValue);
    password.set_children(vec![NodeId(2)]);
    let mut text = Node::new(Role::TextRun);
    text.set_value("secret descendant");
    foreign
        .apply(
            surface(1),
            TreeUpdate {
                nodes: vec![(NodeId(0), root), (NodeId(1), password), (NodeId(2), text)],
                tree: Some(Tree::new(NodeId(0))),
                tree_id: tree(501),
                focus: NodeId(0),
            },
        )
        .unwrap();
    let _ = activate(&mut foreign, &[surface(1)], 10);
    let redacted = foreign.publication_diagnostic(false).to_string();
    assert!(!redacted.contains("public fixture marker"));
    assert!(!redacted.contains("secret"));
    let public = foreign.publication_diagnostic(true).to_string();
    assert!(public.contains("public fixture marker"));
    assert!(public.contains("SetValue"));
    assert!(!public.contains("secret"));
    assert_eq!(foreign.published_text(), "public fixture marker");
    assert_eq!(foreign.published_counts(), (1, 1, 3));
}

#[test]
fn window_publishers_share_retained_nodes_and_keep_mount_and_replay_independent() {
    let mut primary = nested();
    let mut lens = ForeignA11y::new();
    lens.sync_retained_from(&primary);
    let visible = [surface(1)];
    assert!(std::sync::Arc::ptr_eq(
        &primary.surfaces[&surface(1)].trees[&tree(103)].nodes,
        &lens.surfaces[&surface(1)].trees[&tree(103)].nodes
    ));
    let mut primary_consumer = activate(&mut primary, &visible, 10);
    let mut lens_consumer = activate(&mut lens, &[], 0);
    send(
        &mut primary_consumer,
        primary.frame(host(&primary, &[], 0), &[]).unwrap(),
    );
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &visible, 10), &visible).unwrap(),
    );
    assert!(
        primary
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    assert_eq!(
        lens.action_target(&request(tree(103), 1, Action::Click)),
        Ok(surface(1))
    );
    primary
        .apply(
            surface(1),
            TreeUpdate {
                nodes: vec![(NodeId(1), button("moved and updated"))],
                tree: None,
                tree_id: tree(103),
                focus: NodeId(1),
            },
        )
        .unwrap();
    lens.sync_retained_from(&primary);
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &visible, 10), &visible).unwrap(),
    );
    assert!(lens.published_text().contains("moved and updated"));
    let before = lens.published_counts();
    primary.reset_publication();
    assert_eq!(lens.published_counts(), before);
    primary.retire(surface(1));
    lens.sync_retained_from(&primary);
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &[], 0), &[]).unwrap(),
    );
    assert!(
        lens.action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    assert_eq!(lens.published_counts(), (0, 0, 0));
}
