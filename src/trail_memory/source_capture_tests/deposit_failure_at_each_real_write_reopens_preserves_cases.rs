// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn deposit_failure_at_each_real_write_reopens_preserves_and_retries() {
    let baseline = tempfile::tempdir().unwrap();
    let mut baseline_store = FjallStore::open(baseline.path()).unwrap();
    capture_source_document(&mut baseline_store, &document("previous")).unwrap();
    let mut counted = FaultStore::new(baseline_store, None, false);
    let expected = capture_source_document_in(&mut counted, &document("candidate")).unwrap();
    let write_count = counted.writes.load(Ordering::SeqCst);
    assert!(
        write_count >= 5,
        "raw/annotation blobs, manifests and acquisition association"
    );
    drop(counted);

    for (fail_at, after_write) in
        (1..=write_count).flat_map(|write| [(write, false), (write, true)])
    {
        let root = tempfile::tempdir().unwrap();
        let mut seed = FjallStore::open(root.path()).unwrap();
        let previous = capture_source_document(&mut seed, &document("previous")).unwrap();
        let mut failing = FaultStore::new(seed, Some(fail_at), after_write);
        let error = capture_source_document_in(&mut failing, &document("candidate")).unwrap_err();
        assert!(
            error.contains("injected deposit write"),
            "write {fail_at}: {error}"
        );
        drop(failing);

        let mut reopened = FjallStore::open(root.path()).unwrap();
        verify_source_document(&mut reopened, &previous).unwrap();
        let retried = capture_source_document(&mut reopened, &document("candidate")).unwrap();
        assert_eq!(retried, expected, "retry identity after write {fail_at}");
        assert_eq!(
            capture_source_document(&mut reopened, &document("candidate")).unwrap(),
            expected
        );
        verify_source_document(&mut reopened, &previous).unwrap();
        drop(reopened);

        let mut fresh = FjallStore::open(root.path()).unwrap();
        verify_source_document(&mut fresh, &retried).unwrap();
        verify_source_document(&mut fresh, &previous).unwrap();
        let annotations = pollster::block_on(eidetic::typed::list_typed::<
            fleece::FleeceAnnotationRecord,
        >(&mut fresh))
        .unwrap();
        assert_eq!(
            annotations.len(),
            2,
            "retry does not invent an artifact after write {fail_at}"
        );
    }
}

#[test]
fn local_integrity_rejects_wrong_bytes_and_substituted_manifest_identity() {
    for annotation in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(root.path()).unwrap();
        let saved = capture_source_document(&mut store, &document("original")).unwrap();
        let target = id(if annotation {
            &saved.annotation_manifest
        } else {
            &saved.raw_manifest
        });
        let key = format!("blob:{}", target.0.to_hex());
        pollster::block_on(store.put(&key, b"wrong bytes under the correct address")).unwrap();
        assert!(
            verify_source_document(&mut store, &saved)
                .unwrap_err()
                .contains("payload integrity")
        );
        drop(store);
        let mut reopened = FjallStore::open(root.path()).unwrap();
        assert!(
            verify_source_document(&mut reopened, &saved).is_err(),
            "reopening cannot conceal corrupt bytes"
        );
    }

    for annotation in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(root.path()).unwrap();
        let saved = capture_source_document(&mut store, &document("original")).unwrap();
        let replacement = capture_source_document(&mut store, &document("replacement")).unwrap();
        let (requested, substitute) = if annotation {
            (
                id(&saved.annotation_manifest),
                id(&replacement.annotation_manifest),
            )
        } else {
            (id(&saved.raw_manifest), id(&replacement.raw_manifest))
        };
        let substitute_bytes = pollster::block_on(store.get(&substitute.store_key()))
            .unwrap()
            .unwrap();
        pollster::block_on(store.put(&requested.store_key(), &substitute_bytes)).unwrap();
        assert!(
            verify_source_document(&mut store, &saved)
                .unwrap_err()
                .contains("manifest identity mismatch")
        );
    }
}

