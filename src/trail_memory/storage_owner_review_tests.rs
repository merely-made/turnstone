// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use eidetic::Backend as _;
use std::sync::{Arc, mpsc};
use std::time::Duration;

#[test]
fn model_free_actor_recovery_limit_is_typed_preserves_bytes_and_retries() {
    fn files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files(root, &path, out);
            } else if path.is_file() {
                out.push((
                    path.strip_prefix(root).unwrap().to_owned(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut result = Vec::new();
        files(root, root, &mut result);
        result.sort();
        result
    }
    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join(session.as_uuid().to_string())
        .join("memory");
    let store = FjallStore::open(&directory).unwrap();
    pollster::block_on(store.put("retained", &vec![42; 8192])).unwrap();
    store.sync_all().unwrap();
    drop(store);
    let before = snapshot(&directory);
    let mut limits = crate::research_storage::StorageLimits::default();
    limits.recovery.max_value_bytes = 16;
    let generation = uuid::Uuid::new_v4();
    let (actor, updates) = spawn_trail_for_session_with_limits(
        Arc::new(|| {}),
        Some(directory.clone()),
        Default::default(),
        generation,
        Ok(limits),
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(
        custody_test_fixture::custody()
    ))));
    let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        update,
        Update::SourceCaptureRecoveryFailed {
            error: crate::research_storage::SourceReadError::StorageOpen {
                owner: "Session capture",
                error: muniment::StoreError::LimitExceeded {
                    kind: muniment::StoreLimitKind::ValueBytes,
                    limit: 16
                }
            },
            ..
        }
    ));
    let check = |generation| {
        let (reply, receive) = mpsc::sync_channel(1);
        assert!(actor.command(TrailCommand::ResearchSnapshot {
            context: crate::research::Context {
                session,
                generation,
                admissions: vec![],
                clips: vec![]
            },
            selected: None,
            max_chars: Some(1000),
            max_metadata_chars: Some(1000),
            budget: None,
            reply,
        }));
        assert!(matches!(
            receive.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err(crate::research_storage::SourceReadError::StorageOpen {
                owner: "Session capture",
                error: muniment::StoreError::LimitExceeded {
                    kind: muniment::StoreLimitKind::ValueBytes,
                    limit: 16
                }
            })
        ));
    };
    check(generation);
    let new_generation = uuid::Uuid::new_v4();
    assert!(actor.command(TrailCommand::Reopen {
        directory: directory.clone(),
        capture_generation: new_generation
    }));
    check(new_generation);
    let (release, receive) = mpsc::sync_channel(1);
    assert!(actor.command(TrailCommand::Release(release)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    actor.join().unwrap();
    assert_eq!(snapshot(&directory), before);
    let retry_generation = uuid::Uuid::new_v4();
    let (retry, _) = spawn_trail_for_session_with_limits(
        Arc::new(|| {}),
        Some(directory),
        Default::default(),
        retry_generation,
        Ok(crate::research_storage::StorageLimits::default()),
    );
    assert!(retry.command(TrailCommand::SetCustody(Some(
        custody_test_fixture::custody()
    ))));
    let (reply, receive) = mpsc::sync_channel(1);
    assert!(retry.command(TrailCommand::ResearchSnapshot {
        context: crate::research::Context {
            session,
            generation: retry_generation,
            admissions: vec![],
            clips: vec![]
        },
        selected: None,
        max_chars: Some(1000),
        max_metadata_chars: Some(1000),
        budget: None,
        reply,
    }));
    assert!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_ok()
    );
    let (release, receive) = mpsc::sync_channel(1);
    assert!(retry.command(TrailCommand::Release(release)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    retry.join().unwrap();
}

#[test]
fn model_free_capture_and_ack_actor_replies_preserve_real_owner_open_limits() {
    for sealed in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let session = crate::panes::SessionId::new();
        let directory = root
            .path()
            .join(session.as_uuid().to_string())
            .join("memory");
        std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
        let seeded = if sealed {
            sealed_memory_dir(&directory)
        } else {
            directory.clone()
        };
        let store = FjallStore::open(&seeded).unwrap();
        pollster::block_on(store.put("retained", &vec![42; 65536])).unwrap();
        store.sync_all().unwrap();
        drop(store);
        let mut limits = crate::research_storage::StorageLimits::default();
        limits.recovery.max_value_bytes = 4096;
        let generation = uuid::Uuid::new_v4();
        let (actor, updates) = spawn_trail_for_session_with_limits(
            Arc::new(|| {}),
            Some(directory.clone()),
            Default::default(),
            generation,
            Ok(limits),
        );
        assert!(actor.command(TrailCommand::SetCustody(Some(
            custody_test_fixture::custody()
        ))));
        let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
        let expected_owner = if sealed {
            "Sealed session"
        } else {
            "Session capture"
        };
        assert!(matches!(update, Update::SourceCaptureRecoveryFailed {
            error: crate::research_storage::SourceReadError::StorageOpen {
                owner, error: muniment::StoreError::LimitExceeded {
                    kind: muniment::StoreLimitKind::ValueBytes, limit: 4096
                },
            }, ..
        } if owner == expected_owner));
        let request = crate::source_capture::CaptureRequest {
            id: uuid::Uuid::new_v4(),
            session,
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            url: "https://example.test".into(),
            source: None,
            keep: true,
        };
        assert!(actor.command(TrailCommand::CaptureSourceDocument {
            capture_generation: generation,
            request: request.clone(),
            directory: directory.clone(),
            document: None,
        }));
        let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(update, Update::SourceDocumentCaptured {
            result: Err(crate::research_storage::SourceReadError::StorageOpen {
                owner, error: muniment::StoreError::LimitExceeded {
                    kind: muniment::StoreLimitKind::ValueBytes, limit: 4096
                },
            }), ..
        } if owner == expected_owner));
        assert!(actor.command(TrailCommand::AcknowledgeSourceCapture {
            capture_generation: generation,
            request,
            directory,
        }));
        let update = updates.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(update, Update::SourceCaptureSaved {
            result: Err(crate::research_storage::SourceReadError::StorageOpen {
                owner, error: muniment::StoreError::LimitExceeded {
                    kind: muniment::StoreLimitKind::ValueBytes, limit: 4096
                },
            }), ..
        } if owner == expected_owner));
        let (release, receive) = mpsc::sync_channel(1);
        assert!(actor.command(TrailCommand::Release(release)));
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        actor.join().unwrap();
    }
}
