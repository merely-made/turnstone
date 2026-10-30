// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use sceno::{Arrangement, Spiral};

// Future-supplier controls: these require the coordinated immutable
// resource supplier. They cover only load and migration re-save placement.
fn recorded_session_fixture() -> Graph {
    use mere::kernel::graph::apply::add_node;
    let mut graph = Graph::new();
    add_node(
        &mut graph,
        None,
        "https://placement.test/dir/page".into(),
        Default::default(),
    );
    add_node(
        &mut graph,
        None,
        "https://placement.test/dir/".into(),
        Default::default(),
    );
    let mut snapshot = graph.to_snapshot();
    snapshot.edges.clear();
    snapshot.resource_edges.clear();
    let recorded = Graph::try_from_recorded_snapshot(&snapshot).unwrap();
    let unqualified = Graph::try_from_snapshot(&snapshot).unwrap().to_snapshot();
    assert!(
        unqualified.edges.len() + unqualified.resource_edges.len() > 0,
        "the deliberately broken unprofiled load re-derives URL containment"
    );
    recorded
}

// Legacy pixels are deserialize-only fields. Build their historical wire
// form explicitly; serializing PersistedNode intentionally omits them.
fn with_inline_thumbnail(mut wire: serde_json::Value) -> Vec<u8> {
    wire["nodes"][0]["thumbnail_png"] =
        serde_json::json!(encode_rgba_png(&[1, 2, 3, 255], 1, 1).unwrap());
    wire["nodes"][0]["thumbnail_width"] = serde_json::json!(1);
    wire["nodes"][0]["thumbnail_height"] = serde_json::json!(1);
    serde_json::to_vec_pretty(&wire).unwrap()
}

#[test]
fn session_profile_corrupt_facets_preserve_graph_labels_and_deposit_no_images() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(session_graph_store::GRAPH_FILE);
    let mut snapshot = recorded_session_fixture().to_snapshot();
    snapshot.nodes[0].tags = vec!["keep".into(), "Paper".into()];
    let bytes = with_inline_thumbnail(serde_json::to_value(&snapshot).unwrap());
    std::fs::write(&path, &bytes).unwrap();
    let facets = pandect::node_facets_path(root.path());
    std::fs::write(&facets, b"{bad facets").unwrap();
    assert!(try_load_session_graph(root.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read(facets).unwrap(), b"{bad facets");
    assert!(!root.path().join("images").exists());
}