#[test]
fn local_admission_rejects_inconsistent_pair_missing_raw_and_provenance() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let saved = capture_source_document(&mut store, &document("original")).unwrap();
    let other = capture_source_document(&mut store, &document("other")).unwrap();
    let mismatched = StoredSourceDocument {
        raw_manifest: other.raw_manifest,
        ..saved.clone()
    };
    assert!(
        verify_source_document(&mut store, &mismatched)
            .unwrap_err()
            .contains("provenance disagree")
    );

    let raw_id = id(&saved.raw_manifest);
    let mut manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut store, raw_id))
        .unwrap()
        .unwrap();
    manifest.provenance.origin = eidetic::ProvenanceOrigin::Derived;
    pollster::block_on(eidetic::manifest::save_manifest(&mut store, &manifest)).unwrap();
    assert!(
        verify_source_document(&mut store, &saved)
            .unwrap_err()
            .contains("provenance disagree")
    );
    pollster::block_on(store.delete(&raw_id.store_key())).unwrap();
    assert!(
        verify_source_document(&mut store, &saved)
            .unwrap_err()
            .contains("is missing")
    );
}

#[test]
fn identical_raw_bytes_from_distinct_acquisitions_preserve_both_annotations() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let first_document = document("identical");
    let first = capture_source_document(&mut store, &first_document).unwrap();
    let second_document = crate::content::FetchedDocument {
        effective_url: Some("https://other-origin.test/same-bytes".into()),
        acquired_at_ms: first_document.acquired_at_ms + 1,
        ..first_document
    };
    let second = capture_source_document(&mut store, &second_document).unwrap();
    assert_eq!(first.raw_manifest, second.raw_manifest);
    assert_ne!(first.annotation_manifest, second.annotation_manifest);
    verify_source_document(&mut store, &first).unwrap();
    verify_source_document(&mut store, &second).unwrap();
    drop(store);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    verify_source_document(&mut reopened, &first).unwrap();
    verify_source_document(&mut reopened, &second).unwrap();
}

#[test]
fn rehashed_annotation_cannot_bypass_fleece_fact_validation() {
    for changed_fact in ["canonical text", "raw link", "target"] {
        let root = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(root.path()).unwrap();
        let saved = capture_source_document(&mut store, &document("original")).unwrap();
        let annotation_id = id(&saved.annotation_manifest);
        let manifest =
            pollster::block_on(eidetic::manifest::load_manifest(&mut store, annotation_id))
                .unwrap()
                .unwrap();
        let mut record =
            pollster::block_on(fleece::load_fleece_annotation(&mut store, annotation_id))
                .unwrap()
                .unwrap();
        match changed_fact {
            "canonical text" => record
                .extraction
                .canonical_text_record
                .canonical_text
                .push_str(" fabricated"),
            "raw link" => {
                record
                    .extraction
                    .capture
                    .evidence
                    .as_mut()
                    .unwrap()
                    .raw_manifest = Some(eidetic::ManifestId::of_blob(b"unrelated raw"))
            },
            "target" => record.target.capture_hash = eidetic::Hash::of(b"unrelated target"),
            _ => unreachable!(),
        }
        // Save through the lower typed layer: payload hash and schema are valid,
        // but the higher Fleece facts no longer agree. Admission must reject it.
        let changed_id = pollster::block_on(eidetic::save_typed(
            &mut store,
            &record,
            Vec::new(),
            manifest.privacy,
            manifest.provenance,
            manifest.trust,
            manifest.created_at,
        ))
        .unwrap();
        let changed = StoredSourceDocument {
            annotation_manifest: changed_id.to_string(),
            ..saved.clone()
        };
        assert!(
            verify_source_document(&mut store, &changed)
                .unwrap_err()
                .contains("annotation validation"),
            "{changed_fact}"
        );
        verify_source_document(&mut store, &saved).unwrap();
    }
}

