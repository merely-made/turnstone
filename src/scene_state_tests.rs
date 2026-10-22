// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use crate::panes::GraphId;
use crate::scene_state::{self, ViewState};
use mere::canvas::Canvas;
use mere::canvas::controls::CanvasCommand;
use pandect::{CartographyGeometry, SavedMosaic, SavedRegion};
use mere::canvas::mosaic::MosaicTarget;
use mere::kernel::graph::Graph;
use serde_json::{Value, json};
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

/// A scene staged on the container and reopened from the real facet file.
fn reopen(source: &mut Canvas, container: Uuid) -> Canvas {
    scene_state::stage_scene(source, container, &ViewState::default());
    let dir = tempfile::tempdir().unwrap();
    pandect::save_node_facets(dir.path(), source.facets()).unwrap();
    assert!(pandect::node_facets_path(dir.path()).is_file());
    let loaded = pandect::load_node_facets(dir.path()).unwrap().unwrap();
    let mut reopened = fresh_canvas(source);
    reopened.overlay_facets(loaded);
    let scene = scene_state::read_scene(reopened.facets(), container)
        .expect("the scene is on the container")
        .expect("and reads");
    reopened.apply_scene(&scene, None).unwrap();
    reopened
}

#[test]
fn scene_state_round_trips_through_the_real_facet_file_into_a_fresh_canvas() {
    let (mut source, members) = fixture();
    let container = Uuid::new_v4();
    let geometry = source.cartography_geometry();
    assert!(!geometry.regions().is_empty());
    assert!(!geometry.mosaic().unwrap().scale.is_empty());
    let mut reopened = reopen(&mut source, container);
    assert!(
        members
            .iter()
            .any(|id| source.graph().get_node_key_by_id(*id)
                != reopened.graph().get_node_key_by_id(*id)),
        "fixture really changed arena allocation"
    );
    let restored = reopened.cartography_geometry();
    assert_eq!(seed_map(&restored), seed_map(&geometry));
    assert_eq!(restored.regions(), geometry.regions());
    assert_same_mosaic(restored.mosaic().unwrap(), geometry.mosaic().unwrap());
    assert_eq!(restored.iter().collect::<BTreeMap<_, _>>(), geometry.iter().collect::<BTreeMap<_, _>>());
    // Reset must use the restored saved seed, even after a subsequent move.
    reopened.seed_cartography(members.iter().map(|id| (*id, (900.0, 700.0))));
    reopened.reset_to_seed();
    let positions: BTreeMap<_, _> = reopened.cartography_geometry().iter().collect();
    for (id, at) in seed_map(&geometry) {
        assert_eq!(positions[&id], at);
    }
}

#[test]
fn camera_selection_and_pause_ride_the_scene_as_graphshell_saves_them() {
    let (mut source, members) = fixture();
    source.set_camera(mere::canvas::CameraView {
        offset: (120.0, -45.0),
        zoom: 1.75,
    });
    source.set_selected_members(&members[..2]);
    source.set_physics_paused(true);
    let reopened = reopen(&mut source, Uuid::new_v4());
    assert_eq!(reopened.camera().offset, (120.0, -45.0));
    assert_eq!(reopened.camera().zoom, 1.75);
    assert_eq!(
        reopened.selected_members().into_iter().collect::<std::collections::BTreeSet<_>>(),
        members[..2].iter().copied().collect()
    );
    assert!(reopened.physics_paused());
}

#[test]
fn restoring_recipe_seed_replaces_the_saved_seed_without_losing_regions_or_mosaic() {
    let (mut source, _) = fixture();
    let container = Uuid::new_v4();
    scene_state::stage_scene(&mut source, container, &ViewState::default());
    let staged = scene_state::read_scene(source.facets(), container).unwrap().unwrap();
    assert!(staged.cartography.seed().is_some());
    assert!(source.restore_recipe_seed());
    let reopened = reopen(&mut source, container);
    let scene = scene_state::read_scene(reopened.facets(), container).unwrap().unwrap();
    assert!(scene.cartography.seed().is_none());
    assert!(!scene.cartography.regions().is_empty());
    assert!(scene.cartography.mosaic().is_some());
    assert!(
        !reopened.has_saved_seed(),
        "old seed must not revive after reopening"
    );
}

#[test]
fn scene_restore_skips_missing_member_identities_across_every_geometry_field() {
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
    let mut scene = source.capture_scene("test", "free", "").unwrap();
    scene.cartography = geometry;
    let mut reopened = fresh_canvas(&source);
    pandect::write_saved_scene(reopened.facets_mut(), container, &scene);
    let stored = scene_state::read_scene(reopened.facets(), container).unwrap().unwrap();
    reopened.apply_scene(&stored, None).unwrap();
    let restored = reopened.cartography_geometry();
    assert_eq!(seed_map(&restored), BTreeMap::from([(kept, (11.0, 22.0))]));
    assert_eq!(restored.regions()[0].members, vec![kept]);
    let mosaic = restored.mosaic().unwrap();
    assert_eq!(mosaic.group_of, vec![(kept, "kept".into())]);
    assert_eq!(mosaic.member_order, vec![kept]);
    assert_eq!(mosaic.scale, vec![(kept, 1.25)]);
}

fn fold_fixture() -> (tempfile::TempDir, GraphId, Canvas) {
    let dir = tempfile::tempdir().unwrap();
    let graph = GraphId::from_uuid(Uuid::new_v4());
    let mut canvas = Canvas::new();
    canvas.visit("mere://fold-a");
    canvas.visit("mere://fold-b");
    let members: Vec<_> = canvas.graph().nodes().map(|(_, n)| n.id).collect();
    canvas.set_selected_members(&members);
    assert!(canvas.fold_selected(scene_state::scope(graph)).is_some());
    (dir, graph, canvas)
}

