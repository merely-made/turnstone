// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::trail_memory::sealed_memory_dir;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct TestSealer(AtomicBool);
impl PayloadSealer for TestSealer {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        if !self.0.load(Ordering::SeqCst) {
            return Err(eidetic::Error::new("locked"));
        }
        let mut ciphertext = hash.to_hex().into_bytes();
        ciphertext.extend(clear.iter().map(|b| b ^ 0x5a));
        Ok((
            ciphertext,
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([7; 16]),
                format: "fixture".into(),
            },
        ))
    }
    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        cipher: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if !self.0.load(Ordering::SeqCst)
            || marker.format != "fixture"
            || !cipher.starts_with(hash.to_hex().as_bytes())
        {
            return Err(eidetic::Error::new("refused"));
        }
        let clear: Vec<u8> = cipher[64..].iter().map(|b| b ^ 0x5a).collect();
        if eidetic::Hash::of(&clear) != *hash {
            return Err(eidetic::Error::new("tampered"));
        }
        Ok(clear)
    }
}

struct EpochFenceSealer {
    inner: TestSealer,
    epoch: std::sync::Arc<AtomicU64>,
    change_epoch_after_unseal: AtomicBool,
    unseal_calls: std::sync::Arc<AtomicU64>,
}
impl PayloadSealer for EpochFenceSealer {
    fn seal(
        &self,
        hash: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        self.inner.seal(hash, clear)
    }

    fn unseal(
        &self,
        hash: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        ciphertext: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        self.unseal_calls.fetch_add(1, Ordering::SeqCst);
        let clear = self.inner.unseal(hash, marker, ciphertext)?;
        if self.change_epoch_after_unseal.load(Ordering::SeqCst) {
            self.epoch.fetch_add(1, Ordering::SeqCst);
        }
        Ok(clear)
    }
}
#[test]
fn sealed_namespace_refuses_existing_unsealed_content_without_replacement() {
    let root = tempfile::tempdir().unwrap();
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let value = eidetic::models::OpaqueBlob(b"legacy-collision-canary".to_vec());
    let id = pollster::block_on(eidetic::save_typed(
        &mut store,
        &value,
        Vec::new(),
        eidetic::PrivacyClass::LocalOnly,
        eidetic::ProvenanceRecord {
            origin: eidetic::ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: None,
            generated_at: eidetic::Timestamp(0),
        },
        eidetic::TrustEnvelope {
            level: eidetic::TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: eidetic::ModerationState::Unreviewed,
        },
        eidetic::Timestamp(0),
    ))
    .unwrap();
    let key = format!("blob:{}", id.0.to_hex());
    let before = pollster::block_on(store.get(&key)).unwrap();
    let sealer = TestSealer(AtomicBool::new(true));
    assert!(save(&mut store, &sealer, &value, 1).is_err());
    assert_eq!(pollster::block_on(store.get(&key)).unwrap(), before);
}

