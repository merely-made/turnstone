// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::panes::{FrisketId, FrisketLayout, GraphId, PaneId, PaneKindId, PaneNode};

#[test]
fn lens_projection_identity_keeps_retained_links_per_window_and_refuses_missing_lenses() {
    let mut app = App::test_stub();
    app.lenses.clear();
    for ordinal in 0..2 {
        let pane = PaneId(40 + ordinal as u64);
        app.lenses.push(Some(FrisketLayout {
            id: FrisketId::new(format!("lens-test-{ordinal}")),
            label: format!("Document {ordinal}"),
            root: PaneNode::Workspace(Box::new(
                crate::panes::workspace_bridge::OuterWorkspace::single(
                    pane.0,
                    format!("Document {ordinal}"),
                    (
                        PaneContent::Registered(PaneKindId::new("retained-test")),
                        GraphId::nil(),
                    ),
                ),
            )),
        }));
    }
    let mut roots = Vec::new();
    for ordinal in 0..2 {
        let pane = PaneId(40 + ordinal as u64);
        let mut document = Node::new(Role::Group);
        document.set_label("retained document");
        let links = ["First Gemini", "Second Gemini"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let mut link = Node::new(Role::Link);
                link.set_label(name);
                stitch(
                    &format!("test/lens/{ordinal}/link/{index}"),
                    link,
                    Vec::new(),
                )
            })
            .collect();
        let contribution = stitch(&format!("test/lens/{ordinal}/document"), document, links);
        let retained_ids: Vec<_> = contribution.nodes.iter().map(|(id, _)| *id).collect();
        let (tree, _) = project_lens_with_contributions(
            &app,
            ordinal,
            std::collections::HashMap::from([(pane, contribution)]),
        )
        .expect("a live shared Workspace lens projects its retained subtree");
        let window = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(window.1.role(), Role::Window);
        let expected = format!("Turnstone lens {ordinal}");
        assert_eq!(window.1.label(), Some(expected.as_str()));
        assert!(
            retained_ids
                .iter()
                .all(|id| tree.nodes.iter().any(|(node, _)| node == id))
        );
        let mut forest = crate::foreign_a11y::ForeignA11y::new();
        forest
            .frame(tree.to_tree_update(None), &[])
            .expect("actual composed host publication");
        assert!(
            matches_lines(
                &forest.observed_lines(),
                &format!("window: {expected} >> link: First Gemini >> link: Second Gemini",)
            ),
            "window identity and retained link order remain in the published forest"
        );
        roots.push(tree.root);
    }
    assert_ne!(
        roots[0], roots[1],
        "distinct native lenses retain distinct roots"
    );
    assert!(project_lens_with_contributions(&app, 2, Default::default()).is_none());
    app.lenses[1] = None;
    assert!(project_lens_with_contributions(&app, 1, Default::default()).is_none());
}