#[test]
fn session_profile_migration_write_failure_preserves_raw_control_evidence() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(session_graph_store::GRAPH_FILE);
    let mut snapshot = recorded_session_fixture().to_snapshot();
    snapshot.nodes[0].tags = vec!["keep".into(), "Paper".into()];
    let bytes = serde_json::to_vec_pretty(&snapshot).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let blocked_tmp = pandect::node_facets_path(root.path()).with_extension("json.tmp");
    std::fs::create_dir(&blocked_tmp).unwrap();
    assert!(try_load_session_graph(root.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir(blocked_tmp).unwrap();
    let input = try_load_session_graph(root.path()).unwrap().unwrap();
    let first = uuid::Uuid::parse_str(&snapshot.nodes[0].node_id).unwrap();
    assert!(crate::surface_controls::read(
        &input.graph,
        first,
        crate::surface_controls::Control::Keep
    ));
    let saved = crate::session_graph_file::load_profiled_snapshot(&path)
        .unwrap()
        .unwrap();
    assert_eq!(saved.placement, None);
    assert!(
        saved
            .snapshot
            .nodes
            .iter()
            .all(|node| !node.tags.iter().any(|tag| tag == "keep"))
    );
}

#[test]
fn session_profile_recorded_load_and_migration_preserve_held_relations() {
    use mere::kernel::types::ImageRole;
    use pandect::graph_placement::PlacementProfile;
    for image_migration in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(session_graph_store::GRAPH_FILE);
        let graph = recorded_session_fixture();
        crate::session_graph_file::save_profiled(
            &path,
            &graph,
            Some(PlacementProfile::RecordedStrataV1),
        )
        .unwrap();
        let mut input = crate::session_graph_file::load_profiled_snapshot(&path)
            .unwrap()
            .unwrap();
        let migrated_member = input.snapshot.nodes[0].node_id.clone();
        let bytes = if image_migration {
            with_inline_thumbnail(serde_json::to_value(&input).unwrap())
        } else {
            input.snapshot.nodes[0].is_pinned = true;
            serde_json::to_vec_pretty(&input).unwrap()
        };
        std::fs::write(&path, bytes).unwrap();
        let restored = load_session_graph(root.path()).expect("qualified recorded input");
        let restored_snapshot = restored.to_snapshot();
        assert!(restored_snapshot.edges.is_empty());
        assert!(restored_snapshot.resource_edges.is_empty());
        let saved = crate::session_graph_file::load_profiled_snapshot(&path)
            .unwrap()
            .unwrap();
        assert_eq!(saved.placement, Some(PlacementProfile::RecordedStrataV1));
        assert_eq!(saved.snapshot.legacy_image_count(), 0);
        assert_eq!(saved.snapshot.legacy_node_facet_count(), 0);
        if image_migration {
            let migrated = saved
                .snapshot
                .nodes
                .iter()
                .find(|node| node.node_id == migrated_member)
                .unwrap();
            let image = migrated
                .images
                .get(&ImageRole::Preview)
                .expect("the migrated member retains its preview reference");
            assert!(load_image_blob(root.path(), &image.hex()).is_some());
        }
        let reopened = load_session_graph(root.path()).unwrap().to_snapshot();
        assert_eq!(reopened.edges, saved.snapshot.edges);
        assert_eq!(reopened.resource_edges, saved.snapshot.resource_edges);
    }
}

