// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::PayloadSealer;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

struct PausedSealer {
    inner: Arc<pandect::WalletEpochSealer>,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    paused: AtomicBool,
}
impl PayloadSealer for PausedSealer {
    fn seal(
        &self,
        hash: &Hash,
        bytes: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.inner.seal(hash, bytes)
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        bytes: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if !self.paused.swap(true, Ordering::SeqCst) {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
        self.inner.unseal(hash, marker, bytes)
    }
}

#[test]
fn overlapping_sealed_clip_reads_complete_without_a_second_owner_open() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"overlap fixture",
    ));
    let source = "visible selected passage";
    let selection = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start: 0,
        end: source.len(),
    };
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/overlap",
        source,
        &selection,
        100_000,
    )
    .unwrap();
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let context = CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/overlap".into(),
            revision: None,
        },
        capture_id: "overlap".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(1),
    };
    let id = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &context,
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
    let queue = owner_worker::QueueLimits {
        jobs: 4,
        bytes: 8_000_000,
    };
    let first = worker
        .read(
            root.path().to_owned(),
            id,
            policy,
            paused,
            limits.clone(),
            limits.budget(),
            queue,
        )
        .unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first read reached unseal while holding the owner");
    let second = worker
        .read(
            root.path().to_owned(),
            id,
            policy,
            sealer,
            limits.clone(),
            limits.budget(),
            queue,
        )
        .unwrap();
    release_tx.send(()).unwrap();
    let first = first
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    let second = second
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    worker.close();
    assert!(matches!(first.body, ClipBody::Text(ref text) if text == "visible selected passage"));
    assert!(matches!(second.body, ClipBody::Text(ref text) if text == "visible selected passage"));
}

struct CountingSealer {
    inner: Arc<pandect::WalletEpochSealer>,
    unseals: std::sync::atomic::AtomicUsize,
}
impl PayloadSealer for CountingSealer {
    fn seal(
        &self,
        hash: &Hash,
        bytes: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.inner.seal(hash, bytes)
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        bytes: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        self.unseals.fetch_add(1, Ordering::SeqCst);
        self.inner.unseal(hash, marker, bytes)
    }
}

#[test]
fn oversized_record_is_refused_before_unseal_and_json_decode() {
    let root = tempfile::tempdir().unwrap();
    let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"size fixture",
    ));
    let source = "selected text";
    let selection = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start: 0,
        end: source.len(),
    };
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/size",
        source,
        &selection,
        100_000,
    )
    .unwrap();
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let context = CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/size".into(),
            revision: None,
        },
        capture_id: "size".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(1),
    };
    let id = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &context,
        policy,
        &*sealer,
    )
    .unwrap();
    let counting = Arc::new(CountingSealer {
        inner: sealer,
        unseals: std::sync::atomic::AtomicUsize::new(0),
    });
    let worker = owner_worker::Worker::spawn(Arc::new(|| {}));
    let limits = crate::research_storage::StorageLimits::default();
    let tiny = ClipLimits {
        max_record_bytes: 1,
        ..policy
    };
    let rx = worker
        .read(
            root.path().to_owned(),
            id,
            tiny,
            counting.clone(),
            limits.clone(),
            limits.budget(),
            (&limits.clip_queue).into(),
        )
        .unwrap();
    let result = rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    assert!(matches!(result, Err(error) if error.to_string().contains("record exceeds")));
    assert_eq!(counting.unseals.load(Ordering::SeqCst), 0);
    worker.close();
}

struct BindingFixture(Arc<pandect::WalletEpochSealer>);
impl PayloadSealer for BindingFixture {
    fn seal(
        &self,
        hash: &Hash,
        bytes: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.0.seal(hash, bytes)
    }
    fn unseal(
        &self,
        hash: &Hash,
        marker: &eidetic::SealedBlobRef,
        bytes: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        self.0.unseal(hash, marker, bytes)
    }
}
impl crate::custody_binding::BindingAuthority for BindingFixture {
    fn custody_epoch(&self) -> Option<u64> {
        Some(1)
    }
    fn check_bound_status(&self) -> bool {
        true
    }
}

