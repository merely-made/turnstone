use super::*;
use pandect::manifest::{CodicilId, GraphSessionManifest};
use pandect::{GraphId, SessionId};
use uuid::Uuid;

fn fixture_session(idx: u128) -> (SessionId, GraphId) {
    (
        SessionId::from_uuid(Uuid::from_u128(
            0x1000_0000_0000_0000_0000_0000_0000_0000 + idx,
        )),
        GraphId::from_uuid(Uuid::from_u128(
            0x2000_0000_0000_0000_0000_0000_0000_0000 + idx,
        )),
    )
}

fn temp_root(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let root = std::env::temp_dir().join(format!("host-runtime-test-{label}-{pid}-{nanos}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn insert_marks_dirty_and_iter_returns_inserted() {
    let mut store = ManifestStore::new();
    let (sid, gid) = fixture_session(1);
    store.insert(GraphSessionManifest::new(sid, gid));
    assert_eq!(store.len(), 1);
    assert!(store.is_dirty(sid));
    let collected: Vec<_> = store.iter().map(|(s, _)| s).collect();
    assert_eq!(collected, vec![sid]);
}

#[test]
fn update_runs_closure_and_marks_dirty() {
    let mut store = ManifestStore::new();
    let (sid, gid) = fixture_session(2);
    store.insert(GraphSessionManifest::new(sid, gid));
    // Insert marks dirty; flush would need a root. We just check
    // the update path independently of flushing.
    store.dirty.clear();
    assert!(!store.is_dirty(sid));
    let ok = store.update(sid, |m| {
        m.display_name = Some("research".to_string());
        m.extension_set("engine_profile", &"SessionScoped").unwrap();
    });
    assert!(ok);
    assert!(store.is_dirty(sid));
    let m = store.get(sid).unwrap();
    assert_eq!(m.display_name.as_deref(), Some("research"));
    assert_eq!(
        m.extension_get::<String>("engine_profile").as_deref(),
        Some("SessionScoped")
    );
}

#[test]
fn update_returns_false_for_missing_session() {
    let mut store = ManifestStore::new();
    let (sid, _) = fixture_session(3);
    assert!(!store.update(sid, |_| {}));
}

#[test]
fn flush_writes_dirty_manifests_to_disk() {
    let root = temp_root("flush");
    let mut store = ManifestStore::with_root(&root);
    let (sid, gid) = fixture_session(4);
    store.insert(GraphSessionManifest::new(sid, gid));
    let written = store.flush_dirty().unwrap();
    assert_eq!(written, 1);
    assert_eq!(store.dirty_count(), 0);
    let manifest_path = root.join(sid.as_uuid().to_string()).join(MANIFEST_FILE);
    assert!(manifest_path.exists());
    let json = fs::read_to_string(&manifest_path).unwrap();
    assert!(json.contains(&sid.as_uuid().to_string()));
    fs::remove_dir_all(&root).ok();
}

#[test]
fn load_from_disk_recovers_what_flush_wrote() {
    let root = temp_root("load");
    let (sid_a, gid_a) = fixture_session(5);
    let (sid_b, gid_b) = fixture_session(6);
    {
        let mut store = ManifestStore::with_root(&root);
        store.insert(GraphSessionManifest::new(sid_a, gid_a));
        store.insert(GraphSessionManifest::new(sid_b, gid_b));
        store.flush_dirty().unwrap();
    }
    let mut store = ManifestStore::new();
    let report = store.load_from_disk(&root).unwrap();
    assert_eq!(report.loaded.len(), 2);
    assert!(report.failed.is_empty());
    assert!(store.get(sid_a).is_some());
    assert!(store.get(sid_b).is_some());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn load_from_disk_records_malformed_manifests_without_panicking() {
    let root = temp_root("malformed");
    // Create a session dir with garbage in its manifest.
    let bad_dir = root.join("bad-session");
    fs::create_dir_all(&bad_dir).unwrap();
    fs::write(bad_dir.join(MANIFEST_FILE), "not json {{").unwrap();
    // And one well-formed for contrast.
    let (sid, gid) = fixture_session(7);
    let good_dir = root.join(sid.as_uuid().to_string());
    fs::create_dir_all(&good_dir).unwrap();
    let manifest = GraphSessionManifest::new(sid, gid);
    fs::write(
        good_dir.join(MANIFEST_FILE),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let mut store = ManifestStore::new();
    let report = store.load_from_disk(&root).unwrap();
    assert_eq!(report.loaded.len(), 1);
    assert_eq!(report.loaded[0], sid);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].dir_name, "bad-session");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn move_to_trash_relocates_directory_and_drops_memory_entry() {
    let root = temp_root("trash");
    let mut store = ManifestStore::with_root(&root);
    let (sid, gid) = fixture_session(8);
    store.insert(GraphSessionManifest::new(sid, gid));
    store.flush_dirty().unwrap();

    let trashed = store.move_to_trash(sid).unwrap();
    assert!(trashed);
    assert!(store.get(sid).is_none());
    let live_path = root.join(sid.as_uuid().to_string());
    assert!(!live_path.exists());
    let trash_path = root.join(TRASH_DIR).join(sid.as_uuid().to_string());
    assert!(trash_path.exists());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn trash_lists_and_restores_with_identity_intact() {
    let root = temp_root("trash-restore");
    let mut store = ManifestStore::with_root(&root);
    let (sid, gid) = fixture_session(10);
    let mut m = GraphSessionManifest::new(sid, gid);
    m.display_name = Some("expedition".to_string());
    store.insert(m);
    store.flush_dirty().unwrap();
    // A sibling session file inside the dir must survive the round trip
    // (the whole directory moves, sidecars and all).
    fs::write(
        root.join(sid.as_uuid().to_string()).join("facets.json"),
        "{}",
    )
    .unwrap();

    store.move_to_trash(sid).unwrap();
    let listed = store.list_trash();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].session_id, sid);
    assert_eq!(listed[0].display_name.as_deref(), Some("expedition"));

    assert!(store.restore_from_trash(sid).unwrap());
    assert!(store.get(sid).is_some(), "the manifest re-listed in memory");
    assert_eq!(
        store.get(sid).unwrap().root_graph_id,
        gid,
        "identity intact"
    );
    assert!(
        root.join(sid.as_uuid().to_string())
            .join("facets.json")
            .exists(),
        "the session's sidecars came back with it"
    );
    assert!(store.list_trash().is_empty(), "the trash entry is consumed");
    assert!(
        !store.restore_from_trash(sid).unwrap(),
        "a second restore finds nothing"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn restore_never_clobbers_a_live_session() {
    let root = temp_root("trash-clobber");
    let mut store = ManifestStore::with_root(&root);
    let (sid, gid) = fixture_session(11);
    store.insert(GraphSessionManifest::new(sid, gid));
    store.flush_dirty().unwrap();
    store.move_to_trash(sid).unwrap();
    // The same id comes alive again (a re-mint under a hand-copied id).
    store.insert(GraphSessionManifest::new(sid, gid));
    store.flush_dirty().unwrap();
    assert!(
        store.restore_from_trash(sid).is_err(),
        "restore must refuse to overwrite a live session directory"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn consolidation_receipts_round_trip_through_disk() {
    let root = temp_root("consolidate");
    let (sid, gid) = fixture_session(9);
    {
        let mut store = ManifestStore::with_root(&root);
        store.insert(GraphSessionManifest::new(sid, gid));
        store.update(sid, |m| {
            m.record_consolidation(CodicilId("codicil-receipt-xyz".to_string()));
        });
        store.flush_dirty().unwrap();
    }
    let mut store = ManifestStore::new();
    store.load_from_disk(&root).unwrap();
    let restored = store.get(sid).unwrap();
    assert_eq!(restored.consolidated_codicils.len(), 1);
    assert_eq!(
        restored.consolidated_codicils[0],
        CodicilId("codicil-receipt-xyz".to_string())
    );
    assert!(restored.last_consolidated_at.is_some());
    fs::remove_dir_all(&root).ok();
}

#[test]
fn flush_without_root_returns_error() {
    let mut store = ManifestStore::new();
    let (sid, gid) = fixture_session(10);
    store.insert(GraphSessionManifest::new(sid, gid));
    let err = store.flush_dirty().unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}