#[test]
fn session_profile_invalid_resource_and_declared_legacy_refuse_without_rewrite() {
    use mere::kernel::persistence::PersistedShownResource;
    use pandect::graph_placement::PlacementProfile;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(session_graph_store::GRAPH_FILE);
    let mut invalid = recorded_session_fixture().to_snapshot();
    invalid.shown_resources.push(PersistedShownResource {
        surface_id: "not-a-uuid".into(),
        resource_id: uuid::Uuid::new_v4().to_string(),
    });
    std::fs::write(
        &path,
        with_inline_thumbnail(serde_json::to_value(&invalid).unwrap()),
    )
    .unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(
        load_session_graph(root.path()).is_none(),
        "invalid explicit resource data is fallible"
    );
    assert!(
        !root.path().join("images").exists(),
        "refused input deposits no migration blobs"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    crate::session_graph_file::save_profiled(
        &path,
        &Graph::new(),
        Some(PlacementProfile::LegacySurfaceV1),
    )
    .unwrap();
    let original = std::fs::read(&path).unwrap();
    assert!(
        load_session_graph(root.path()).is_none(),
        "declared legacy needs qualified replay"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn session_profile_absent_migration_does_not_mint_qualification() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(session_graph_store::GRAPH_FILE);
    let mut snapshot = recorded_session_fixture().to_snapshot();
    snapshot.nodes[0].is_pinned = true;
    std::fs::write(&path, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
    assert!(load_session_graph(root.path()).is_some());
    let saved = crate::session_graph_file::load_profiled_snapshot(&path)
        .unwrap()
        .unwrap();
    assert_eq!(saved.placement, None);
    assert_eq!(saved.snapshot.legacy_node_facet_count(), 0);
}

#[test]
fn place_collection_sidecar_is_exact_and_refuses_corruption() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(load_place_collection(root.path()).unwrap(), None);
    let selected = crate::place::PlaceCollectionVersion {
        moot: crate::place::PlaceId([1; 32]),
        collection: crate::place::PlaceCollectionId([2; 32]),
        frontier: vec![[3; 32], [4; 32]],
        membership_commitment: [5; 32],
    };
    save_place_collection(root.path(), Some(&selected)).unwrap();
    assert_eq!(load_place_collection(root.path()).unwrap(), Some(selected));
    save_place_collection(root.path(), None).unwrap();
    assert_eq!(load_place_collection(root.path()).unwrap(), None);
    std::fs::write(
        root.path().join(PLACE_COLLECTION_FILE),
        b"{\"version\":2,\"selection\":null}",
    )
    .unwrap();
    assert!(
        load_place_collection(root.path())
            .unwrap_err()
            .contains("unsupported")
    );
    std::fs::write(root.path().join(PLACE_COLLECTION_FILE), br#"{"version":1}"#).unwrap();
    assert!(
        load_place_collection(root.path())
            .unwrap_err()
            .contains("decode")
    );
    std::fs::write(root.path().join(PLACE_COLLECTION_FILE), b"{").unwrap();
    assert!(
        load_place_collection(root.path())
            .unwrap_err()
            .contains("decode")
    );
}

fn temp_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "turnstone-session-test-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The flat single-session layout migrates into `sessions/<id>/` exactly
/// once: the sidecars MOVE (the flat graph is gone), the manifest is
/// written, and `pick_session` finds the minted session. A second boot
/// with a populated store migrates nothing.
#[test]
fn flat_layout_migrates_into_sessions_once() {
    let root = temp_root("migrate");
    std::fs::write(root.join(session_graph_store::GRAPH_FILE), b"{}").unwrap();
    std::fs::write(root.join(frisket_store::FRAME_FILE), b"{}").unwrap();

    let mut store = load_manifests(&root);
    let id = migrate_flat_layout(&root, &mut store).expect("the flat layout migrates");
    let dir = session_dir(&root, id);
    assert!(dir.join(session_graph_store::GRAPH_FILE).exists());
    assert!(dir.join(frisket_store::FRAME_FILE).exists());
    assert!(dir.join("manifest.json").exists());
    assert!(
        !root.join(session_graph_store::GRAPH_FILE).exists(),
        "the flat graph MOVED, not copied"
    );
    assert_eq!(store.len(), 1);
    assert_eq!(pick_session(&root, &store), Some(id));

    // Reload from disk (a second boot): nothing migrates again.
    let mut store2 = load_manifests(&root);
    assert_eq!(store2.len(), 1);
    assert_eq!(migrate_flat_layout(&root, &mut store2), None);
}

/// The current-session marker round-trips, and a stale marker (a session
/// that no longer exists) falls back to the most recent manifest.
#[test]
fn current_session_marker_round_trips_and_falls_back() {
    let root = temp_root("current");
    let mut store = ManifestStore::with_root(sessions_root(&root));
    let a = SessionId::new();
    let b = SessionId::new();
    let mut ma = GraphSessionManifest::new(a, crate::panes::GraphId::nil());
    let mut mb = GraphSessionManifest::new(b, crate::panes::GraphId::nil());
    // Creation and later mutation are explicit fixture facts. Immediate
    // wall-clock calls can tie, making HashMap iteration decide this test.
    let at = |seconds| std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds);
    ma.created_at = at(1);
    mb.created_at = at(2);
    mb.updated_at = at(2);
    ma.updated_at = at(3);
    store.insert(ma);
    store.insert(mb);

    record_current_session(&root, a);
    assert_eq!(pick_session(&root, &store), Some(a));
    record_current_session(&root, SessionId::new());
    // The recorded id is unknown; the newest manifest wins. `a` was
    // touched later than `b` was created, so updated_at prefers `a`.
    assert_eq!(pick_session(&root, &store), Some(a));
}

#[test]
fn projection_score_sidecar_round_trips_without_becoming_graph_truth() {
    let root = temp_root("projection-score");
    let score = Score::new(Arrangement::Spiral(Spiral::default()));
    save_projection_score(&root, &score);
    assert_eq!(load_projection_score(&root), Some(score));
    assert!(projection_score_path(&root).exists());
    assert!(
        !root.join(session_graph_store::GRAPH_FILE).exists(),
        "the score remains a view sidecar"
    );
}

#[test]
fn place_binding_round_trips_as_a_distinct_public_sidecar() {
    let root = temp_root("place-binding");
    let binding = PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "commons",
    )
    .unwrap();

    assert_eq!(load_place_binding(&root).unwrap(), None);
    save_place_binding(&root, &binding).unwrap();
    assert_eq!(load_place_binding(&root).unwrap(), Some(binding));
    assert!(
        !root.join(session_graph_store::GRAPH_FILE).exists(),
        "the place binding remains beside graph truth"
    );
}

#[test]
fn a_routine_save_updates_a_binding_but_never_mints_one() {
    let root = temp_root("place-binding-update");
    let binding = PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "commons",
    )
    .unwrap();

    // No admitted place yet: a save reports that it wrote nothing rather
    // than creating a binding admission never granted.
    assert_eq!(update_place_binding(&root, &binding).unwrap(), false);
    assert!(!place_binding_path(&root).exists());
    assert_eq!(load_place_binding(&root).unwrap(), None);

    // Once admission has established one, ordinary saves may update it.
    save_place_binding(&root, &binding).unwrap();
    let mut renamed = binding.clone();
    renamed.default_channel = "hall".into();
    assert_eq!(update_place_binding(&root, &renamed).unwrap(), true);
    assert_eq!(load_place_binding(&root).unwrap(), Some(renamed));
}

