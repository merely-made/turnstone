// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
#[test]
fn reading_order_follows_children_and_excludes_orphans() {
    let mut root = Node::new(Role::Document);
    root.set_label("page");
    root.set_children(vec![accesskit::NodeId(3), accesskit::NodeId(2)]);
    let named = |name: &str| {
        let mut n = Node::new(Role::Link);
        n.set_label(name);
        n
    };
    let tree = UxTree {
        root: accesskit::NodeId(1),
        nodes: vec![
            (accesskit::NodeId(2), named("second")),
            (accesskit::NodeId(4), named("orphan")),
            (accesskit::NodeId(1), root),
            (accesskit::NodeId(3), named("first")),
        ],
    };
    assert_eq!(
        tree_lines(&tree),
        ["document: page", "link: first", "link: second"]
    );
}