#[test]
fn request_receipt_failure_reopen_retry_never_loses_or_duplicates_deposit() {
    use super::super::source_capture_receipts::{capture_request, recover_receipts};
    let document = document("candidate");
    let request = request(&document);
    let baseline = tempfile::tempdir().unwrap();
    let mut seeded = FjallStore::open(baseline.path()).unwrap();
    capture_source_document(&mut seeded, &super::document("previous")).unwrap();
    let mut counted = FaultStore::new(seeded, None, false);
    let expected = capture_request(&mut counted, &request, Some(&document)).unwrap();
    let write_count = counted.writes.load(Ordering::SeqCst);
    drop(counted);

    for (fail_at, after_write) in
        (1..=write_count).flat_map(|write| [(write, false), (write, true)])
    {
        let root = tempfile::tempdir().unwrap();
        let mut seed = FjallStore::open(root.path()).unwrap();
        let previous = capture_source_document(&mut seed, &super::document("previous")).unwrap();
        let mut failing = FaultStore::new(seed, Some(fail_at), after_write);
        let error = capture_request(&mut failing, &request, Some(&document)).unwrap_err();
        assert!(
            error.contains("injected deposit write"),
            "{fail_at}/{after_write}: {error}"
        );
        drop(failing);

        let mut reopened = FjallStore::open(root.path()).unwrap();
        verify_source_document(&mut reopened, &previous).unwrap();
        let inventory = recover_receipts(&mut reopened).unwrap();
        assert_eq!(inventory.len(), usize::from(fail_at > 1 || after_write));
        if let Some(receipt) = inventory.first() {
            assert_eq!(receipt.request, request);
        }
        // An intent without retained raw bytes visibly requires the original
        // response. Otherwise repair/reuse can proceed with the origin absent.
        let no_response_retry = capture_request(&mut reopened, &request, None);
        if let Err(error) = no_response_retry {
            assert!(
                error.contains("requires the original"),
                "{fail_at}/{after_write}: {error}"
            );
        }
        assert_eq!(
            capture_request(&mut reopened, &request, Some(&document)).unwrap(),
            expected
        );
        assert_eq!(
            capture_request(&mut reopened, &request, None).unwrap(),
            expected
        );
        drop(reopened);

        let mut fresh = FjallStore::open(root.path()).unwrap();
        assert_eq!(
            capture_request(&mut fresh, &request, None).unwrap(),
            expected
        );
        let inventory = recover_receipts(&mut fresh).unwrap();
        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].stored.as_ref(), Some(&expected));
        verify_source_document(&mut fresh, &previous).unwrap();
        let annotations = pollster::block_on(eidetic::typed::list_typed::<
            fleece::FleeceAnnotationRecord,
        >(&mut fresh))
        .unwrap();
        assert_eq!(annotations.len(), 2, "receipt retry duplicated an artifact");
    }
}

#[test]
fn receipt_rejects_reused_request_context_and_unreadable_recovery_inventory() {
    use super::super::source_capture_receipts::{capture_request, recover_receipts};
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let document = document("candidate");
    let request = request(&document);
    let saved = capture_request(&mut store, &request, Some(&document)).unwrap();
    for changed in ["session", "node", "response"] {
        let mut reused = request.clone();
        match changed {
            "session" => reused.session = crate::panes::SessionId::new(),
            "node" => reused.node = uuid::Uuid::new_v4(),
            "response" => reused.source.as_mut().unwrap().acquired_at_ms += 1,
            _ => unreachable!(),
        }
        assert!(
            capture_request(&mut store, &reused, None)
                .unwrap_err()
                .contains("identity was reused")
        );
    }
    assert_eq!(capture_request(&mut store, &request, None).unwrap(), saved);
    let key = format!("turnstone/source-capture-request/v1/{}", request.id);
    pollster::block_on(store.put(&key, b"{broken receipt")).unwrap();
    assert!(
        recover_receipts(&mut store)
            .unwrap_err()
            .contains("unreadable")
    );
    assert!(
        capture_request(&mut store, &request, None)
            .unwrap_err()
            .contains("unreadable")
    );
    verify_source_document(&mut store, &saved).unwrap();
}

