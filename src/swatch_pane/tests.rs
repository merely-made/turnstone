// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Update;

fn gloss_pane_on_sample_graph() -> (SwatchPane, App) {
    let mut app = App::test_stub();
    *app.graph_runtimes.active_canvas_mut() = mere::canvas::Canvas::with_sample_graph();
    let mut pane = SwatchPane::new(GLOSS_MINIMAP);
    pane.sync(&app, 480.0, 400.0);
    let _ = pane.scene(480, 400, 1.0);
    (pane, app)
}

fn overmap_pane_on_fork_pair() -> (SwatchPane, App, crate::panes::SessionId) {
    let mut app = App::test_stub();
    let donor = app.session_id;
    let mut donor_m = pandect::GraphSessionManifest::new(
        donor,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xd0)),
    );
    donor_m.display_name = Some("home".to_string());
    app.sessions.insert(donor_m);
    let fork = crate::panes::SessionId::new();
    let mut fork_m = pandect::GraphSessionManifest::new(
        fork,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xf0)),
    );
    fork_m.parent_session = Some(donor);
    app.sessions.insert(fork_m);
    app.session_id = fork;
    let mut pane = SwatchPane::new(OVERMAP_LINEAGE);
    pane.sync(&app, 480.0, 400.0);
    let _ = pane.scene(480, 400, 1.0);
    (pane, app, donor)
}

/// The leaf pipeline end to end, headless, for the minimap preset — the
/// pipeline every leaf consumer reuses (the old GlossPane's charter).
#[test]
fn minimap_preset_paints_through_the_pipeline() {
    let (mut pane, _app) = gloss_pane_on_sample_graph();
    assert!(!pane.runner.state().swatch.graph.nodes.is_empty());
    assert!(
        !pane.runner.state().swatch.graph.edges.is_empty(),
        "edge endpoints must match back to nodes bit-exactly"
    );
    assert!(
        !pane.runner.state().swatch.show_labels,
        "minimap stays bare"
    );
    let _scene = pane.scene(480, 400, 1.0);
    assert!(
        pane.rendered
            .get(GLOSS_MINIMAP.leaf_key)
            .is_some_and(|c| !c.is_empty()),
        "the minimap leaf must render paint commands at its laid-out box"
    );
}

/// A minimap node click drains the Open activation for that node's url —
/// the intent riding the node as preset data.
#[test]
fn clicking_a_minimap_node_activates_open() {
    let (mut pane, _app) = gloss_pane_on_sample_graph();
    let key = pane.runner.state().swatch.graph.nodes[0]
        .key
        .clone()
        .expect("every minimap node carries its url as a key");
    let (x, y) = pane
        .resolve(
            &taproot::Selector::class("graph-canvas-swatch-node").with_attr("data-key", &key),
            [0.0, 0.0, 480.0, 400.0],
        )
        .expect("the node must resolve by its data-key");
    let intents = pane.click(x, y, 480, 400);
    assert!(
        matches!(&intents[..], [SwatchIntent::Activate(SwatchActivate::Open(url))] if *url == key),
        "a node click drains Open for that node's url, got {intents:?}"
    );
}

fn diagnostic_fixture() -> (SwatchPane, App) {
    let mut app = App::test_stub();
    app.diagnostic_inspection = Some(Ok(apparatus_core::Inspection {
        lines: (0..30)
            .map(|index| apparatus_core::InspectionLine {
                id: format!("apparatus/fixture/record/{index}"),
                text: format!("Diagnostic record {index}"),
            })
            .collect(),
    }));
    let mut pane = SwatchPane::new(GLOSS_MINIMAP);
    pane.set_sections(vec![crate::sections::DIAGNOSTICS_SECTION]);
    pane.sync(&app, 480.0, 400.0);
    pane.scene(480, 400, 1.0);
    (pane, app)
}

fn diagnostic_dom_node(pane: &SwatchPane, identity: &str) -> NodeId {
    let dom = pane.dom.borrow();
    let mut pending = vec![dom.document()];
    while let Some(node) = pending.pop() {
        pending.extend(dom.dom_children(node));
        if dom.attribute(node, &Default::default(), &"data-diagnostic-id".into()) == Some(identity)
        {
            return node;
        }
    }
    panic!("missing diagnostic DOM row {identity}");
}