fn view_file(dir: &std::path::Path) -> std::path::PathBuf {
    crate::session::view_intent_path(dir)
}

#[test]
fn fold_records_round_trip_in_the_view_intent_and_expanding_stays_view_only() {
    let (dir, graph, mut canvas) = fold_fixture();
    let expected = canvas.fold_records().to_vec();
    scene_state::save_view(dir.path(), &canvas, &Default::default(), &ViewState::default()).unwrap();
    canvas.restore_folds(vec![]);
    let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert_eq!(canvas.fold_records(), expected);
    let before = canvas.graph().nodes().count();
    assert!(canvas.expand_selected_fold());
    scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).unwrap();
    assert_eq!(canvas.graph().nodes().count(), before);
    let reopened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert!(canvas.fold_records().is_empty());
    assert!(!reopened.state.read_only);
}

#[test]
fn unreadable_and_interrupted_view_intents_survive_saves_byte_for_byte() {
    let (dir, graph, mut canvas) = fold_fixture();
    for raw in [b"not json".as_slice(), b"[1]"] {
        std::fs::write(view_file(dir.path()), raw).unwrap();
        let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
        assert!(opened.state.read_only);
        assert!(scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).is_err());
        assert_eq!(std::fs::read(view_file(dir.path())).unwrap(), raw);
    }
    std::fs::remove_file(view_file(dir.path())).unwrap();
    let tmp = view_file(dir.path()).with_extension("json.tmp");
    std::fs::write(&tmp, b"interrupted evidence").unwrap();
    let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert!(opened.state.read_only);
    assert!(scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).is_err());
    assert!(!view_file(dir.path()).exists());
    assert_eq!(std::fs::read(tmp).unwrap(), b"interrupted evidence");
}

#[test]
fn stale_foreign_future_and_overlapping_fold_records_are_retained() {
    let (dir, graph, mut canvas) = fold_fixture();
    let valid = json!(canvas.fold_records()[0]);
    let mut foreign = valid.clone();
    foreign["source_scope"] = json!("another graph");
    let mut future = valid.clone();
    future["version"] = json!(100);
    let mut stale = valid.clone();
    stale["members"] = json!([Uuid::new_v4().to_string(), Uuid::new_v4().to_string()]);
    let rejected = vec![valid.clone(), foreign, future, stale];
    let mut raw = vec![valid.clone()];
    raw.extend(rejected.clone());
    std::fs::write(view_file(dir.path()), json!({"folds": raw}).to_string()).unwrap();
    canvas.restore_folds(vec![]);
    let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert_eq!(canvas.fold_records().len(), 1);
    assert_eq!(opened.state.retained_len(), 4);
    scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(view_file(dir.path())).unwrap()).unwrap();
    for raw in rejected {
        assert!(saved["retained_folds"].as_array().unwrap().contains(&raw));
    }
    // Expanding an accepted record cannot resurrect an overlapping one from
    // the retained set at the next ordinary reopen.
    canvas.expand_selected_fold();
    scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).unwrap();
    let reopened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert!(canvas.fold_records().is_empty());
    assert_eq!(reopened.state.retained_len(), 4);
}

#[test]
fn a_session_saved_before_the_view_intent_shapes_is_read_once_and_rewritten() {
    let (dir, graph, mut canvas) = fold_fixture();
    let record = json!(canvas.fold_records()[0]);
    canvas.restore_folds(vec![]);
    std::fs::write(
        view_file(dir.path()),
        json!({
            "layout_strategy": "grid.default",
            "physics_paused": true,
            "command_menu": {"added": ["Reseed layout"], "removed": [], "recent": []},
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("folds.json"),
        json!({"version": 1, "records": [record]}).to_string(),
    )
    .unwrap();
    let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert_eq!(canvas.fold_records().len(), 1, "folds.json records are taken in");
    assert_eq!(opened.commands.added, ["Reseed layout"], "the menu is carried");
    scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).unwrap();
    assert!(!dir.path().join("folds.json").exists(), "the save holds its records");
    let saved: Value = serde_json::from_slice(&std::fs::read(view_file(dir.path())).unwrap()).unwrap();
    assert!(saved.get("layout_strategy").is_none(), "the flat fields are gone");
    assert_eq!(saved["folds"].as_array().unwrap().len(), 1);
    assert_eq!(saved["commands"]["added"], json!(["Reseed layout"]));
    canvas.restore_folds(vec![]);
    let again = scene_state::open_view(dir.path(), graph, &mut canvas);
    assert_eq!(canvas.fold_records().len(), 1);
    assert_eq!(again.commands.added, ["Reseed layout"]);
}

#[test]
fn an_unreadable_fold_sidecar_is_left_in_place() {
    let (dir, graph, mut canvas) = fold_fixture();
    canvas.restore_folds(vec![]);
    for raw in [b"not json".as_slice(), br#"{"version":99,"records":[]}"#] {
        std::fs::write(dir.path().join("folds.json"), raw).unwrap();
        let opened = scene_state::open_view(dir.path(), graph, &mut canvas);
        assert!(!opened.state.read_only, "the view intent itself is fine");
        scene_state::save_view(dir.path(), &canvas, &opened.commands, &opened.state).unwrap();
        assert_eq!(std::fs::read(dir.path().join("folds.json")).unwrap(), raw);
    }
}
