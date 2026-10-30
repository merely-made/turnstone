// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn queued_native_write_shares_owner_with_an_inflight_read() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"read write fixture",
    ));
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let make_context = || CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/write".into(),
            revision: None,
        },
        capture_id: "write".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(1),
    };
    let make_export = |source: &str| {
        let selected = crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start: 0,
            end: source.len(),
        };
        crate::producer_retention::export_knot_selection(
            "mere://knot/write",
            source,
            &selected,
            100_000,
        )
        .unwrap()
    };
    let (first_export, first_backing) = make_export("first passage");
    let first_id = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &first_export,
            backing: &first_backing,
        },
        &make_context(),
        policy,
        &*sealer,
    )
    .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let paused = Arc::new(PausedSealer {
        inner: sealer.clone(),
        entered: entered_tx,
        release: Mutex::new(release_rx),
        paused: AtomicBool::new(false),
    });
    let worker = owner_worker::Worker::spawn(Arc::new(|| {}));
    let limits = crate::research_storage::StorageLimits::default();
    let first = worker
        .read(
            root.path().to_owned(),
            first_id,
            policy,
            paused,
            limits.clone(),
            limits.budget(),
            (&limits.clip_queue).into(),
        )
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let (export, backing) = make_export("new passage");
    let access = crate::research::ClipAccess {
        root: root.path().into(),
        sealer: sealer.clone(),
        epoch: Arc::new(|| Some(1)),
    };
    let write = worker
        .retain(
            root.path().to_owned(),
            owner_worker::OwnedNative::Knot { export, backing },
            make_context(),
            policy,
            access,
            1,
            limits.clone(),
        )
        .unwrap();
    release_tx.send(()).unwrap();
    assert!(
        matches!(first.recv_timeout(Duration::from_secs(10)).unwrap().into_inner().unwrap().body, ClipBody::Text(ref text) if text == "first passage")
    );
    let new_id = write
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    assert_ne!(new_id, first_id);
    let read = worker
        .read(
            root.path().to_owned(),
            new_id,
            policy,
            sealer,
            limits.clone(),
            limits.budget(),
            (&limits.clip_queue).into(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    assert!(matches!(read.body, ClipBody::Text(ref text) if text == "new passage"));
    worker.close();
}

fn process_sealer() -> Arc<pandect::WalletEpochSealer> {
    Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId(uuid::Uuid::from_u128(17)),
        pandect::KeyEpochId(uuid::Uuid::from_u128(19)),
        b"process reopen fixture",
    ))
}

#[test]
#[ignore = "child of acknowledged_write_reopens_after_owner_process_exit"]
fn acknowledged_write_process_child() {
    let root = std::path::PathBuf::from(
        std::env::var_os("TURNSTONE_TEST_CLIP_ROOT").expect("fixture root"),
    );
    assert_eq!(
        std::fs::read(root.join("fixture-marker")).unwrap(),
        b"clip-process-fixture"
    );
    assert!(!root.join("retained-clips").exists());
    let source = "process durable selection";
    let selected = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start: 0,
        end: source.len(),
    };
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/process",
        source,
        &selected,
        100_000,
    )
    .unwrap();
    let context = CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/process".into(),
            revision: None,
        },
        capture_id: "process".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(1),
    };
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let sealer = process_sealer();
    let access = crate::research::ClipAccess {
        root: root.clone(),
        sealer,
        epoch: Arc::new(|| Some(1)),
    };
    let limits = crate::research_storage::StorageLimits::default();
    let worker = owner_worker::Worker::spawn(Arc::new(|| {}));
    let id = worker
        .retain(
            root.clone(),
            owner_worker::OwnedNative::Knot { export, backing },
            context,
            policy,
            access,
            1,
            limits,
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    std::fs::write(root.join("ack.json"), serde_json::to_vec(&id).unwrap()).unwrap();
    // Bypass ordinary Rust destructors: only an acknowledged worker sync can
    // have completed before this child exits. OS/page-cache survival is not a power-loss test.
    std::process::exit(0);
}

#[test]
fn acknowledged_write_reopens_after_owner_process_exit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("fixture-marker"), b"clip-process-fixture").unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "retained_clip::owner_overlap_tests::owner_write_overlap_test::acknowledged_write_process_child", "--ignored", "--nocapture"])
        .env("TURNSTONE_TEST_CLIP_ROOT", root.path())
        .output().unwrap();
    assert!(
        child.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    let id: ManifestId =
        serde_json::from_slice(&std::fs::read(root.path().join("ack.json")).unwrap()).unwrap();
    let limits = crate::research_storage::StorageLimits::default();
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let worker = owner_worker::Worker::spawn(Arc::new(|| {}));
    let clip = worker
        .read(
            root.path().to_owned(),
            id,
            policy,
            process_sealer(),
            limits.clone(),
            limits.budget(),
            (&limits.clip_queue).into(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    assert!(matches!(clip.body, ClipBody::Text(ref text) if text == "process durable selection"));
    worker.close();
}