#[test]
fn diagnostic_rows_keep_producer_identity_and_retained_bounds_without_activation() {
    let (mut pane, mut app) = diagnostic_fixture();
    let placement = crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0);
    let id = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/0");
    let tree = pane
        .diagnostic_tree(crate::panes::PaneId(7), placement)
        .unwrap();
    let (_, row) = tree
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap();
    assert_eq!(row.label(), Some("Diagnostic record 0"));
    assert!(tree.nodes.iter().all(|(_, node)| {
        !node.supports_action(accesskit::Action::Click)
            && !node.supports_action(accesskit::Action::Focus)
    }));
    let dom_node = diagnostic_dom_node(&pane, "apparatus/fixture/record/0");
    let visible = pane
        .layout
        .visible_rect(&pane.dom.borrow(), dom_node)
        .unwrap();
    assert_eq!(
        row.bounds(),
        Some(accesskit::Rect::new(
            f64::from(placement.x + visible.0),
            f64::from(placement.y + visible.1),
            f64::from(placement.x + visible.0 + visible.2),
            f64::from(placement.y + visible.1 + visible.3),
        ))
    );
    assert!(
        pane.click(
            visible.0 + visible.2 / 2.0,
            visible.1 + visible.3 / 2.0,
            480,
            400
        )
        .is_empty()
    );
    let Some(Ok(inspection)) = app.diagnostic_inspection.as_mut() else {
        unreachable!()
    };
    inspection.lines[0].text = "Diagnostic record updated".into();
    inspection.lines.insert(
        0,
        apparatus_core::InspectionLine {
            id: "apparatus/fixture/new".into(),
            text: "New earlier row".into(),
        },
    );
    pane.sync(&app, 480.0, 400.0);
    pane.scene(480, 400, 1.0);
    let updated = pane
        .diagnostic_tree(crate::panes::PaneId(7), placement)
        .unwrap();
    assert_eq!(
        updated
            .nodes
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .unwrap()
            .1
            .label(),
        Some("Diagnostic record updated")
    );
    let another = pane
        .diagnostic_tree(crate::panes::PaneId(8), placement)
        .unwrap();
    assert!(
        !another.nodes.iter().any(|(candidate, _)| *candidate == id),
        "pane identity scopes shared inspection rows"
    );
}

#[test]
fn diagnostic_subtree_omits_clipped_hidden_and_removed_rows() {
    use layout_dom_api::{LayoutDomMut, QualName};
    let (mut pane, _app) = diagnostic_fixture();
    let placement = crate::surface::Rect::new(0.0, 0.0, 480.0, 400.0);
    let id = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/0");
    let tree = pane
        .diagnostic_tree(crate::panes::PaneId(7), placement)
        .unwrap();
    let clipped = uxtree::node_id_for_path("turnstone/gloss/7/apparatus/fixture/record/29");
    assert!(
        !tree
            .nodes
            .iter()
            .any(|(candidate, _)| *candidate == clipped),
        "the below-container row is absent, not assigned invented bounds"
    );
    let root_bounds = tree
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == tree.root)
        .unwrap()
        .1
        .bounds()
        .unwrap();
    let dom_node = diagnostic_dom_node(&pane, "apparatus/fixture/record/0");
    pane.dom.borrow_mut().set_attribute(
        dom_node,
        QualName::new(None, "".into(), "style".into()),
        &format!(
            "position:absolute;left:0;top:{}px;width:150px;height:20px;padding:0",
            root_bounds.height() - 5.0
        ),
    );
    pane.scene(480, 400, 1.0);
    let partial = pane
        .diagnostic_tree(crate::panes::PaneId(7), placement)
        .unwrap();
    let bounds = partial
        .nodes
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .unwrap()
        .1
        .bounds()
        .unwrap();
    assert!(
        bounds.height() > 0.0 && bounds.height() < 20.0,
        "ancestor clipping supplies the actual visible part: {bounds:?}"
    );
    pane.dom.borrow_mut().set_attribute(
        dom_node,
        QualName::new(None, "".into(), "aria-hidden".into()),
        "true",
    );
    pane.scene(480, 400, 1.0);
    assert!(
        !pane
            .diagnostic_tree(crate::panes::PaneId(7), placement)
            .unwrap()
            .nodes
            .iter()
            .any(|(candidate, _)| *candidate == id)
    );
    pane.set_sections(Vec::new());
    let app = App::test_stub();
    pane.sync(&app, 480.0, 400.0);
    pane.scene(480, 400, 1.0);
    assert!(
        pane.diagnostic_tree(crate::panes::PaneId(7), placement)
            .is_none(),
        "removing the section retires the whole subtree"
    );
}

