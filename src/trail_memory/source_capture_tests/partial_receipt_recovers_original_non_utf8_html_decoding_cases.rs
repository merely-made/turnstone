// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn partial_receipt_recovers_original_non_utf8_html_decoding_offline() {
    use super::super::source_capture_receipts::{capture_request, recover_receipts};
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
    use super::super::source_capture_receipts::capture_request;
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
