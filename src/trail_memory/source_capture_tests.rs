// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use async_trait::async_trait;
use eidetic::{Backend, WriteOp};
use muniment::StoreError;
use std::sync::atomic::{AtomicUsize, Ordering};

fn document(label: &str) -> crate::content::FetchedDocument {
    let body = format!("<html><main><p>{label} naïve kestrel</p></main></html>");
    crate::content::FetchedDocument {
        bytes: body.as_bytes().to_vec(),
        body,
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(format!("https://capture.test/{label}")),
        acquired_at_ms: 1_700_000_000_001,
    }
}

struct ActorFixtureSealer;
impl eidetic::PayloadSealer for ActorFixtureSealer {
    fn seal(
        &self,
        _: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        Ok((
            clear.iter().map(|byte| byte ^ 0x5a).collect(),
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([3; 16]),
                format: "actor-fixture".into(),
            },
        ))
    }

    fn unseal(
        &self,
        _: &eidetic::Hash,
        marker: &eidetic::SealedBlobRef,
        ciphertext: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        if marker.format != "actor-fixture" {
            return Err(eidetic::Error::new("wrong fixture marker"));
        }
        Ok(ciphertext.iter().map(|byte| byte ^ 0x5a).collect())
    }
}

fn actor_fixture_custody() -> crate::trail_memory::SourceCustody {
    crate::trail_memory::SourceCustody {
        sealer: std::sync::Arc::new(ActorFixtureSealer),
        epoch: std::sync::Arc::new(|| Some(1)),
    }
}

/// Fail immediately before or after one real write. The latter models a write
/// that committed before its acknowledgement was lost. Real Fjall deposits
/// remain behind for a fresh owner to reopen and retry.
struct FaultStore {
    inner: FjallStore,
    writes: AtomicUsize,
    fail_at: Option<usize>,
    after_write: bool,
}

impl FaultStore {
    fn new(inner: FjallStore, fail_at: Option<usize>, after_write: bool) -> Self {
        Self {
            inner,
            writes: AtomicUsize::new(0),
            fail_at,
            after_write,
        }
    }
}

#[async_trait]
impl Backend for FaultStore {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let write = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at == Some(write) && !self.after_write {
            return Err(StoreError::Backend(format!(
                "injected deposit write {write}: {key}"
            )));
        }
        self.inner.put(key, bytes).await?;
        if self.fail_at == Some(write) && self.after_write {
            return Err(StoreError::Backend(format!(
                "injected deposit write {write} acknowledged after commit: {key}"
            )));
        }
        Ok(())
    }
    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
        // The capture path currently uses individual put acknowledgements.
        // Preserve the same prefix-failure instrument if it adopts a batch.
        for op in ops {
            match op {
                WriteOp::Put { key, value } => self.put(key, value).await?,
                WriteOp::Delete { key } => self.delete(key).await?,
            }
        }
        Ok(())
    }
}

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

fn id(value: &str) -> eidetic::ManifestId {
    serde_json::from_value(serde_json::json!(value)).unwrap()
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

fn request(document: &crate::content::FetchedDocument) -> crate::source_capture::CaptureRequest {
    crate::source_capture::CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session: crate::panes::SessionId::new(),
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://capture.test/candidate".into(),
        source: Some(crate::source_capture::SourceIdentity::of(document)),
        keep: false,
    }
}

