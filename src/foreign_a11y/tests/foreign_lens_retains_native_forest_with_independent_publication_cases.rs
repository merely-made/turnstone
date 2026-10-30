// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn foreign_lens_retains_native_forest_with_independent_publication_and_retirement() {
    let mut primary = nested();
    let visible = [surface(1)];
    let mut primary_consumer = activate(&mut primary, &visible, 10);
    let mut lens = ForeignA11y::new();
    lens.sync_retained_from(&primary);
    assert!(
        lens.ready(surface(1)),
        "a new native lens inherits the retained supplier forest"
    );
    let mut lens_consumer = activate(&mut lens, &visible, 10);
    assert!(primary_consumer.state().subtree_root(tree(103)).is_some());
    assert!(lens_consumer.state().subtree_root(tree(103)).is_some());
    send(
        &mut primary_consumer,
        primary.frame(host(&primary, &[], 0), &[]).unwrap(),
    );
    let click = request(tree(103), 1, Action::Click);
    assert!(primary.action_target(&click).is_err());
    assert_eq!(lens.action_target(&click), Ok(surface(1)));
    assert!(lens_consumer.state().subtree_root(tree(103)).is_some());
    primary
        .apply(surface(1), document(tree(103), "updated native link"))
        .unwrap();
    lens.sync_retained_from(&primary);
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &visible, 10), &visible).unwrap(),
    );
    assert!(
        lens.observed_lines()
            .iter()
            .any(|line| line.contains("updated native link"))
    );
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &[], 0), &[]).unwrap(),
    );
    assert!(lens.action_target(&click).is_err());
    send(
        &mut primary_consumer,
        primary
            .frame(host(&primary, &visible, 10), &visible)
            .unwrap(),
    );
    assert_eq!(primary.action_target(&click), Ok(surface(1)));
    let mut moved = host(&lens, &visible, 10);
    let graft = lens
        .graft(surface(1), Rect::new(40.0, 60.0, 240.0, 260.0))
        .unwrap();
    moved
        .nodes
        .iter_mut()
        .find(|(id, _)| *id == NodeId(10))
        .unwrap()
        .1 = graft;
    send(&mut lens_consumer, lens.frame(moved, &visible).unwrap());
    assert_eq!(lens.action_target(&click), Ok(surface(1)));
    assert!(lens_consumer.state().subtree_root(tree(103)).is_some());
    primary.retire(surface(1));
    lens.sync_retained_from(&primary);
    assert!(
        lens.action_target(&click).is_err(),
        "retired supplier cannot survive in another window"
    );
    send(
        &mut lens_consumer,
        lens.frame(host(&lens, &[], 0), &[]).unwrap(),
    );
    assert!(lens_consumer.state().subtree_root(tree(103)).is_none());
}

#[test]
fn local_guests_and_nested_native_guests_share_one_observed_forest() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let local_id = tree(201);
    let mut host = host(&foreign, &visible, 20);
    host.nodes[0].1.set_children(vec![NodeId(20), NodeId(10)]);
    host.nodes.push((
        NodeId(20),
        graft_node(Role::Group, local_id, (100.0, 50.0), (300.0, 200.0)),
    ));
    let local = document(local_id, "local field");
    let safe = foreign
        .initial_host_with_guests(host.clone(), &visible, &[local.clone()])
        .unwrap();
    let mut consumer = ConsumerTree::new(safe, true);
    let updates = foreign
        .frame_with_guests(host.clone(), &visible, &[local.clone()])
        .unwrap();
    send(&mut consumer, updates);
    assert_eq!(
        consumer.state().focus().unwrap().label().as_deref(),
        Some("local field")
    );
    let lines = foreign.observed_lines();
    assert!(crate::a11y::matches_lines(
        &lines,
        "button: local field >> button: first"
    ));
    let mut changed = local.clone();
    changed.nodes[1].1.set_label("edited field");
    send(
        &mut consumer,
        foreign
            .frame_with_guests(host, &visible, &[changed])
            .unwrap(),
    );
    assert_eq!(
        consumer.state().focus().unwrap().label().as_deref(),
        Some("edited field")
    );
    assert!(
        foreign
            .observed_lines()
            .iter()
            .any(|line| line == "button: edited field")
    );
}

