// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::canvas::Canvas;
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::{EdgeAssertion, ProvenanceSubKind};
use scenograph::attention::Attention;

const URLS: [&str; 4] = [
    "https://woodshed.example/progression",
    "https://woodshed.example/take",
    "https://knot.example/lyrics",
    "https://sources.example/chords",
];

fn fixture() -> (App, PaneId, Vec<uuid::Uuid>) {
    let mut app = App::test_stub();
    let pane = app.default_graph_pane();
    let members = app
        .with_graph_pane(pane, |canvas| {
            for url in URLS {
                canvas.visit(url);
            }
            let members: Vec<_> = URLS
                .iter()
                .map(|url| canvas.graph().get_node_by_url(url).unwrap().1.id)
                .collect();
            assert!(canvas.tag_node(members[0], "song"));
            assert!(canvas.tag_node(members[0], "practice"));
            assert!(canvas.tag_node(members[2], "song"));
            assert!(canvas.tag_node(members[2], "publishing"));
            canvas.ingest_graph(|graph| {
                let keys: Vec<_> = members
                    .iter()
                    .map(|id| graph.get_node_key_by_id(*id).unwrap())
                    .collect();
                mere::kernel::graph::apply::assert_relation(
                    graph,
                    keys[0],
                    keys[3],
                    EdgeAssertion::Provenance {
                        sub_kind: ProvenanceSubKind::ExtractedFrom,
                    },
                )
                .unwrap();
                mere::kernel::graph::apply::assert_relation(
                    graph,
                    keys[2],
                    keys[0],
                    EdgeAssertion::Provenance {
                        sub_kind: ProvenanceSubKind::RewrittenFrom,
                    },
                )
                .unwrap();
                true
            });
            let keys: Vec<_> = members.iter().map(|id| canvas.graph().get_node_key_by_id(*id).unwrap()).collect();
            assert!(canvas.graph().node_content_tags(keys[0]).unwrap().contains("song"), "public tag operation must retain canonical source data");
            assert!(canvas.graph().node_content_tags(keys[2]).unwrap().contains("publishing"));
            let relations: Vec<_> = canvas.graph().projected_relations().map(|(_, row)| row).collect();
            for (from, to) in [(keys[0], keys[3]), (keys[2], keys[0])] {
                assert!(relations.iter().any(|r| r.from == from && r.to == to && matches!(r.kind, mere::kernel::graph::RelationKind::Provenance(_))), "fixture source provenance must survive public writes and reconciliation: {relations:?}");
            }
            canvas.resize(800, 600);
            canvas.set_physics_paused(true);
            canvas.clear_selection();
            members
        })
        .unwrap();
    app.focus = FocusTarget::Graph(pane);
    (app, pane, members)
}

fn place(app: &mut App, pane: PaneId, id: &'static str) {
    app.update(Action::SetLayoutStrategy(Some(id)));
    app.with_graph_pane(pane, |canvas| {
        App::drive_layout_strategy_for(canvas, 800, 600);
        canvas.frame(800, 600);
    })
    .unwrap();
}

fn arrange_after_ordinary_graph_tear_out() -> (App, PaneId, PaneId, GraphId) {
    let (mut app, graph_pane, _) = fixture();
    let owner = app.graph_for_pane(graph_pane).unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ARRANGE,
    )));
    let arrange = app.active_pane.unwrap();
    app.active_pane = Some(graph_pane);
    app.focus = FocusTarget::Graph(graph_pane);
    app.update(Action::TearOutActivePane);
    assert!(
        app.lenses.iter().flatten().any(|space| space
            .iter_leaves()
            .any(|(id, content, graph)| id == graph_pane
                && *content == PaneContent::Orrery
                && graph == owner))
    );
    assert!(
        !app.frisket
            .iter_leaves()
            .any(|(_, content, _)| *content == PaneContent::Orrery)
    );
    assert_eq!(app.graph_for_pane(arrange), Some(owner));
    // Ordinary tear-out seeds its destination with an Orrery before moving
    // the original beside it. Close that seed through the normal pane action
    // to establish one explicit selection owner in the receiving window.
    let seed = app
        .lenses
        .iter()
        .flatten()
        .flat_map(|space| space.iter_leaves())
        .find(|(id, content, graph)| {
            *id != graph_pane && **content == PaneContent::Orrery && *graph == owner
        })
        .map(|(id, _, _)| id)
        .expect("tear-out seeds a destination graph pane");
    app.active_pane = Some(seed);
    app.focus = FocusTarget::Graph(seed);
    app.update(Action::CloseActivePane);
    assert_eq!(app.pane_content(seed), None);
    assert_eq!(app.graph_for_pane(graph_pane), Some(owner));
    app.active_pane = Some(arrange);
    app.focus = FocusTarget::Pane(arrange);
    (app, arrange, graph_pane, owner)
}

#[path = "scene_tests/app_adoption_reopens_saved_mosaic_seed_regions_and_cases.rs"]
mod app_adoption_reopens_saved_mosaic_seed_regions_and_cases;
#[path = "scene_tests/arrange_buttons_follow_the_unique_torn_out_graph_cases.rs"]
mod arrange_buttons_follow_the_unique_torn_out_graph_cases;
#[path = "scene_tests/territory_from_read_targets_its_map_and_preserves_cases.rs"]
mod territory_from_read_targets_its_map_and_preserves_cases;