#[test]
fn saved_acknowledgement_failure_reopens_retries_and_preserves_committed_flag() {
    use super::super::source_capture_receipts::{
        acknowledge_saved, capture_request, recover_receipts,
    };
    for after_write in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut store = FjallStore::open(root.path()).unwrap();
        let document = document("saved-acknowledgement");
        let request = request(&document);
        let saved = capture_request(&mut store, &request, Some(&document)).unwrap();
        assert!(!recover_receipts(&mut store).unwrap()[0].saved);
        let mut failing = FaultStore::new(store, Some(1), after_write);
        let error = acknowledge_saved(&mut failing, &request).unwrap_err();
        assert!(error.contains("injected deposit write"));
        drop(failing);

        let mut reopened = FjallStore::open(root.path()).unwrap();
        let receipts = recover_receipts(&mut reopened).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].saved, after_write,
            "a committed saved receipt survives a lost acknowledgement"
        );
        assert_eq!(
            capture_request(&mut reopened, &request, None).unwrap(),
            saved
        );
        assert_eq!(
            recover_receipts(&mut reopened).unwrap()[0].saved,
            after_write,
            "deposit replay cannot clear a previously acknowledged save"
        );
        acknowledge_saved(&mut reopened, &request).unwrap();
        drop(reopened);

        let mut fresh = FaultStore::new(FjallStore::open(root.path()).unwrap(), Some(1), false);
        assert!(recover_receipts(&mut fresh).unwrap()[0].saved);
        acknowledge_saved(&mut fresh, &request).unwrap();
        assert_eq!(
            fresh.writes.load(Ordering::SeqCst),
            0,
            "duplicate saved acknowledgement revalidates without a new write"
        );
        verify_source_document(&mut fresh, &saved).unwrap();
    }
}

#[test]
fn saved_acknowledgement_requires_exact_verified_completed_receipt() {
    use super::super::source_capture_receipts::{acknowledge_saved, capture_request};
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let document = document("saved-acknowledgement");
    let request = request(&document);
    assert!(
        acknowledge_saved(&mut store, &request)
            .unwrap_err()
            .contains("no deposit receipt")
    );
    let saved = capture_request(&mut store, &request, Some(&document)).unwrap();
    let mut wrong_request = request.clone();
    wrong_request.graph = crate::panes::GraphId::new();
    assert!(
        acknowledge_saved(&mut store, &wrong_request)
            .unwrap_err()
            .contains("different owner")
    );
    acknowledge_saved(&mut store, &request).unwrap();
    let raw_id = id(&saved.raw_manifest);
    pollster::block_on(store.put(
        &format!("blob:{}", raw_id.0.to_hex()),
        b"corrupt saved source",
    ))
    .unwrap();
    assert!(
        acknowledge_saved(&mut store, &request)
            .unwrap_err()
            .contains("payload integrity"),
        "even a previously saved request must pass current local integrity checks"
    );
}

#[test]
fn damaged_saved_artifact_does_not_hide_other_decoded_recovery_receipts() {
    use super::super::source_capture_receipts::{
        acknowledge_saved, capture_request, recover_receipts, verify_receipt,
    };
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let first_document = document("first-saved");
    let first_request = request(&first_document);
    let first = capture_request(&mut store, &first_request, Some(&first_document)).unwrap();
    acknowledge_saved(&mut store, &first_request).unwrap();
    let second_document = document("second-saved");
    let second_request = request(&second_document);
    capture_request(&mut store, &second_request, Some(&second_document)).unwrap();
    acknowledge_saved(&mut store, &second_request).unwrap();
    let raw_id = id(&first.raw_manifest);
    pollster::block_on(store.put(
        &format!("blob:{}", raw_id.0.to_hex()),
        b"corrupt first saved source",
    ))
    .unwrap();
    drop(store);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    let inventory = recover_receipts(&mut reopened).unwrap();
    assert_eq!(inventory.len(), 2);
    for receipt in inventory {
        let result = verify_receipt(&mut reopened, &receipt);
        if receipt.request == first_request {
            assert!(result.unwrap_err().contains("payload integrity"));
        } else {
            assert_eq!(receipt.request, second_request);
            result.unwrap();
        }
    }
}