#[test]
fn inactive_adapter_does_not_consume_pending_activation() {
    use std::sync::atomic::{AtomicBool, Ordering};

    fn update_if_active(active: bool, updater: impl FnOnce() -> TreeUpdate) -> Option<TreeUpdate> {
        active.then(updater)
    }
    let foreign = nested();
    let visible = [surface(1)];
    let initial = foreign
        .initial_host(host(&foreign, &visible, 10), &visible)
        .unwrap();
    let replay = AtomicBool::new(true);
    // Early cadence inspection must leave the notification pending.
    assert!(replay.load(Ordering::Acquire));
    let mut called = false;
    let result = update_if_active(false, || {
        called = true;
        activation_safe_update(&replay, initial.clone(), &initial).0
    });
    assert!(result.is_none());
    assert!(!called);
    assert!(replay.load(Ordering::Acquire));
}

#[test]
fn nested_focus_and_two_panes_keep_identical_local_ids_distinct() {
    let mut foreign = nested();
    foreign.admit(surface(2), tree(201)).unwrap();
    foreign
        .apply(surface(2), document(tree(201), "second"))
        .unwrap();
    let visible = [surface(1), surface(2)];
    foreign.reset_publication();
    let initial = foreign
        .initial_host(host(&foreign, &visible, 10), &visible)
        .unwrap();
    let mut consumer = ConsumerTree::new(initial, true);
    let mut changes = Changes::default();
    for update in foreign
        .frame(host(&foreign, &visible, 10), &visible)
        .unwrap()
    {
        consumer.update_and_process_changes(update, &mut changes);
    }
    assert_eq!(changes.focused, vec![(NodeId(1), tree(103))]);
    let focus = consumer.state().focus_id().unwrap();
    assert_eq!(
        consumer.state().locate_node(focus),
        Some((NodeId(1), tree(103)))
    );
    assert!(
        consumer
            .state()
            .node_by_tree_local_id(NodeId(1), tree(201))
            .is_some()
    );
    assert_eq!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .unwrap(),
        surface(1)
    );
    assert_eq!(
        foreign
            .action_target(&request(tree(201), 1, Action::Click))
            .unwrap(),
        surface(2)
    );
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Focus))
            .is_err()
    );
    assert!(
        foreign
            .action_target(&request(tree(103), 99, Action::Click))
            .is_err()
    );
    assert!(
        foreign
            .action_target(&request(TreeId::ROOT, 0, Action::Click))
            .is_err()
    );
}

#[test]
fn idle_delta_reaches_consumer_without_a_pixel_frame() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    assert!(!foreign.needs_publish());
    foreign
        .apply(
            surface(1),
            TreeUpdate {
                nodes: vec![(NodeId(1), button("idle title"))],
                tree: None,
                tree_id: tree(103),
                focus: NodeId(1),
            },
        )
        .unwrap();
    assert!(foreign.needs_publish());
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert_eq!(
        consumer
            .state()
            .node_by_tree_local_id(NodeId(1), tree(103))
            .unwrap()
            .label()
            .as_deref(),
        Some("idle title")
    );
    assert!(!foreign.needs_publish());
}

#[test]
fn navigation_prunes_old_nested_trees_and_ignores_stale_callbacks() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    foreign
        .apply(surface(1), wrapper(tree(101), tree(104)))
        .unwrap();
    assert!(!foreign.ready(surface(1)));
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    assert!(
        foreign
            .apply(surface(1), document(tree(103), "stale"))
            .is_err()
    );
    foreign
        .apply(surface(1), document(tree(104), "replacement"))
        .unwrap();
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert!(consumer.state().subtree_root(tree(102)).is_none());
    assert!(consumer.state().subtree_root(tree(103)).is_none());
    assert_eq!(
        consumer
            .state()
            .locate_node(consumer.state().focus_id().unwrap()),
        Some((NodeId(1), tree(104)))
    );
}