#[test]
fn private_capture_reopens_and_keeps_bytes_and_receipt_sealed() {
    let root = tempfile::tempdir().unwrap();
    let sealer = TestSealer(AtomicBool::new(true));
    let text = "PRIVATE-CAPTURE-CANARY-λ-837";
    let body = format!("<html><main><p>{text}</p></main></html>");
    let document = FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        effective_url: Some("https://private.example/secret".into()),
        content_type: Some("text/html".into()),
        acquired_at_ms: 1234,
    };
    let request = CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session: crate::panes::SessionId::new(),
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://private.example/secret".into(),
        source: Some(SourceIdentity::of(&document)),
        keep: true,
    };
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    sealer.0.store(false, Ordering::SeqCst);
    assert!(capture_request(&mut store, &sealer, &request, Some(&document)).is_err());
    assert!(pollster::block_on(store.list("")).unwrap().is_empty());
    sealer.0.store(true, Ordering::SeqCst);
    let stored = capture_request(&mut store, &sealer, &request, Some(&document)).unwrap();
    let all = pollster::block_on(store.list("")).unwrap();
    for key in all {
        let bytes = pollster::block_on(store.get(&key)).unwrap().unwrap();
        assert!(!bytes.windows(text.len()).any(|w| w == text.as_bytes()));
        assert!(
            !bytes
                .windows("https://private.example/secret".len())
                .any(|w| w == b"https://private.example/secret")
        );
    }
    acknowledge(&mut store, &sealer, &request).unwrap();
    drop(store);
    let mut reopened = muniment::FjallStore::open(root.path()).unwrap();
    assert_eq!(
        capture_request(&mut reopened, &sealer, &request, None).unwrap(),
        stored
    );
    let mut changed = request.clone();
    changed.url = "https://other.example/".into();
    assert!(capture_request(&mut reopened, &sealer, &changed, None).is_err());
    let recovered = receipts(&mut reopened, &sealer).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].saved);
    let admission = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            session: request.session,
            graph: request.graph,
            node: request.node,
            annotation_manifest: stored.annotation_manifest.clone(),
        },
        occurrence: 0,
        title: "Private fixture".into(),
        url: request.url.clone(),
        error: None,
    };
    assert!(
        content(&mut reopened, &sealer, &admission)
            .unwrap()
            .text
            .contains(text)
    );
    sealer.0.store(false, Ordering::SeqCst);
    assert!(capture_request(&mut reopened, &sealer, &request, None).is_err());
    assert!(receipts(&mut reopened, &sealer).is_err());
    assert!(content(&mut reopened, &sealer, &admission).is_err());
    drop(reopened);
    let mut paths = vec![root.path().to_path_buf()];
    while let Some(path) = paths.pop() {
        if path.is_dir() {
            paths.extend(
                std::fs::read_dir(path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes.windows(text.len()).any(|w| w == text.as_bytes()));
            assert!(
                !bytes
                    .windows("https://private.example/secret".len())
                    .any(|w| w == b"https://private.example/secret")
            );
        }
    }
}

