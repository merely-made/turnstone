// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashSet;

use super::*;
use crate::action::{Action, Update};
use crate::content::{ContentFacts, StructureFacts};

fn app_with_live_doc() -> App {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.apply_update(Update::ContentSpawned {
        node,
        facts: Some(ContentFacts {
            engine: "genet.web".to_string(),
            lane: crate::content::ContentLane::Document,
            lineage: None,
            capabilities: Default::default(),
            structure: Some(StructureFacts {
                title: Some("Example Domain".to_string()),
                headings: 1,
                links: 1,
                outline: vec![
                    OutlineFact {
                        depth: 0,
                        role: "heading",
                        name: "Example Domain".to_string(),
                    },
                    OutlineFact {
                        depth: 1,
                        role: "link",
                        name: "More information...".to_string(),
                    },
                ],
            }),
        }),
    });
    app
}

/// The stitched tree is coherent: one window root, every id unique
/// (disjoint subtree ranges — the deletion-matrix bar), and every
/// child id present in the node list.
#[test]
fn the_stitched_tree_is_coherent() {
    let app = app_with_live_doc();
    let tree = project_app(&app);
    let ids: Vec<_> = tree.nodes.iter().map(|(id, _)| *id).collect();
    let unique: HashSet<_> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "no id collisions across subtrees");
    for (id, n) in &tree.nodes {
        for child in n.children() {
            assert!(
                unique.contains(child),
                "node {id:?} references a missing child {child:?}"
            );
        }
    }
    let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
    assert_eq!(root.role(), Role::Window);
    assert_eq!(root.label(), Some("Turnstone"));
}

/// The pane structure, the workbench tiling, and the live document's
/// outline all arrive under the one root, and the document declares its
/// partial capability instead of implying coverage.
#[test]
fn panes_workbench_and_documents_project_under_one_root() {
    let app = app_with_live_doc();
    let lines = a11y_lines(&app);
    let has = |s: &str| lines.iter().any(|l| l.contains(s));
    assert!(has("group: Read"), "{lines:?}");
    assert!(has("tab: Tile"), "the tile's tab projects");
    assert!(has("document: Example Domain"), "{lines:?}");
    assert!(has("heading: Example Domain"));
    assert!(has("link: More information..."));
    assert!(
        !has("graph canvas, 1 nodes"),
        "inactive Orrery is not a visible canvas"
    );
    let doc = project_app(&app)
        .nodes
        .into_iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == Role::Document)
        .expect("the live document projects");
    assert_eq!(
        doc.description(),
        Some("structural outline only: no bounds, no per-element focus"),
        "the capability is declared, not implied"
    );
}

/// The omnibar joins the chrome subtree only while it is open, carrying
/// its live text.
#[test]
fn the_omnibar_projects_while_open() {
    let mut app = App::test_stub();
    assert!(
        !a11y_lines(&app).iter().any(|l| l.starts_with("textinput")),
        "closed omnibar projects nothing"
    );
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('h'));
    app.update(Action::OmnibarChar('i'));
    let lines = a11y_lines(&app);
    assert!(
        lines.iter().any(|l| l == "textinput: omnibar = hi"),
        "{lines:?}"
    );
}
