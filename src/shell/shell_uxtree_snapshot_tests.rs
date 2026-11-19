// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use accesskit::{Action, Role, TreeInfo};
use uuid::Uuid;

const GOLDEN: &str = include_str!("../../tests/fixtures/uxtree-snapshot.json");
const SECRET: &str = "P10-PLANTED-PRIVATE-TEXT";

fn fixture() -> (Vec<TreeUpdate>, ActionEnvelopes) {
    let guest_id = TreeId(Uuid::from_u128(1));
    let mut root = Node::new(Role::Window);
    root.set_children(vec![NodeId(2), NodeId(3)]);
    root.set_label("Public fixture");
    let mut button = Node::new(Role::Button);
    button.set_label("Open");
    button.add_action(Action::Click);
    let mut password = Node::new(Role::PasswordInput);
    password.set_label(SECRET);
    password.set_description(SECRET);
    password.set_value(SECRET);
    password.set_children(vec![NodeId(4)]);
    let mut graft = Node::new(Role::GenericContainer);
    graft.set_tree_id(guest_id);
    graft.set_label(SECRET);
    let mut child = Node::new(Role::Label);
    child.set_label(SECRET);
    let root_update = TreeUpdate {
        tree_id: TreeId::ROOT,
        tree: Some(TreeInfo::new(NodeId(1))),
        focus: NodeId(3),
        nodes: vec![
            (NodeId(1), root),
            (NodeId(2), button),
            (NodeId(3), password),
            (NodeId(4), graft),
        ],
    };
    let guest_update = TreeUpdate {
        tree_id: guest_id,
        tree: Some(TreeInfo::new(NodeId(1))),
        focus: NodeId(1),
        nodes: vec![(NodeId(1), child)],
    };
    let forest = cambium_winit_a11y::guest::ForeignA11y::<uuid::Uuid>::default()
        .tooling_forest(root_update, &[], &[guest_update])
        .unwrap();
    let mut public =
        crate::a11y::action_envelope(NodeId(1), NodeId(2), crate::a11y::A11yRoute::OpenOmnibar);
    public.action_ids.push("fixture.open".into());
    let mut protected =
        crate::a11y::action_envelope(NodeId(1), NodeId(3), crate::a11y::A11yRoute::OpenOmnibar);
    protected.authored_key = Some(SECRET.into());
    protected.action_ids.push(SECRET.into());
    (forest, [(NodeId(2), public), (NodeId(3), protected)].into())
}

#[test]
fn uxtree_snapshot_checks_redaction_before_reading_and_matches_golden() {
    let (forest, routes) = fixture();
    assert!(forest.iter().any(|tree| {
        tree.nodes
            .iter()
            .any(|(_, node)| node.label() == Some(SECRET))
    }));

    TEXT_READS.with(|reads| reads.set(0));
    let private = tooling_snapshot(&forest, &routes, false).to_string();
    assert_eq!(TEXT_READS.with(|reads| reads.get()), 0);
    assert!(!private.contains("Public fixture"));
    assert!(!private.contains("fixture.open"));
    assert!(!private.contains(SECRET));

    let public = tooling_snapshot(&forest, &routes, true);
    assert_eq!(TEXT_READS.with(|reads| reads.get()), 2);
    assert!(!public.to_string().contains(SECRET));
    assert_eq!(public, serde_json::from_str::<Value>(GOLDEN).unwrap());
}