#[test]
fn actor_drops_private_read_reply_when_custody_epoch_changes_after_unseal() {
    use crate::trail_memory::{SourceCustody, TrailCommand, spawn_trail_for_session};
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join("sessions")
        .join(session.as_uuid().to_string())
        .join("memory");
    std::fs::create_dir_all(&directory).unwrap();
    let body = "<html><main><p>late custody reply private canary λ</p></main></html>";
    let document = FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body: body.into(),
        effective_url: Some("https://private.example/late".into()),
        content_type: Some("text/html".into()),
        acquired_at_ms: 1_700_000_123_456,
    };
    let request = CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session,
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://private.example/late".into(),
        source: Some(SourceIdentity::of(&document)),
        keep: true,
    };
    let epoch = std::sync::Arc::new(AtomicU64::new(9));
    let sealer = std::sync::Arc::new(EpochFenceSealer {
        inner: TestSealer(AtomicBool::new(true)),
        epoch: epoch.clone(),
        change_epoch_after_unseal: AtomicBool::new(false),
        unseal_calls: std::sync::Arc::new(AtomicU64::new(0)),
    });
    let stored = {
        let mut sealed = muniment::FjallStore::open(sealed_memory_dir(&directory)).unwrap();
        let stored =
            capture_request(&mut sealed, sealer.as_ref(), &request, Some(&document)).unwrap();
        acknowledge(&mut sealed, sealer.as_ref(), &request).unwrap();
        stored
    };
    let admission = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            session,
            graph: request.graph,
            node: request.node,
            annotation_manifest: stored.annotation_manifest,
        },
        occurrence: 0,
        title: "Private late reply fixture".into(),
        url: request.url,
        error: None,
    };
    let generation = uuid::Uuid::new_v4();
    let wake: crate::trail_memory::Wake = std::sync::Arc::new(|| {});
    let (actor, updates) = spawn_trail_for_session(
        wake,
        Some(directory.clone()),
        Default::default(),
        generation,
    );
    let custody_sealer: std::sync::Arc<dyn eidetic::PayloadSealer + Send + Sync> = sealer.clone();
    let custody_epoch = epoch.clone();
    assert!(actor.command(TrailCommand::SetCustody(Some(SourceCustody {
        sealer: custody_sealer,
        epoch: std::sync::Arc::new(move || Some(custody_epoch.load(Ordering::SeqCst))),
    }))));
    assert!(matches!(
        updates.recv_timeout(Duration::from_secs(5)).unwrap(),
        crate::action::Update::SourceCaptureRevalidated { .. }
    ));

    assert!(actor.command(TrailCommand::LoadSavedSources {
        generation,
        request: uuid::Uuid::new_v4(),
        session,
        admissions: vec![admission.clone()],
        directory: directory.clone(),
    }));
    let loaded = updates.recv_timeout(Duration::from_secs(5)).unwrap();
    let crate::action::Update::SavedSourcesLoaded { result, .. } = loaded else {
        panic!("expected private source loading update")
    };
    let content = result.unwrap().remove(0).content.unwrap();
    let target = crate::saved_sources::Target {
        key: admission.key.clone(),
        text_hash: content.text_hash,
        selector: None,
    };
    sealer
        .change_epoch_after_unseal
        .store(true, Ordering::SeqCst);
    assert!(actor.command(TrailCommand::ReadSavedSource {
        generation,
        request: uuid::Uuid::new_v4(),
        node: admission.key.node,
        url: crate::saved_sources::address(&target).unwrap(),
        target: target.clone(),
        admission,
        directory,
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        crate::action::Update::SavedSourceRead { result, .. } => {
            assert!(result.unwrap_err().contains("custody changed"));
        },
        _ => panic!("expected private source read update"),
    }
    assert!(epoch.load(Ordering::SeqCst) > 9);
    let (release, receive) = std::sync::mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::Release(release)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    actor.join().unwrap();
}

#[test]
fn recovery_does_not_unseal_after_custody_closes_before_recovery() {
    use crate::trail_memory::{SourceCustody, TrailCommand, spawn_trail_for_session};
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join("sessions")
        .join(session.as_uuid().to_string())
        .join("memory");
    std::fs::create_dir_all(&directory).unwrap();
    let body = "<html><main><p>recovery custody canary</p></main></html>";
    let document = FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body: body.into(),
        effective_url: Some("https://private.example/recovery".into()),
        content_type: Some("text/html".into()),
        acquired_at_ms: 1_700_000_123_456,
    };
    let request = CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session,
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://private.example/recovery".into(),
        source: Some(SourceIdentity::of(&document)),
        keep: true,
    };
    let epoch = std::sync::Arc::new(AtomicU64::new(9));
    let unseal_calls = std::sync::Arc::new(AtomicU64::new(0));
    let sealer = std::sync::Arc::new(EpochFenceSealer {
        inner: TestSealer(AtomicBool::new(true)),
        epoch,
        change_epoch_after_unseal: AtomicBool::new(false),
        unseal_calls: unseal_calls.clone(),
    });
    {
        let mut sealed = muniment::FjallStore::open(sealed_memory_dir(&directory)).unwrap();
        capture_request(&mut sealed, sealer.as_ref(), &request, Some(&document)).unwrap();
        acknowledge(&mut sealed, sealer.as_ref(), &request).unwrap();
    }
    let before = unseal_calls.load(Ordering::SeqCst);
    let generation = uuid::Uuid::new_v4();
    let (actor, _) = spawn_trail_for_session(
        std::sync::Arc::new(|| {}),
        Some(directory),
        Default::default(),
        generation,
    );
    let snapshots = std::sync::Arc::new(AtomicU64::new(0));
    let callback_snapshots = snapshots.clone();
    let custody_sealer: std::sync::Arc<dyn eidetic::PayloadSealer + Send + Sync> = sealer;
    assert!(actor.command(TrailCommand::SetCustody(Some(SourceCustody {
        sealer: custody_sealer,
        epoch: std::sync::Arc::new(move || {
            match callback_snapshots.fetch_add(1, Ordering::SeqCst) {
                // SetCustody admission and legacy receipt recovery see the
                // live binding. It closes immediately before sealed read.
                0 | 1 => Some(9),
                _ => None,
            }
        }),
    }))));
    let (ready, receive) = std::sync::mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::CustodyReady(ready)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(snapshots.load(Ordering::SeqCst) >= 3);
    assert_eq!(
        unseal_calls.load(Ordering::SeqCst),
        before,
        "closed custody must be checked before reading or unsealing recovery records"
    );
    let (release, receive) = std::sync::mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::Release(release)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    actor.join().unwrap();
}
