// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use accesskit::{Action, Uuid as TreeUuid};
use accesskit_consumer::{NodeRef as ConsumerNode, Tree as ConsumerTree, TreeChangeHandler};

fn tree(id: u64) -> TreeId {
    TreeId(TreeUuid::from_u64_pair(0, id))
}

fn surface(id: u128) -> Uuid {
    Uuid::from_u128(id)
}

fn wrapper(id: TreeId, child: TreeId) -> TreeUpdate {
    let mut root = Node::new(Role::ScrollView);
    root.set_children(vec![NodeId(1)]);
    let mut graft = Node::new(Role::GenericContainer);
    graft.set_tree_id(child);
    TreeUpdate {
        nodes: vec![(NodeId(0), root), (NodeId(1), graft)],
        tree: Some(Tree::new(NodeId(0))),
        tree_id: id,
        focus: NodeId(1),
    }
}

fn button(label: &str) -> Node {
    let mut button = Node::new(Role::Button);
    button.set_label(label);
    button.add_action(Action::Click);
    button
}

fn document(id: TreeId, label: &str) -> TreeUpdate {
    let mut root = Node::new(Role::Document);
    root.set_children(vec![NodeId(1)]);
    TreeUpdate {
        nodes: vec![(NodeId(0), root), (NodeId(1), button(label))],
        tree: Some(Tree::new(NodeId(0))),
        tree_id: id,
        focus: NodeId(1),
    }
}

fn host(foreign: &ForeignA11y, visible: &[Uuid], focus: u64) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    for (index, surface) in visible.iter().enumerate() {
        if let Some(graft) = foreign.graft(
            *surface,
            Rect::new(
                index as f64 * 100.0,
                0.0,
                index as f64 * 100.0 + 100.0,
                100.0,
            ),
        ) {
            let id = NodeId(10 + index as u64);
            children.push(id);
            nodes.push((id, graft));
        }
    }
    root.set_children(children);
    nodes.insert(0, (NodeId(0), root));
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(0))),
        tree_id: TreeId::ROOT,
        focus: NodeId(focus),
    }
}

fn request(id: TreeId, node: u64, action: Action) -> ActionRequest {
    ActionRequest {
        action,
        target_tree: id,
        target_node: NodeId(node),
        data: None,
    }
}

#[derive(Default)]
struct Changes {
    focused: Vec<(NodeId, TreeId)>,
}

