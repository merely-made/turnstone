// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn project_app_capturing(
    app: &App,
    orrery_pane: &mut Option<crate::panes::PaneId>,
    contribution: &mut impl FnMut(crate::panes::PaneId) -> Option<UxTree>,
) -> UxTree {
    let chrome = project_chrome(app);
    // Live documents, in graph order (deterministic), each from its mirrored
    // structural outline. The visible canvas carries them when present;
    // a full-width Workbench carries them while Orrery is an inactive tab.
    // A document without a structural read is announced without children.
    let docs: Vec<UxTree> = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter(|(_, n)| matches!(app.content.get(n.id), Some(NodeContent::Live)))
        .map(|(_, n)| project_live_document(app, n.id, n.url()))
        .collect();
    let mut docs = Some(docs);
    let visible_canvas = app.frisket.outer_workspace().ok().is_some_and(|frame| {
        frame.active_ids().iter().any(|id| {
            matches!(
                app.pane_content(crate::panes::PaneId(id.0)),
                Some(PaneContent::Orrery)
            )
        })
    });
    let panes = crate::panes::project_frisket_with(&app.frisket, |content, id| match content {
        PaneContent::Workbench => app.workbench_for_pane(id).map(|workbench| {
            let layout = mere::platen::project_tile_layout(workbench);
            if visible_canvas {
                return layout;
            }
            let mut children = vec![layout];
            children.extend(docs.take().unwrap_or_default());
            let mut root = Node::new(Role::Group);
            root.set_label("Workbench documents");
            stitch(
                &format!("turnstone/workbench/{}/documents", id.0),
                root,
                children,
            )
        }),
        PaneContent::Orrery => {
            if orrery_pane.is_none() {
                *orrery_pane = Some(id);
            }
            // The canvas leaf carries the graph summary plus the live
            // documents (their pixels composite over the canvas region).
            let count = app
                .graph_for_pane(id)
                .and_then(|graph| app.graph_runtimes.canvas(graph))
                .map(|canvas| canvas.graph().nodes().count())
                .unwrap_or(0);
            let mut root = Node::new(Role::Group);
            root.set_label(format!("graph canvas, {count} nodes"));
            let subtrees = docs.take().unwrap_or_default();
            Some(stitch("turnstone/canvas", root, subtrees))
        },
        PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION => {
            Some(project_frozen_projection(app))
        },
        // Promoted read-only Diagnostics rows and Arrange refusal status contribute here;
        // active built-in controls retain their separate qualification gate.
        PaneContent::Gloss(_) | PaneContent::Registered(_) => contribution(id),
        _ => None,
    });
    let mut root = Node::new(Role::Window);
    root.set_label("Turnstone");
    stitch("turnstone", root, vec![chrome, panes])
}
