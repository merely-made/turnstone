// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::graph_scenes;
use mere::canvas::Canvas;
use mere::canvas::controls::CanvasCommand;
use mere::canvas::geometry::{CartographyGeometry, SavedMosaic, SavedRegion};
use mere::canvas::mosaic::MosaicTarget;
use mere::kernel::graph::Graph;
use std::collections::BTreeMap;
use uuid::Uuid;

fn fixture() -> (Canvas, Vec<Uuid>) {
    let mut canvas = Canvas::new();
    let urls = [
        "https://woodshed.example/a",
        "https://woodshed.example/b",
        "https://knot.example/c",
    ];
    for url in urls {
        canvas.visit(url);
    }
    let members: Vec<_> = urls
        .iter()
        .map(|url| canvas.graph().get_node_by_url(url).unwrap().1.id)
        .collect();
    canvas.resize(800, 600);
    canvas.set_layout_strategy(Some("mosaic.default".into()));
    let projection =
        canvas.project_arrangement_for_view("mosaic.default", 800, 600, None, false, 1.0, None);
    canvas.apply_strategy_positions(&projection.positions);
    canvas.set_strategy_regions(projection.regions);
    assert!(canvas.mosaic_move_member(members[2], MosaicTarget::Group("woodshed.example".into())));
    canvas.select_member(members[2]);
    CanvasCommand::GrowTiles.apply(&mut canvas, (800, 600));
    canvas.set_seed();
    (canvas, members)
}

fn fresh_canvas(source: &Canvas) -> Canvas {
    let mut snapshot = source.graph().to_snapshot_at(0);
    // Deliberately allocate the members in the other order. Facets must bind
    // UUIDs, rather than the old Canvas's arena keys.
    snapshot.nodes.reverse();
    let mut canvas = Canvas::with_graph(Graph::from_snapshot(&snapshot));
    canvas.resize(800, 600);
    canvas.set_physics_paused(true);
    canvas
}

fn seed_map(geometry: &CartographyGeometry) -> BTreeMap<Uuid, (f32, f32)> {
    geometry
        .seed()
        .unwrap_or_default()
        .iter()
        .copied()
        .collect()
}

fn assert_same_mosaic(a: &SavedMosaic, b: &SavedMosaic) {
    assert_eq!(
        a.group_of.iter().cloned().collect::<BTreeMap<_, _>>(),
        b.group_of.iter().cloned().collect::<BTreeMap<_, _>>()
    );
    assert_eq!(
        a.scale.iter().copied().collect::<BTreeMap<_, _>>(),
        b.scale.iter().copied().collect::<BTreeMap<_, _>>()
    );
    assert_eq!(a.group_order, b.group_order);
    assert_eq!(a.member_order, b.member_order);
}

#[test]
fn scene_state_round_trips_through_the_real_facet_file_into_a_fresh_canvas() {
    let (source, members) = fixture();
    let container = Uuid::new_v4();
    let geometry = source.cartography_geometry();
    assert!(!geometry.regions().is_empty());
    assert!(!geometry.mosaic().unwrap().scale.is_empty());
    let mut facets = pandect::NodeFacetStore::new();
    pandect::write_arrangement_positions(&mut facets, geometry.iter());
    graph_scenes::write(&mut facets, Some(container), &geometry);
    let dir = tempfile::tempdir().unwrap();
    pandect::save_node_facets(dir.path(), &facets).unwrap();
    assert!(pandect::node_facets_path(dir.path()).is_file());
    drop(facets);
    let loaded = pandect::load_node_facets(dir.path()).unwrap().unwrap();
    let mut reopened = fresh_canvas(&source);
    assert!(
        members
            .iter()
            .any(|id| source.graph().get_node_key_by_id(*id)
                != reopened.graph().get_node_key_by_id(*id)),
        "fixture really changed arena allocation"
    );
    reopened.overlay_facets(loaded);
    reopened.seed_cartography(pandect::read_arrangement_positions(reopened.facets()));
    graph_scenes::restore(&mut reopened, Some(container));
    let restored = reopened.cartography_geometry();
    assert_eq!(seed_map(&restored), seed_map(&geometry));
    assert_eq!(restored.regions(), geometry.regions());
    assert_same_mosaic(restored.mosaic().unwrap(), geometry.mosaic().unwrap());
    // Reset must use the restored saved seed, even after a subsequent move.
    reopened.seed_cartography(members.iter().map(|id| (*id, (900.0, 700.0))));
    reopened.reset_to_seed();
    let positions: BTreeMap<_, _> = reopened.cartography_geometry().iter().collect();
    for (id, at) in seed_map(&geometry) {
        assert_eq!(positions[&id], at);
    }
}