#[test]
fn removing_place_binding_preserves_retained_session_files() {
    let root = temp_root("place-binding-remove");
    let binding = PlaceBindingV1::new(
        crate::place::PlaceId([0x91; 32]),
        crate::place::SharedContainerId([0x92; 32]),
        crate::place::ChatSpaceId([0x93; 32]),
        "hall",
    )
    .unwrap();
    save_place_binding(&root, &binding).unwrap();
    std::fs::write(root.join("place-history.redb"), b"retained").unwrap();

    assert_eq!(remove_place_binding(&root).unwrap(), true);
    assert_eq!(load_place_binding(&root).unwrap(), None);
    assert!(root.join("place-history.redb").exists());
    assert_eq!(remove_place_binding(&root).unwrap(), false);

    // A malformed path must report failure and preserve what it found.
    let binding_path = place_binding_path(&root);
    std::fs::create_dir(&binding_path).unwrap();
    std::fs::write(binding_path.join("keep"), b"unexpected retained data").unwrap();
    assert!(remove_place_binding(&root).is_err());
    assert!(binding_path.join("keep").exists());
}

#[test]
fn marking_a_place_left_preserves_the_binding_and_retained_session_files() {
    let root = temp_root("place-left-mark");
    let binding = PlaceBindingV1::new(
        crate::place::PlaceId([0x94; 32]),
        crate::place::SharedContainerId([0x95; 32]),
        crate::place::ChatSpaceId([0x96; 32]),
        "hall",
    )
    .unwrap();
    save_place_binding(&root, &binding).unwrap();
    std::fs::write(root.join("place-history.redb"), b"retained").unwrap();
    let retained = crate::place::PlaceLeftSummary {
        graph_nodes: 3,
        chat_messages: 5,
        members: 2,
    };

    assert_eq!(load_place_left(&root).unwrap(), None);
    mark_place_left(&root, &retained).unwrap();
    assert_eq!(load_place_left(&root).unwrap(), Some(retained));
    // Unlike `remove_place_binding`, the binding and every retained
    // store stay exactly as they were: Rejoin needs both.
    assert_eq!(load_place_binding(&root).unwrap(), Some(binding));
    assert!(root.join("place-history.redb").exists());

    assert_eq!(clear_place_left(&root).unwrap(), true);
    assert_eq!(load_place_left(&root).unwrap(), None);
    assert_eq!(clear_place_left(&root).unwrap(), false);

    // A malformed marker must report failure rather than silently
    // reading as joined or as left.
    let marker_path = root.join(PLACE_LEFT_FILE);
    std::fs::write(&marker_path, b"not json").unwrap();
    assert!(load_place_left(&root).is_err());
    std::fs::write(
        &marker_path,
        br#"{"version":2,"graph_nodes":0,"chat_messages":0,"members":0}"#,
    )
    .unwrap();
    assert!(matches!(
        load_place_left(&root),
        Err(PlaceSidecarError::UnsupportedVersion(2))
    ));
}

