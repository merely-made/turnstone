// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn same_directory_actor_reopen_rejects_old_opening_commands_before_writes() {
    use super::super::source_capture_receipts::recover_receipts;
    let root = tempfile::tempdir().unwrap();
    let document = document("current-actor-opening");
    let request = request(&document);
    let directory = root
        .path()
        .join("sessions")
        .join(request.session.as_uuid().to_string())
        .join("memory");
    let initial_generation = uuid::Uuid::new_v4();
    let current_generation = uuid::Uuid::new_v4();
    let (handle, updates) = spawn_trail_for_session(
        std::sync::Arc::new(|| {}),
        Some(directory.clone()),
        Default::default(),
        initial_generation,
    );
    assert!(handle.command(TrailCommand::Reopen {
        directory: directory.clone(),
        capture_generation: current_generation,
    }));
    assert!(handle.command(TrailCommand::SetCustody(Some(actor_fixture_custody()))));

    let stale_document = super::document("stale-opening-must-not-deposit");
    let stale_request = crate::source_capture::CaptureRequest {
        id: uuid::Uuid::new_v4(),
        node: uuid::Uuid::new_v4(),
        source: Some(crate::source_capture::SourceIdentity::of(&stale_document)),
        url: stale_document.effective_url.clone().unwrap(),
        ..request.clone()
    };
    assert!(handle.command(TrailCommand::CaptureSourceDocument {
        capture_generation: initial_generation,
        request: stale_request.clone(),
        directory: directory.clone(),
        document: Some(stale_document.clone()),
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceDocumentCaptured {
            capture_generation,
            request: refused,
            result,
        } => {
            assert_eq!(capture_generation, initial_generation);
            assert_eq!(refused, stale_request);
            assert!(result.unwrap_err().contains("another session opening"));
        },
        _ => panic!("expected stale-opening capture refusal"),
    }
    assert!(handle.command(TrailCommand::AcknowledgeSourceCapture {
        capture_generation: initial_generation,
        request: stale_request.clone(),
        directory: directory.clone(),
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceCaptureSaved {
            capture_generation,
            request: refused,
            result,
        } => {
            assert_eq!(capture_generation, initial_generation);
            assert_eq!(refused, stale_request.id);
            assert!(result.unwrap_err().contains("another session opening"));
        },
        _ => panic!("expected stale-opening save refusal"),
    }

    let foreign_document = super::document("foreign-session-must-not-deposit");
    let foreign_request = crate::source_capture::CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session: crate::panes::SessionId::new(),
        source: Some(crate::source_capture::SourceIdentity::of(&foreign_document)),
        url: foreign_document.effective_url.clone().unwrap(),
        ..request.clone()
    };
    assert!(handle.command(TrailCommand::CaptureSourceDocument {
        capture_generation: current_generation,
        request: foreign_request.clone(),
        directory: directory.clone(),
        document: Some(foreign_document.clone()),
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceDocumentCaptured {
            capture_generation,
            request: refused,
            result,
        } => {
            assert_eq!(capture_generation, current_generation);
            assert_eq!(refused, foreign_request);
            assert!(result.unwrap_err().contains("another session opening"));
        },
        _ => panic!("expected foreign-session capture refusal"),
    }
    assert!(handle.command(TrailCommand::AcknowledgeSourceCapture {
        capture_generation: current_generation,
        request: foreign_request.clone(),
        directory: directory.clone(),
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceCaptureSaved {
            capture_generation,
            request: refused,
            result,
        } => {
            assert_eq!(capture_generation, current_generation);
            assert_eq!(refused, foreign_request.id);
            assert!(result.unwrap_err().contains("another session opening"));
        },
        _ => panic!("expected foreign-session acknowledgement refusal"),
    }

    // Positive control immediately after same-directory Reopen: this cannot
    // pass if the old Fjall owner prevented the new handle from opening.
    assert!(handle.command(TrailCommand::CaptureSourceDocument {
        capture_generation: current_generation,
        request: request.clone(),
        directory: directory.clone(),
        document: Some(document.clone()),
    }));
    let stored = match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceDocumentCaptured {
            capture_generation,
            request: captured,
            result,
        } => {
            assert_eq!(capture_generation, current_generation);
            assert_eq!(captured, request);
            result.unwrap()
        },
        _ => panic!("expected current-opening capture success"),
    };
    assert!(handle.command(TrailCommand::AcknowledgeSourceCapture {
        capture_generation: initial_generation,
        request: request.clone(),
        directory: directory.clone(),
    }));
    match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
        Update::SourceCaptureSaved {
            capture_generation,
            request: refused,
            result,
        } => {
            assert_eq!(capture_generation, initial_generation);
            assert_eq!(refused, request.id);
            assert!(result.unwrap_err().contains("another session opening"));
        },
        _ => panic!("expected old-opening acknowledgement refusal"),
    }
    let (ack, receive) = std::sync::mpsc::sync_channel(1);
    assert!(handle.command(TrailCommand::Release(ack)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut inspect = FjallStore::open(&directory).unwrap();
    assert!(recover_receipts(&mut inspect).unwrap().is_empty());
    let stale_hash = eidetic::Hash::of(&stale_document.bytes);
    assert!(
        pollster::block_on(inspect.get(&format!("blob:{}", stale_hash.to_hex())))
            .unwrap()
            .is_none()
    );
    let foreign_hash = eidetic::Hash::of(&foreign_document.bytes);
    assert!(
        pollster::block_on(inspect.get(&format!("blob:{}", foreign_hash.to_hex())))
            .unwrap()
            .is_none()
    );
    drop(inspect);

    let fixture_custody = actor_fixture_custody();
    let mut inspect = FjallStore::open(super::super::sealed_memory_dir(&directory)).unwrap();
    let inventory =
        super::super::sealed_capture::receipts(&mut inspect, fixture_custody.sealer.as_ref())
            .unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].request, request);
    assert!(
        !inventory[0].saved,
        "wrong-generation ack cannot mark a valid receipt saved"
    );
    for hash in [stale_hash, foreign_hash] {
        assert!(
            pollster::block_on(inspect.get(&format!("blob:{}", hash.to_hex())))
                .unwrap()
                .is_none()
        );
    }
    super::super::sealed_capture::verify(
        &mut inspect,
        fixture_custody.sealer.as_ref(),
        &request,
        &stored,
    )
    .unwrap();
    drop(inspect);

    let final_generation = uuid::Uuid::new_v4();
    assert!(handle.command(TrailCommand::Reopen {
        directory: directory.clone(),
        capture_generation: final_generation,
    }));
    assert!(handle.command(TrailCommand::AcknowledgeSourceCapture {
        capture_generation: final_generation,
        request: request.clone(),
        directory: directory.clone(),
    }));
    loop {
        match updates.recv_timeout(Duration::from_secs(5)).unwrap() {
            Update::SourceCaptureRecovered {
                capture_generation,
                request: recovered,
                stored: recovered_stored,
            } => {
                assert_eq!(capture_generation, final_generation);
                assert_eq!(recovered, request);
                assert_eq!(recovered_stored, Some(stored.clone()));
            },
            Update::SourceCaptureSaved {
                capture_generation,
                request: completed,
                result,
            } => {
                assert_eq!(capture_generation, final_generation);
                assert_eq!(completed, request.id);
                result.unwrap();
                break;
            },
            _ => panic!("unexpected capture recovery update"),
        }
    }
    let (ack, receive) = std::sync::mpsc::sync_channel(1);
    assert!(handle.command(TrailCommand::Release(ack)));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(handle);
    let fixture_custody = actor_fixture_custody();
    let mut fresh = FjallStore::open(super::super::sealed_memory_dir(&directory)).unwrap();
    let inventory =
        super::super::sealed_capture::receipts(&mut fresh, fixture_custody.sealer.as_ref())
            .unwrap();
    assert_eq!(inventory.len(), 1);
    assert!(
        inventory[0].saved,
        "current-generation positive acknowledgement commits"
    );
    super::super::sealed_capture::verify(
        &mut fresh,
        fixture_custody.sealer.as_ref(),
        &request,
        &stored,
    )
    .unwrap();
}
