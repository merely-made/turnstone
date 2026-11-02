// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::contributed_a11y::{ContributedA11yRoute, PublishedRoute, count_source_comparisons};
use crate::{panes::PaneId, surface::Rect};
use accesskit::{Action, Node, NodeId, Role, Tree, TreeId};
use std::collections::HashMap;

const CONTROLS: u64 = 64;

fn source() -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_children((1..=CONTROLS).map(NodeId).collect::<Vec<_>>());
    let mut nodes = vec![(NodeId(0), root)];
    for id in 1..=CONTROLS {
        let mut node = Node::new(Role::Button);
        node.set_label(format!("Control {id}"));
        node.add_action(Action::Click);
        node.add_action(Action::Focus);
        nodes.push((NodeId(id), node));
    }
    TreeUpdate {
        tree_id: TreeId::ROOT,
        tree: Some(Tree::new(NodeId(0))),
        nodes,
        focus: NodeId(0),
    }
}

fn admissions(source: TreeUpdate) -> HashMap<(TreeId, NodeId), Admission> {
    pane_admissions(PaneId(1), source)
}

fn pane_admissions(pane: PaneId, source: TreeUpdate) -> HashMap<(TreeId, NodeId), Admission> {
    let source = Arc::new(source);
    let tree = uxtree::graft::tree_id_for_path(&format!("admission-comparisons/{}", pane.0));
    (1..=CONTROLS)
        .map(|id| {
            (
                (tree, NodeId(id)),
                Admission::Contributed(PublishedRoute {
                    route: ContributedA11yRoute {
                        pane,
                        generation: 8,
                        node: genet_scripted_dom::NodeId::from_raw(id),
                    },
                    rect: Rect::new(10.0, 20.0, 300.0, 200.0),
                    source: source.clone(),
                }),
            )
        })
        .collect()
}

#[test]
fn contributed_sources_keep_independent_semantic_results() {
    let mut queue = ActionQueueState::default();
    let mut before = admissions(source());
    before.extend(pane_admissions(PaneId(2), source()));
    queue.publish(before);
    let captured = queue.published.clone();
    let mut changed = source();
    changed.nodes[1].1.set_label("Changed second pane");
    let mut after = admissions(source());
    after.extend(pane_admissions(PaneId(2), changed));
    let comparisons = count_source_comparisons(|| queue.publish(after));
    assert_eq!(comparisons, 2, "each independent source is compared once");
    for (target, admission) in captured {
        let Admission::Contributed(route) = &admission.admission else {
            unreachable!()
        };
        assert_eq!(
            queue.is_current(&request(&target), &admission),
            route.route.pane == PaneId(1),
            "an unrelated source change cannot retire an unchanged pane"
        );
    }
}

fn request(target: &(TreeId, NodeId)) -> ActionRequest {
    ActionRequest {
        target_tree: target.0,
        target_node: target.1,
        action: Action::Click,
        data: None,
    }
}

#[test]
fn contributed_republication_compares_each_shared_source_once() {
    let mut queue = ActionQueueState::default();
    queue.publish(admissions(source()));
    let before = queue.published.clone();
    let comparisons = count_source_comparisons(|| queue.publish(admissions(source())));
    assert_eq!(
        comparisons, 1,
        "one equal source must not be compared again for every control"
    );
    for (target, captured) in before {
        assert!(
            queue.is_current(&request(&target), &captured),
            "equal freshly allocated source retains every queued admission"
        );
    }
}

#[test]
fn contributed_source_identity_does_not_replace_semantic_equality() {
    let mut non_reflexive = source();
    non_reflexive.nodes[1]
        .1
        .set_bounds(accesskit::Rect::new(f64::NAN, 0.0, 10.0, 10.0));
    let same_allocation = Arc::new(non_reflexive);
    assert!(
        !crate::contributed_a11y::source_trees_equal(&same_allocation, &same_allocation),
        "even identical source allocations retain AccessKit's floating-point equality"
    );
}

#[test]
fn contributed_source_changes_retire_every_queued_control() {
    for change in [
        "label",
        "focus",
        "bounds",
        "disabled",
        "action",
        "transform",
    ] {
        let mut queue = ActionQueueState::default();
        queue.publish(admissions(source()));
        let before = queue.published.clone();
        let mut changed = source();
        match change {
            "label" => changed.nodes[1].1.set_label("Changed"),
            "focus" => changed.focus = NodeId(1),
            "bounds" => changed.nodes[1]
                .1
                .set_bounds(accesskit::Rect::new(1.0, 2.0, 30.0, 40.0)),
            "disabled" => changed.nodes[1].1.set_disabled(),
            "action" => changed.nodes[1].1.remove_action(Action::Click),
            "transform" => changed.nodes[1]
                .1
                .set_transform(accesskit::Affine::translate((1.0, 2.0))),
            _ => unreachable!(),
        }
        let comparisons = count_source_comparisons(|| queue.publish(admissions(changed)));
        assert_eq!(comparisons, 1, "{change}: compare the changed source once");
        for (target, captured) in before {
            assert!(
                !queue.is_current(&request(&target), &captured),
                "{change}: changing any source property retires the old source admission"
            );
        }
    }
}

#[test]
fn contributed_route_placement_session_and_target_changes_remain_stale() {
    for change in ["rect", "pane", "generation", "node", "removed"] {
        let mut queue = ActionQueueState::default();
        queue.publish(admissions(source()));
        let (&target, captured) = queue.published.iter().next().unwrap();
        let captured = captured.clone();
        let mut changed = admissions(source());
        if change == "removed" {
            changed.remove(&target);
        } else {
            let Admission::Contributed(route) = changed.get_mut(&target).unwrap() else {
                unreachable!()
            };
            match change {
                "rect" => route.rect.x += 1.0,
                "pane" => route.route.pane = PaneId(2),
                "generation" => route.route.generation += 1,
                "node" => route.route.node = genet_scripted_dom::NodeId::from_raw(999),
                _ => unreachable!(),
            }
        }
        queue.publish(changed);
        assert!(
            !queue.is_current(&request(&target), &captured),
            "{change}: source equality cannot revive a changed route"
        );
    }
}