#[test]
fn restoring_recipe_seed_removes_old_seed_facets_without_losing_regions_or_mosaic() {
    let (mut source, _) = fixture();
    let container = Uuid::new_v4();
    let mut facets = pandect::NodeFacetStore::new();
    graph_scenes::write(&mut facets, Some(container), &source.cartography_geometry());
    assert!(pandect::read_arrangement_seed(&facets).is_some());
    assert!(source.restore_recipe_seed());
    graph_scenes::write(&mut facets, Some(container), &source.cartography_geometry());
    assert!(pandect::read_arrangement_seed(&facets).is_none());
    assert!(!pandect::read_arrangement_regions(&facets, container).is_empty());
    assert!(pandect::read_arrangement_mosaic(&facets, container).is_some());
    let dir = tempfile::tempdir().unwrap();
    pandect::save_node_facets(dir.path(), &facets).unwrap();
    let mut reopened = fresh_canvas(&source);
    reopened.overlay_facets(pandect::load_node_facets(dir.path()).unwrap().unwrap());
    graph_scenes::restore(&mut reopened, Some(container));
    assert!(
        !reopened.has_saved_seed(),
        "old seed must not revive after reopening"
    );
}

#[test]
fn clearing_scene_regions_and_mosaic_preserves_positions_and_the_saved_seed() {
    let (source, _) = fixture();
    let container = Uuid::new_v4();
    let geometry = source.cartography_geometry();
    let mut facets = pandect::NodeFacetStore::new();
    pandect::write_arrangement_positions(&mut facets, geometry.iter());
    graph_scenes::write(&mut facets, Some(container), &geometry);
    let positions = pandect::read_arrangement_positions(&facets);
    let cleared =
        CartographyGeometry::default().with_seed(geometry.seed().map(|s| s.iter().copied()));
    graph_scenes::write(&mut facets, Some(container), &cleared);
    assert!(pandect::read_arrangement_regions(&facets, container).is_empty());
    assert!(pandect::read_arrangement_mosaic(&facets, container).is_none());
    assert_eq!(pandect::read_arrangement_positions(&facets), positions);
    let seed: BTreeMap<_, _> = pandect::read_arrangement_seed(&facets)
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(seed, seed_map(&geometry));
}

#[test]
fn scene_restore_skips_missing_member_identities_across_all_new_facet_families() {
    let (source, members) = fixture();
    let kept = members[0];
    let missing = Uuid::new_v4();
    let container = Uuid::new_v4();
    let geometry = CartographyGeometry::default()
        .with_seed(Some([(kept, (11.0, 22.0)), (missing, (33.0, 44.0))]))
        .with_regions([SavedRegion {
            contour: vec![(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)],
            label: Some("persisted region".into()),
            members: vec![missing, kept],
        }])
        .with_mosaic(Some(SavedMosaic {
            group_of: vec![(missing, "gone".into()), (kept, "kept".into())],
            group_order: vec!["kept".into()],
            member_order: vec![missing, kept],
            scale: vec![(missing, 4.0), (kept, 1.25)],
        }));
    let mut facets = pandect::NodeFacetStore::new();
    graph_scenes::write(&mut facets, Some(container), &geometry);
    let mut reopened = fresh_canvas(&source);
    reopened.overlay_facets(facets);
    graph_scenes::restore(&mut reopened, Some(container));
    let restored = reopened.cartography_geometry();
    assert_eq!(seed_map(&restored), BTreeMap::from([(kept, (11.0, 22.0))]));
    assert_eq!(restored.regions()[0].members, vec![kept]);
    let mosaic = restored.mosaic().unwrap();
    assert_eq!(mosaic.group_of, vec![(kept, "kept".into())]);
    assert_eq!(mosaic.member_order, vec![kept]);
    assert_eq!(mosaic.scale, vec![(kept, 1.25)]);
}