impl TreeChangeHandler for Changes {
    fn node_added(&mut self, _: &ConsumerNode<'_>) {}
    fn node_updated(&mut self, _: &ConsumerNode<'_>, _: &ConsumerNode<'_>) {}
    fn node_removed(&mut self, _: &ConsumerNode<'_>) {}
    fn focus_moved(&mut self, _: Option<&ConsumerNode<'_>>, new: Option<&ConsumerNode<'_>>) {
        if let Some(node) = new {
            self.focused
                .push(node.tree_state.locate_node(node.id()).unwrap());
        }
    }
}

fn send(consumer: &mut ConsumerTree, updates: Vec<TreeUpdate>) {
    for update in updates {
        consumer.update_and_process_changes(update, &mut Changes::default());
    }
}

fn nested() -> ForeignA11y {
    let mut foreign = ForeignA11y::new();
    foreign.admit(surface(1), tree(101)).unwrap();
    foreign
        .apply(surface(1), wrapper(tree(101), tree(102)))
        .unwrap();
    assert!(!foreign.ready(surface(1)));
    foreign
        .apply(surface(1), wrapper(tree(102), tree(103)))
        .unwrap();
    foreign
        .apply(surface(1), document(tree(103), "first"))
        .unwrap();
    foreign
}

fn activate(foreign: &mut ForeignA11y, visible: &[Uuid], focus: u64) -> ConsumerTree {
    foreign.reset_publication();
    let initial = foreign
        .initial_host(host(foreign, visible, focus), visible)
        .unwrap();
    assert_eq!(initial.focus, NodeId(0));
    let mut consumer = ConsumerTree::new(initial, true);
    send(
        &mut consumer,
        foreign
            .frame(host(foreign, visible, focus), visible)
            .unwrap(),
    );
    consumer
}

// Exercise this host's recursive expansion against the consumer the
// platform adapters run. AccessKit 0.25 gives Windows, Unix and macOS one
// consumer (0.39); the tests still resolve global nodes through the
// admitted subtree and locate_node, as the older Windows consumer required.
macro_rules! platform_recursive_consumer {
    ($module:ident, $consumer:ident) => {
        mod $module {
            use super::*;
            use $consumer::{
                NodeRef as PlatformNode, Tree as PlatformTree,
                TreeChangeHandler as PlatformChangeHandler,
            };

            #[derive(Default)]
            struct FocusChanges(Vec<(NodeId, TreeId)>);

            impl PlatformChangeHandler for FocusChanges {
                fn node_added(&mut self, _: &PlatformNode<'_>) {}
                fn node_updated(&mut self, _: &PlatformNode<'_>, _: &PlatformNode<'_>) {}
                fn node_removed(&mut self, _: &PlatformNode<'_>) {}
                fn focus_moved(
                    &mut self,
                    _: Option<&PlatformNode<'_>>,
                    new: Option<&PlatformNode<'_>>,
                ) {
                    if let Some(node) = new {
                        self.0.push(node.tree_state.locate_node(node.id()).unwrap());
                    }
                }
            }

            #[test]
            fn activation_at_every_batch_boundary_restarts_nested_replay() {
                use std::sync::atomic::{AtomicBool, Ordering};

                // Cover initial replay (including final focus-only updates)
                // and an unchanged frame whose guests were already live.
                for already_live in [false, true] {
                    let mut template = nested();
                    let visible = [surface(1)];
                    if already_live {
                        let _ = activate(&mut template, &visible, 10);
                    }
                    let batch = template
                        .frame(host(&template, &visible, 10), &visible)
                        .unwrap();
                    if !already_live {
                        assert!(batch.iter().any(|update| update.tree_id == TreeId::ROOT
                            && update.nodes.is_empty()
                            && update.focus == NodeId(10)));
                    }
                    for activation_index in 0..batch.len() {
                        let mut foreign = nested();
                        let initial = foreign
                            .initial_host(host(&foreign, &visible, 10), &visible)
                            .unwrap();
                        let mut consumer = PlatformTree::new(initial.clone(), true);
                        let mut changes = FocusChanges::default();
                        if already_live {
                            for update in foreign
                                .frame(host(&foreign, &visible, 10), &visible)
                                .unwrap()
                            {
                                consumer.update_and_process_changes(update, &mut changes);
                            }
                        }
                        let updates = foreign
                            .frame(host(&foreign, &visible, 10), &visible)
                            .unwrap();
                        assert_eq!(updates.len(), batch.len());
                        let replay = AtomicBool::new(false);
                        let mut aborted_at = None;
                        for (index, update) in updates.into_iter().enumerate() {
                            if index == activation_index {
                                // Platform activation replaces the consumer
                                // immediately before this guarded closure.
                                consumer = PlatformTree::new(initial.clone(), true);
                                replay.store(true, Ordering::Release);
                            }
                            let (update, abort) = activation_safe_update(&replay, update, &initial);
                            // Actual platform consumers validate every
                            // intermediate update, including the substitute.
                            consumer.update_and_process_changes(update, &mut changes);
                            if abort {
                                aborted_at = Some(index);
                                break;
                            }
                        }
                        assert_eq!(aborted_at, Some(activation_index));
                        assert!(!replay.load(Ordering::Acquire));
                        assert_eq!(
                            consumer
                                .state()
                                .locate_node(consumer.state().focus_id().unwrap()),
                            Some((NodeId(0), TreeId::ROOT))
                        );

                        foreign.reset_publication();
                        assert!(foreign.needs_publish());
                        for update in foreign
                            .frame(host(&foreign, &visible, 10), &visible)
                            .unwrap()
                        {
                            let (update, abort) = activation_safe_update(&replay, update, &initial);
                            assert!(!abort);
                            consumer.update_and_process_changes(update, &mut changes);
                        }
                        let state = consumer.state();
                        for id in [tree(101), tree(102), tree(103)] {
                            assert!(state.subtree_root(id).is_some());
                        }
                        let focus = state.focus_id().unwrap();
                        assert_eq!(state.locate_node(focus), Some((NodeId(1), tree(103))));
                        assert_eq!(
                            state.node_by_id(focus).unwrap().label().as_deref(),
                            Some("first")
                        );
                    }
                }
            }

            #[test]
            fn recursive_frame_orders_descendants_and_preserves_two_pane_ids() {
                let mut foreign = nested();
                foreign.admit(surface(2), tree(201)).unwrap();
                foreign
                    .apply(surface(2), document(tree(201), "second pane"))
                    .unwrap();
                let visible = [surface(1), surface(2)];
                foreign.reset_publication();
                let initial = foreign
                    .initial_host(host(&foreign, &visible, 10), &visible)
                    .unwrap();
                assert_eq!(initial.focus, NodeId(0));
                let mut consumer = PlatformTree::new(initial, true);
                let mut changes = FocusChanges::default();
                for update in foreign
                    .frame(host(&foreign, &visible, 10), &visible)
                    .unwrap()
                {
                    // The consumer validates every intermediate parent,
                    // subtree and focus transition, not only the final tree.
                    consumer.update_and_process_changes(update, &mut changes);
                }
                assert_eq!(changes.0, vec![(NodeId(1), tree(103))]);
                let state = consumer.state();
                for id in [tree(101), tree(102), tree(103), tree(201)] {
                    assert!(state.subtree_root(id).is_some());
                }
                let first = state.focus_id().unwrap();
                let second = state
                    .node_by_id(state.subtree_root(tree(201)).unwrap())
                    .unwrap()
                    .children()
                    .next()
                    .unwrap()
                    .id();
                assert_ne!(first, second);
                assert_eq!(state.locate_node(first), Some((NodeId(1), tree(103))));
                assert_eq!(state.locate_node(second), Some((NodeId(1), tree(201))));
                assert_eq!(
                    state.node_by_id(first).unwrap().label().as_deref(),
                    Some("first")
                );
                assert_eq!(
                    state.node_by_id(second).unwrap().label().as_deref(),
                    Some("second pane")
                );
            }
        }
    };
}

platform_recursive_consumer!(platform_consumer, accesskit_consumer);

#[path = "tests/foreign_lens_retains_native_forest_with_independent_publication_cases.rs"]
mod foreign_lens_retains_native_forest_with_independent_publication_cases;
#[path = "tests/incomplete_tree_waits_and_invalid_host_frame_keeps_cases.rs"]
mod incomplete_tree_waits_and_invalid_host_frame_keeps_cases;