#[test]
fn hidden_remount_and_adapter_reactivation_replay_all_descendants() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    send(
        &mut consumer,
        foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
    );
    assert!(consumer.state().subtree_root(tree(101)).is_none());
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert_eq!(
        consumer
            .state()
            .locate_node(consumer.state().focus_id().unwrap()),
        Some((NodeId(1), tree(103)))
    );
    let consumer = activate(&mut foreign, &visible, 10);
    assert!(consumer.state().subtree_root(tree(103)).is_some());
}

#[test]
fn removed_local_nodes_retire_their_actions() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    foreign
        .apply(
            surface(1),
            TreeUpdate {
                nodes: vec![(NodeId(0), Node::new(Role::Document))],
                tree: None,
                tree_id: tree(103),
                focus: NodeId(0),
            },
        )
        .unwrap();
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    send(
        &mut consumer,
        foreign
            .frame(host(&foreign, &visible, 10), &visible)
            .unwrap(),
    );
    assert!(
        consumer
            .state()
            .node_by_tree_local_id(NodeId(1), tree(103))
            .is_none()
    );
}

#[test]
fn malformed_known_tree_withdraws_but_unknown_tree_cannot_corrupt() {
    let mut foreign = nested();
    let visible = [surface(1)];
    let mut consumer = activate(&mut foreign, &visible, 10);
    assert!(
        foreign
            .apply(surface(1), document(tree(999), "unknown"))
            .is_err()
    );
    assert!(foreign.ready(surface(1)));
    assert_eq!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .unwrap(),
        surface(1)
    );
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
    assert!(!foreign.ready(surface(1)));
    assert!(foreign.needs_publish());
    assert!(
        foreign
            .graft(surface(1), Rect::new(0.0, 0.0, 100.0, 100.0))
            .is_none()
    );
    assert!(
        foreign
            .action_target(&request(tree(103), 1, Action::Click))
            .is_err()
    );
    send(
        &mut consumer,
        foreign.frame(host(&foreign, &[], 0), &[]).unwrap(),
    );
    assert!(consumer.state().subtree_root(tree(101)).is_none());
}

#[test]
fn duplicate_ownership_cycles_invalid_focus_and_local_graphs_fail_closed() {
    let mut foreign = nested();
    assert!(foreign.admit(surface(2), tree(101)).is_err());
    assert!(foreign.admit(surface(2), TreeId::ROOT).is_err());
    assert!(
        foreign
            .apply(surface(2), document(tree(103), "foreign"))
            .is_err()
    );
    assert!(foreign.ready(surface(1)));

    foreign.admit(surface(2), tree(201)).unwrap();
    foreign
        .apply(surface(2), document(tree(201), "owned elsewhere"))
        .unwrap();
    foreign.admit(surface(3), tree(301)).unwrap();
    assert!(
        foreign
            .apply(surface(3), wrapper(tree(301), tree(201)))
            .is_err()
    );
    assert!(!foreign.ready(surface(3)));
    assert!(foreign.ready(surface(2)));

    let mut duplicate_grafts = wrapper(tree(301), tree(302));
    duplicate_grafts.nodes[0]
        .1
        .set_children(vec![NodeId(1), NodeId(2)]);
    duplicate_grafts
        .nodes
        .push((NodeId(2), duplicate_grafts.nodes[1].1.clone()));
    let mut local_cycle = document(tree(301), "cycle");
    local_cycle.nodes[1].1.set_children(vec![NodeId(0)]);
    let mut invalid_focus = document(tree(301), "focus");
    invalid_focus.focus = NodeId(999);
    let mut duplicate_node = document(tree(301), "duplicate");
    duplicate_node.nodes.push((NodeId(1), button("duplicate")));
    let mut unreachable = document(tree(301), "unreachable");
    unreachable.nodes.push((NodeId(9), button("unreachable")));
    let mut graft_with_children = wrapper(tree(301), tree(302));
    graft_with_children.nodes[1].1.set_children(vec![NodeId(0)]);
    for update in [
        wrapper(tree(301), tree(301)),
        duplicate_grafts,
        local_cycle,
        invalid_focus,
        duplicate_node,
        unreachable,
        graft_with_children,
    ] {
        foreign.admit(surface(3), tree(301)).unwrap();
        assert!(foreign.apply(surface(3), update).is_err());
        assert!(!foreign.ready(surface(3)));
        assert!(foreign.ready(surface(1)));
    }
}