#[test]
fn request_receipt_failure_reopen_retry_never_loses_or_duplicates_deposit() {
    use super::source_capture_receipts::{capture_request, recover_receipts};
    let document = document("candidate");
    let request = request(&document);
    let baseline = tempfile::tempdir().unwrap();
    let mut seeded = FjallStore::open(baseline.path()).unwrap();
    capture_source_document(&mut seeded, &self::document("previous")).unwrap();
    let mut counted = FaultStore::new(seeded, None, false);
    let expected = capture_request(&mut counted, &request, Some(&document)).unwrap();
    let write_count = counted.writes.load(Ordering::SeqCst);
    drop(counted);

    for (fail_at, after_write) in
        (1..=write_count).flat_map(|write| [(write, false), (write, true)])
    {
        let root = tempfile::tempdir().unwrap();
        let mut seed = FjallStore::open(root.path()).unwrap();
        let previous = capture_source_document(&mut seed, &self::document("previous")).unwrap();
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
    use super::source_capture_receipts::{capture_request, recover_receipts};
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
    use super::source_capture_receipts::{acknowledge_saved, capture_request, recover_receipts};
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
    use super::source_capture_receipts::{acknowledge_saved, capture_request};
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
    use super::source_capture_receipts::{
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

#[test]
fn partial_receipt_recovers_original_non_utf8_html_decoding_offline() {
    use super::source_capture_receipts::{capture_request, recover_receipts};
    let document = crate::content::FetchedDocument {
        bytes: b"<html><main><p>na\xefve kestrel</p></main></html>".to_vec(),
        body: "<html><main><p>naïve kestrel</p></main></html>".into(),
        content_type: Some("text/html; charset=windows-1252".into()),
        effective_url: Some("https://capture.test/original-decoder".into()),
        acquired_at_ms: 1_700_000_000_003,
    };
    assert!(std::str::from_utf8(&document.bytes).is_err());
    let request = request(&document);
    let baseline = tempfile::tempdir().unwrap();
    let mut counted = FaultStore::new(FjallStore::open(baseline.path()).unwrap(), None, false);
    let expected = capture_request(&mut counted, &request, Some(&document)).unwrap();
    let last_write = counted.writes.load(Ordering::SeqCst);
    drop(counted);

    let root = tempfile::tempdir().unwrap();
    let mut interrupted = FaultStore::new(
        FjallStore::open(root.path()).unwrap(),
        Some(last_write),
        false,
    );
    assert!(
        capture_request(&mut interrupted, &request, Some(&document))
            .unwrap_err()
            .contains("receipt write")
    );
    drop(interrupted);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    let inventory = recover_receipts(&mut reopened).unwrap();
    assert_eq!(inventory.len(), 1);
    assert!(
        inventory[0].stored.is_none(),
        "the final result receipt never committed"
    );
    let body_hash = eidetic::Hash::of(document.body.as_bytes());
    let body_key = format!("blob:{}", body_hash.to_hex());
    let retained_body = pollster::block_on(reopened.get(&body_key))
        .unwrap()
        .unwrap();
    assert_eq!(retained_body, document.body.as_bytes());
    pollster::block_on(reopened.put(&body_key, b"wrong decoded body under the original address"))
        .unwrap();
    assert!(
        capture_request(&mut reopened, &request, None)
            .unwrap_err()
            .contains("decoded-body payload integrity")
    );
    pollster::block_on(reopened.put(&body_key, &retained_body)).unwrap();
    let mut replacement = document.clone();
    replacement.bytes = b"<html><main><p>new response</p></main></html>".to_vec();
    replacement.body = String::from_utf8(replacement.bytes.clone()).unwrap();
    replacement.acquired_at_ms += 1;
    let unrelated_root = tempfile::tempdir().unwrap();
    let mut unrelated = FjallStore::open(unrelated_root.path()).unwrap();
    assert!(
        capture_request(&mut unrelated, &request, Some(&replacement))
            .unwrap_err()
            .contains("differs from the admitted source"),
        "a fresh request still refuses a substituted live response"
    );
    assert!(recover_receipts(&mut unrelated).unwrap().is_empty());
    let recovered = capture_request(&mut reopened, &request, Some(&replacement)).unwrap();
    assert_eq!(recovered, expected);
    let replacement_hash = eidetic::Hash::of(&replacement.bytes);
    assert!(
        pollster::block_on(reopened.get(&format!("blob:{}", replacement_hash.to_hex())))
            .unwrap()
            .is_none(),
        "partial retry reuses the original response without depositing its replacement"
    );
    let record = pollster::block_on(fleece::load_fleece_annotation(
        &mut reopened,
        id(&recovered.annotation_manifest),
    ))
    .unwrap()
    .unwrap();
    assert!(
        record
            .extraction
            .canonical_text_record
            .canonical_text
            .contains("naïve")
    );
    drop(reopened);
    let mut fresh = FjallStore::open(root.path()).unwrap();
    assert_eq!(
        capture_request(&mut fresh, &request, None).unwrap(),
        expected
    );
}

fn manifest_bytes(store: &mut dyn eidetic::Store, value: &str) -> Vec<u8> {
    pollster::block_on(store.get(&id(value).store_key()))
        .unwrap()
        .unwrap()
}

fn html_bytes(bytes: Vec<u8>, label: &str) -> crate::content::FetchedDocument {
    crate::content::FetchedDocument {
        body: String::from_utf8(bytes.clone()).unwrap(),
        bytes,
        content_type: Some("text/html; charset=utf-8".into()),
        effective_url: Some(format!("https://capture.test/{label}")),
        acquired_at_ms: 1_700_000_000_007,
    }
}

#[test]
fn raw_response_cannot_replace_existing_typed_annotation_at_identical_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let previous = capture_source_document(&mut store, &document("existing-annotation")).unwrap();
    let before = manifest_bytes(&mut store, &previous.annotation_manifest);
    let annotation_id = id(&previous.annotation_manifest);
    let json_bytes = pollster::block_on(store.get(&format!("blob:{}", annotation_id.0.to_hex())))
        .unwrap()
        .unwrap();
    let response = html_bytes(json_bytes, "annotation-json-served-as-html");
    let error = capture_source_document(&mut store, &response).unwrap_err();
    assert!(
        error.contains("incompatible existing manifest role"),
        "{error}"
    );
    assert_eq!(
        manifest_bytes(&mut store, &previous.annotation_manifest),
        before
    );
    verify_source_document(&mut store, &previous).unwrap();
    drop(store);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    assert_eq!(
        manifest_bytes(&mut reopened, &previous.annotation_manifest),
        before
    );
    verify_source_document(&mut reopened, &previous).unwrap();
}

#[test]
fn annotation_deposit_cannot_replace_an_existing_captured_raw_payload() {
    let original_document = document("planned-annotation");
    let plan_root = tempfile::tempdir().unwrap();
    let mut plan = FjallStore::open(plan_root.path()).unwrap();
    let planned = capture_source_document(&mut plan, &original_document).unwrap();
    let planned_id = id(&planned.annotation_manifest);
    let planned_bytes = pollster::block_on(plan.get(&format!("blob:{}", planned_id.0.to_hex())))
        .unwrap()
        .unwrap();
    drop(plan);

    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let previous = capture_source_document(
        &mut store,
        &html_bytes(planned_bytes, "existing-json-response"),
    )
    .unwrap();
    assert_eq!(previous.raw_manifest, planned.annotation_manifest);
    let before = manifest_bytes(&mut store, &previous.raw_manifest);
    let error = capture_source_document(&mut store, &original_document).unwrap_err();
    assert!(
        error.contains("incompatible existing manifest role"),
        "{error}"
    );
    assert_eq!(manifest_bytes(&mut store, &previous.raw_manifest), before);
    assert!(
        pollster::block_on(store.get(&id(&planned.raw_manifest).store_key()))
            .unwrap()
            .is_none(),
        "all roles must be checked before the new raw manifest is deposited"
    );
    verify_source_document(&mut store, &previous).unwrap();
    drop(store);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    verify_source_document(&mut reopened, &previous).unwrap();
}

#[test]
fn schema_role_collision_refuses_in_fresh_and_existing_stores_without_replacement() {
    let schema_fixture = tempfile::tempdir().unwrap();
    let mut fixture = FjallStore::open(schema_fixture.path()).unwrap();
    pollster::block_on(fleece::bootstrap_fleece_annotation_schema(&mut fixture)).unwrap();
    let schema_id = fleece::FLEECE_ANNOTATION_SCHEMA_REF.0;
    let schema_before = manifest_bytes(&mut fixture, &schema_id.to_string());
    let schema_bytes = pollster::block_on(fixture.get(&format!("blob:{}", schema_id.0.to_hex())))
        .unwrap()
        .unwrap();
    let response = html_bytes(schema_bytes, "schema-json-served-as-html");
    let error = capture_source_document(&mut fixture, &response).unwrap_err();
    assert!(error.contains("incompatible typed roles"), "{error}");
    assert_eq!(
        manifest_bytes(&mut fixture, &schema_id.to_string()),
        schema_before
    );
    let schema = pollster::block_on(eidetic::load_typed::<eidetic::SchemaDefinition>(
        &mut fixture,
        &mut eidetic::NoFetcher,
        schema_id,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        schema.schema_id,
        fleece::eidetic_bridge::FLEECE_ANNOTATION_SCHEMA_ID
    );

    let fresh_root = tempfile::tempdir().unwrap();
    let mut fresh = FjallStore::open(fresh_root.path()).unwrap();
    assert!(
        capture_source_document(&mut fresh, &response)
            .unwrap_err()
            .contains("incompatible typed roles")
    );
    assert!(
        pollster::block_on(fresh.list("manifest:"))
            .unwrap()
            .is_empty()
    );
    assert!(pollster::block_on(fresh.list("blob:")).unwrap().is_empty());
}

#[test]
fn incompatible_existing_schema_role_is_preserved_and_refused_before_deposit() {
    let fixture_root = tempfile::tempdir().unwrap();
    let mut fixture = FjallStore::open(fixture_root.path()).unwrap();
    pollster::block_on(fleece::bootstrap_fleece_annotation_schema(&mut fixture)).unwrap();
    let schema_id = fleece::FLEECE_ANNOTATION_SCHEMA_REF.0;
    let schema_bytes = pollster::block_on(fixture.get(&format!("blob:{}", schema_id.0.to_hex())))
        .unwrap()
        .unwrap();
    let valid_manifest =
        pollster::block_on(eidetic::manifest::load_manifest(&mut fixture, schema_id))
            .unwrap()
            .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let opaque_id = pollster::block_on(eidetic::save_typed(
        &mut store,
        &eidetic::models::OpaqueBlob(schema_bytes),
        Vec::new(),
        eidetic::PrivacyClass::LocalOnly,
        valid_manifest.provenance,
        valid_manifest.trust,
        valid_manifest.created_at,
    ))
    .unwrap();
    assert_eq!(opaque_id, schema_id);
    let before = manifest_bytes(&mut store, &opaque_id.to_string());
    let error = capture_source_document(&mut store, &document("ordinary-candidate")).unwrap_err();
    assert!(
        error.contains("incompatible existing manifest role"),
        "{error}"
    );
    assert_eq!(manifest_bytes(&mut store, &opaque_id.to_string()), before);
    pollster::block_on(eidetic::load_typed::<eidetic::models::OpaqueBlob>(
        &mut store,
        &mut eidetic::NoFetcher,
        opaque_id,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        pollster::block_on(store.list("manifest:")).unwrap().len(),
        1
    );
}

#[test]
fn compatible_reuse_preserves_exact_manifest_metadata_and_refuses_corruption() {
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let first_document = document("same-response");
    let first = capture_source_document(&mut store, &first_document).unwrap();
    let raw_before = manifest_bytes(&mut store, &first.raw_manifest);
    let annotation_before = manifest_bytes(&mut store, &first.annotation_manifest);
    assert_eq!(
        capture_source_document(&mut store, &first_document).unwrap(),
        first
    );
    assert_eq!(manifest_bytes(&mut store, &first.raw_manifest), raw_before);
    assert_eq!(
        manifest_bytes(&mut store, &first.annotation_manifest),
        annotation_before
    );
    let second_document = crate::content::FetchedDocument {
        effective_url: Some("https://another.test/identical-response".into()),
        acquired_at_ms: first_document.acquired_at_ms + 1,
        ..first_document.clone()
    };
    let second = capture_source_document(&mut store, &second_document).unwrap();
    assert_eq!(first.raw_manifest, second.raw_manifest);
    assert_eq!(manifest_bytes(&mut store, &first.raw_manifest), raw_before);
    verify_source_document(&mut store, &first).unwrap();
    verify_source_document(&mut store, &second).unwrap();
    let raw_id = id(&first.raw_manifest);
    pollster::block_on(store.put(
        &format!("blob:{}", raw_id.0.to_hex()),
        b"damaged retained payload",
    ))
    .unwrap();
    let error = capture_source_document(&mut store, &first_document).unwrap_err();
    assert!(
        error.contains("preflight: payload integrity mismatch"),
        "{error}"
    );
    assert_eq!(manifest_bytes(&mut store, &first.raw_manifest), raw_before);
}

#[test]
fn new_request_cannot_repair_missing_previous_raw_via_decoded_body_deposit() {
    use super::source_capture_receipts::capture_request;
    let root = tempfile::tempdir().unwrap();
    let mut store = FjallStore::open(root.path()).unwrap();
    let document = document("missing-prior-payload");
    let previous_request = request(&document);
    let previous = capture_request(&mut store, &previous_request, Some(&document)).unwrap();
    let source = previous_request.source.as_ref().unwrap();
    assert_eq!(source.hash, source.body_hash);
    let before = manifest_bytes(&mut store, &previous.raw_manifest);
    let raw_id = id(&previous.raw_manifest);
    let blob_key = format!("blob:{}", raw_id.0.to_hex());
    pollster::block_on(store.delete(&blob_key)).unwrap();
    let new_request = request(&document);
    let error = capture_request(&mut store, &new_request, Some(&document)).unwrap_err();
    assert!(
        error.contains("refuses missing existing artifact payload"),
        "{error}"
    );
    assert!(pollster::block_on(store.get(&blob_key)).unwrap().is_none());
    assert_eq!(manifest_bytes(&mut store, &previous.raw_manifest), before);
    drop(store);
    let mut reopened = FjallStore::open(root.path()).unwrap();
    assert!(
        pollster::block_on(reopened.get(&blob_key))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        manifest_bytes(&mut reopened, &previous.raw_manifest),
        before
    );
    assert!(verify_source_document(&mut reopened, &previous).is_err());
}

#[test]
fn same_directory_actor_reopen_rejects_old_opening_commands_before_writes() {
    use super::source_capture_receipts::recover_receipts;
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

    let stale_document = self::document("stale-opening-must-not-deposit");
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

    let foreign_document = self::document("foreign-session-must-not-deposit");
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
    assert!(pollster::block_on(inspect.get(&format!("blob:{}", stale_hash.to_hex()))).unwrap().is_none());
    let foreign_hash = eidetic::Hash::of(&foreign_document.bytes);
    assert!(pollster::block_on(inspect.get(&format!("blob:{}", foreign_hash.to_hex()))).unwrap().is_none());
    drop(inspect);

    let fixture_custody = actor_fixture_custody();
    let mut inspect = FjallStore::open(super::sealed_memory_dir(&directory)).unwrap();
    let inventory = super::sealed_capture::receipts(&mut inspect, fixture_custody.sealer.as_ref()).unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].request, request);
    assert!(!inventory[0].saved, "wrong-generation ack cannot mark a valid receipt saved");
    for hash in [stale_hash, foreign_hash] {
        assert!(pollster::block_on(inspect.get(&format!("blob:{}", hash.to_hex()))).unwrap().is_none());
    }
    super::sealed_capture::verify(&mut inspect, fixture_custody.sealer.as_ref(), &request, &stored).unwrap();
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
    let mut fresh = FjallStore::open(super::sealed_memory_dir(&directory)).unwrap();
    let inventory = super::sealed_capture::receipts(&mut fresh, fixture_custody.sealer.as_ref()).unwrap();
    assert_eq!(inventory.len(), 1);
    assert!(
        inventory[0].saved,
        "current-generation positive acknowledgement commits"
    );
    super::sealed_capture::verify(&mut fresh, fixture_custody.sealer.as_ref(), &request, &stored).unwrap();
}