#[test]
fn export_claim_fences_retirement_and_publishes_only_the_bound_destination() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"claim fixture",
    ));
    let source = "chosen body";
    let selection = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start: 0,
        end: source.len(),
    };
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/claim",
        source,
        &selection,
        100_000,
    )
    .unwrap();
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let context = CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/claim".into(),
            revision: None,
        },
        capture_id: "claim".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(1),
    };
    let id = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &context,
        policy,
        &*sealer,
    )
    .unwrap();
    let host = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let authority = Arc::new(BindingFixture(sealer));
    let bound = crate::custody_binding::BoundSealer::bind(authority.clone(), host.clone()).unwrap();
    let (wake_tx, wake_rx) = mpsc::channel();
    let worker = owner_worker::Worker::spawn(Arc::new(move || {
        let _ = wake_tx.send(());
    }));
    let limits = crate::research_storage::StorageLimits::default();
    let path = output.path().join("selected.mereclip");
    let mut too_small = limits.clone();
    too_small.clip_queue.max_bytes = crate::retained_clip::export::wire_limit(policy).unwrap()
        + root.path().as_os_str().len()
        + path.as_os_str().len();
    assert!(
        worker
            .prepare_export(
                root.path().to_owned(),
                path.clone(),
                false,
                id,
                policy,
                bound.clone(),
                1,
                too_small
            )
            .is_err()
    );
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());
    let pending = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            false,
            id,
            policy,
            bound.clone(),
            1,
            limits.clone(),
        )
        .unwrap();
    let prepared = pending.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(prepared.value().is_ok());
    let prepared_data = prepared.value().as_ref().unwrap();
    assert!(prepared_data.matches(&bound, 1, id, root.path(), &path, false));
    assert!(!prepared_data.matches(&bound, 1, id, output.path(), &path, false));
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());
    bound.retire();
    assert!(bound.claim().is_none());
    drop(prepared);
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());

    let fresh = crate::custody_binding::BoundSealer::bind(authority.clone(), host.clone()).unwrap();
    let epoch = fresh.epoch().unwrap();
    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            false,
            id,
            policy,
            fresh.clone(),
            epoch,
            limits.clone(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let other_host = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let other = crate::custody_binding::BoundSealer::bind(authority.clone(), other_host).unwrap();
    assert!(
        worker
            .commit_export(other.claim().unwrap(), prepared)
            .is_err()
    );
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());

    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            false,
            id,
            policy,
            fresh.clone(),
            epoch,
            limits.clone(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let other_worker = owner_worker::Worker::spawn(Arc::new(|| {}));
    assert!(
        other_worker
            .commit_export(fresh.claim().unwrap(), prepared)
            .is_err()
    );
    other_worker.close();
    assert!(std::fs::read_dir(output.path()).unwrap().next().is_none());

    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            false,
            id,
            policy,
            fresh.clone(),
            epoch,
            limits.clone(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let claim = fresh.claim().unwrap();
    fresh.retire();
    let committed = worker.commit_export(claim, prepared).unwrap();
    committed
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    let package: ClipPackageV1 = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(package.payloads[0].bytes, b"chosen body");
    let original = std::fs::read(&path).unwrap();
    let latest = crate::custody_binding::BoundSealer::bind(authority, host).unwrap();
    let epoch = latest.epoch().unwrap();
    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            false,
            id,
            policy,
            latest.clone(),
            epoch,
            limits.clone(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let result = worker
        .commit_export(latest.claim().unwrap(), prepared)
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::write(&path, b"old file").unwrap();
    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            path.clone(),
            true,
            id,
            policy,
            latest.clone(),
            epoch,
            limits.clone(),
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    worker
        .commit_export(latest.claim().unwrap(), prepared)
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner()
        .unwrap();
    let overwritten: ClipPackageV1 =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(overwritten.payloads[0].bytes, b"chosen body");

    let second_path = output.path().join("claimed-before-shutdown.mereclip");
    let prepared = worker
        .prepare_export(
            root.path().to_owned(),
            second_path.clone(),
            false,
            id,
            policy,
            latest.clone(),
            epoch,
            limits,
        )
        .unwrap()
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let claim = latest.claim().unwrap();
    while wake_rx.try_recv().is_ok() {}
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let hold = worker
        .submit(
            owner_worker::QueueLimits {
                jobs: 4,
                bytes: 512 * 1024 * 1024,
            },
            1,
            move |_, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            },
        )
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let committed = worker.commit_export(claim, prepared).unwrap();
    drop(committed);
    worker.close();
    release_tx.send(()).unwrap();
    hold.recv_timeout(Duration::from_secs(10))
        .unwrap()
        .into_inner();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !second_path.exists() {
        wake_rx
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .unwrap();
    }
    let package: ClipPackageV1 =
        serde_json::from_slice(&std::fs::read(second_path).unwrap()).unwrap();
    assert_eq!(package.payloads[0].bytes, b"chosen body");
}

#[path = "owner_write_overlap_test.rs"]
mod owner_write_overlap_test;
