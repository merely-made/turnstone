// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::collections::HashSet;

#[test]
fn a_contributed_tree_attaches_below_its_registered_pane() {
    let mut app = App::test_stub();
    app.update(crate::action::Action::SummonPane(
        crate::panes::PaneKindId::new(crate::panes::kind::SETTINGS),
    ));
    let pane = app
        .frisket
        .iter_leaves()
        .find_map(|(pane, content, _)| match content {
            PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SETTINGS => {
                Some(pane)
            },
            _ => None,
        })
        .expect("registered pane");
    let contributed_root = node_id_for_path("test/contributed/root");
    let contributed_child = node_id_for_path("test/contributed/child");
    let mut root = Node::new(Role::Group);
    root.set_label("settings surface");
    root.set_children(vec![contributed_child]);
    let mut child = Node::new(Role::Button);
    child.set_label("Apply");
    let contribution = UxTree {
        root: contributed_root,
        nodes: vec![(contributed_child, child), (contributed_root, root)],
    };

    let (tree, _) = project_app_with_routes_and_contributions(
        &app,
        std::collections::HashMap::from([(pane, contribution)]),
    );
    assert!(
        tree.nodes.iter().any(|(id, _)| *id == contributed_child),
        "the subtree child remains in the composed update"
    );
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.children().contains(&contributed_root)),
        "the registered pane leaf points at the contributed root"
    );
}

#[test]
fn diagnostic_gloss_rows_attach_without_routes_and_retire_after_removal() {
    let mut app = App::test_stub();
    app.update(crate::action::Action::SummonPane(
        crate::panes::PaneKindId::new(crate::panes::kind::GLOSS),
    ));
    let pane = app
        .frisket
        .iter_leaves()
        .find_map(|(pane, content, _)| matches!(content, PaneContent::Gloss(_)).then_some(pane))
        .unwrap();
    app.diagnostic_inspection = Some(Ok(apparatus_core::Inspection {
        lines: vec![apparatus_core::InspectionLine {
            id: "apparatus/fixture/record/1".into(),
            text: "Loss evicted: 2".into(),
        }],
    }));
    let mut surface = crate::swatch_pane::SwatchPane::new(crate::swatch_pane::GLOSS_MINIMAP);
    surface.set_sections(vec![crate::sections::DIAGNOSTICS_SECTION]);
    surface.sync(&app, 480.0, 400.0);
    surface.scene(480, 400, 1.0);
    let contribution = surface
        .diagnostic_tree(pane, crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0))
        .unwrap();
    let row_id = contribution
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Loss evicted: 2"))
        .unwrap()
        .0;
    let diagnostic_root = contribution.root;
    let (tree, routes) = project_app_with_routes_and_contributions(
        &app,
        std::collections::HashMap::from([(pane, contribution)]),
    );
    assert!(
        tree.nodes
            .iter()
            .any(|(id, node)| *id == row_id && node.label() == Some("Loss evicted: 2")),
        "the platform tree must contain the actual diagnostic row"
    );
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.children().contains(&diagnostic_root)),
        "the Gloss leaf must attach its diagnostic subtree"
    );
    assert!(
        !routes.contains_key(&row_id),
        "read-only diagnostics create no activation route"
    );
    surface.set_sections(Vec::new());
    surface.sync(&app, 480.0, 400.0);
    surface.scene(480, 400, 1.0);
    assert!(
        surface
            .diagnostic_tree(pane, crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0))
            .is_none()
    );
    let (removed, routes) = project_app_with_routes(&app);
    assert!(!removed.nodes.iter().any(|(id, _)| *id == row_id));
    let before = app.graph_runtimes.graph().nodes().count();
    assert!(apply_route(&mut app, routes.get(&row_id), row_id).is_empty());
    assert_eq!(app.graph_runtimes.graph().nodes().count(), before);
    assert!(
        app.take_events().iter().any(|event| matches!(
            event,
            crate::observe::AppEvent::InteractionMissed {
                what: "a11y-action",
                ..
            }
        )),
        "a stale platform request stays an attributable miss"
    );
}

/// UIA stops traversal at a node whose children reference an id absent
/// from the pushed tree, which reads to a person as "nothing after that
/// when I hit down arrow". The in-process assert-a11y walk never caught
/// this because it iterates the flat node list rather than following
/// children.
#[test]
fn the_pushed_tree_has_no_dangling_children_and_no_duplicate_ids() {
    let app = App::test_stub();
    let tree = project_app(&app);
    let mut seen = HashSet::new();
    for (id, _) in &tree.nodes {
        assert!(seen.insert(*id), "duplicate node id {id:?} in one update");
    }
    for (id, node) in &tree.nodes {
        for child in node.children() {
            assert!(
                seen.contains(child),
                "node {id:?} references child {child:?} that is not in the update"
            );
        }
    }
    assert!(seen.contains(&tree.root), "the root itself is missing");
}