#[test]
fn unsupported_place_binding_stays_a_visible_error() {
    let root = temp_root("place-version");
    let binding = PlaceBindingV1::new(
        crate::place::PlaceId([0x11; 32]),
        crate::place::SharedContainerId([0x22; 32]),
        crate::place::ChatSpaceId([0x33; 32]),
        "commons",
    )
    .unwrap();
    let mut value = serde_json::to_value(binding).unwrap();
    value["version"] = serde_json::json!(99);
    std::fs::write(
        place_binding_path(&root),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();

    let error = load_place_binding(&root).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unsupported place binding version 99")
    );
    assert!(
        place_binding_path(&root).exists(),
        "a failed load does not erase the evidence"
    );
}

#[test]
fn legacy_node_columns_migrate_on_load_and_existing_facets_win() {
    let root = temp_root("node-facet-migration");
    let mut canvas = mere::canvas::Canvas::new();
    let key = canvas.visit("https://legacy.example/");
    let node_id = canvas.graph().get_node(key).unwrap().id;
    save_session_graph(&root, canvas.graph());

    let graph_file = root.join(session_graph_store::GRAPH_FILE);
    let mut snapshot = crate::session_graph_file::load_snapshot(&graph_file)
        .unwrap()
        .unwrap();
    snapshot.nodes[0].is_pinned = true;
    std::fs::write(
        &graph_file,
        serde_json::to_string_pretty(&snapshot).unwrap(),
    )
    .unwrap();

    let mut facets = pandect::NodeFacetStore::new();
    facets
        .set(
            node_id,
            chartulary::FacetId::new(mere::kernel::graph::node_facets::ARRANGEMENT_PIN),
            serde_json::json!(false),
            &chartulary::AcceptAll,
        )
        .unwrap();
    save_node_facets(&root, &facets);

    let restored = load_session_graph(&root).expect("legacy graph");
    let restored_key = restored.get_node_key_by_id(node_id).unwrap();
    assert_eq!(
        restored.node_is_pinned(restored_key),
        Some(false),
        "the canonical sidecar overlays the imported legacy value"
    );

    let canonical = crate::session_graph_file::load_snapshot(&graph_file)
        .unwrap()
        .unwrap();
    assert!(!canonical.nodes[0].is_pinned);
    assert_eq!(canonical.legacy_node_facet_count(), 0);
    let persisted = load_node_facets(&root).unwrap();
    assert_eq!(
        persisted.get(
            &node_id,
            &chartulary::FacetId::new(mere::kernel::graph::node_facets::ARRANGEMENT_PIN,),
        ),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn image_gc_keeps_live_blobs_and_drops_only_hash_named_orphans() {
    let root = temp_root("image-gc");
    let live = mere::kernel::types::ImageRef::new([1; 32], 1, 1);
    let orphan = mere::kernel::types::ImageRef::new([2; 32], 1, 1);
    save_image_blob(&root, &live.hex(), b"live");
    save_image_blob(&root, &orphan.hex(), b"orphan");
    std::fs::write(images_dir(&root).join("README"), b"not a blob").unwrap();

    let mut canvas = mere::canvas::Canvas::new();
    let key = canvas.visit("https://live.example/");
    let node = canvas.graph().get_node(key).unwrap().id;
    assert!(canvas.set_node_favicon_for(node, live));

    assert_eq!(gc_orphan_image_blobs(&root, canvas.graph()), 1);
    assert!(load_image_blob(&root, &live.hex()).is_some());
    assert!(load_image_blob(&root, &orphan.hex()).is_none());
    assert!(images_dir(&root).join("README").exists());
    assert_eq!(
        gc_orphan_image_blobs(&root, canvas.graph()),
        0,
        "re-running the sweep is a no-op"
    );
}