/// The overmap draws a fork's lineage and a picked-off part's apart: each
/// a kinded relation with its own line and words.
#[test]
fn overmap_preset_draws_pick_lineage_beside_fork_lineage() {
    let (mut pane, mut app, donor) = overmap_pane_on_fork_pair();
    let picked = crate::panes::SessionId::new();
    let mut picked_m = pandect::GraphSessionManifest::new(
        picked,
        crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xe0)),
    );
    picked_m.display_name = Some("home (picked from home)".to_string());
    picked_m.picked_from = Some(pandect::PickedFrom {
        session: donor,
        at: 0,
        container: uuid::Uuid::from_u128(0xd0),
    });
    app.sessions.insert(picked_m);
    pane.sync(&app, 480.0, 400.0);
    let state = pane.runner.state();
    let kinds: Vec<(&str, &str)> = state
        .swatch
        .relations
        .iter()
        .map(|relation| (relation.kind.as_str(), relation.label.as_str()))
        .collect();
    assert_eq!(kinds.len(), 2, "{kinds:?}");
    assert!(
        kinds.iter().any(|(kind, _)| *kind == "forked-from"),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("picked-from", "home (picked from home) picked from home")),
        "{kinds:?}"
    );
    assert!(
        state
            .swatch
            .relation_lines
            .contains(&("picked-from".to_string(), sprigging::RelationLine::Dashed))
    );
}

/// The overmap preset renders both sessions + the lineage edge, labels
/// on, and a session-node click drains the Switch activation.
#[test]
fn overmap_preset_renders_lineage_and_switches() {
    let (mut pane, _app, donor) = overmap_pane_on_fork_pair();
    assert_eq!(pane.runner.state().swatch.graph.nodes.len(), 2);
    assert_eq!(pane.runner.state().swatch.graph.edges.len(), 1);
    assert!(pane.runner.state().swatch.show_labels, "identity labels on");
    assert_eq!(
        pane.runner.state().swatch.selected,
        Some(uuid::Uuid::from_u128(0xf0)),
        "the current session's container is selected"
    );
    let _scene = pane.scene(480, 400, 1.0);
    assert!(
        pane.rendered
            .get(OVERMAP_LINEAGE.leaf_key)
            .is_some_and(|c| !c.is_empty())
    );
    let (x, y) = pane
        .resolve(
            &taproot::Selector::class("graph-canvas-swatch-node")
                .with_attr("data-key", &donor.0.to_string()),
            [0.0, 0.0, 480.0, 400.0],
        )
        .expect("the donor session node resolves by its data-key");
    let intents = pane.click(x, y, 480, 400);
    assert!(
        matches!(&intents[..], [SwatchIntent::Activate(SwatchActivate::Switch(id))] if *id == donor),
        "a session-node click drains Switch for that session, got {intents:?}"
    );
}

/// Hover transitions set and clear the emphasis, preset-agnostically.
///
/// The emphasis is the canvas component's own state, so this reads it
/// where it is now observable: the rendered node's class. The pane no
/// longer holds a hover field to assert against, which is the point.
#[test]
fn hovering_sets_and_clears_emphasis() {
    let (mut pane, _app, donor) = overmap_pane_on_fork_pair();
    let selector = taproot::Selector::class("graph-canvas-swatch-node")
        .with_attr("data-key", &donor.0.to_string());
    let (x, y) = pane
        .resolve(&selector, [0.0, 0.0, 480.0, 400.0])
        .expect("the donor session node resolves");
    let key = donor.0.to_string();
    let hovered_class = |pane: &SwatchPane| {
        use layout_dom_api::LayoutDom;
        let dom = pane.dom.borrow();
        dom.all_with_class(dom.document(), "graph-canvas-swatch-node")
            .into_iter()
            .filter(|node| {
                dom.attribute(
                    *node,
                    &layout_dom_api::Namespace::from(""),
                    &layout_dom_api::LocalName::from("data-key"),
                ) == Some(key.as_str())
            })
            .any(|node| {
                dom.attribute(
                    node,
                    &layout_dom_api::Namespace::from(""),
                    &layout_dom_api::LocalName::from("class"),
                )
                .is_some_and(|class| class.contains("hovered"))
            })
    };

    assert!(pane.hover(x, y, 480, 400), "entering the node is a change");
    assert!(hovered_class(&pane), "the canvas renders its own emphasis");
    assert!(!pane.hover(x, y, 480, 400), "same target, no re-dispatch");
    assert!(pane.hover_leave(), "leaving the pane is a change");
    assert!(!hovered_class(&pane), "emphasis cleared");
}

/// A recovered content state colors the minimap node open — the preset
/// gather reads the same lifecycle the canvas colors by.
#[test]
fn minimap_nodes_color_by_content_state() {
    let (mut pane, mut app) = gloss_pane_on_sample_graph();
    let id = app
        .graph_runtimes
        .graph()
        .nodes()
        .next()
        .map(|(_, n)| n.id)
        .unwrap();
    app.apply_update(Update::ContentSpawned {
        node: id,
        facts: None,
    });
    pane.sync(&app, 480.0, 400.0);
    let node = pane
        .runner
        .state()
        .swatch
        .graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap();
    assert_eq!(node.kind, NodeState::Open, "live content reads open");
}
