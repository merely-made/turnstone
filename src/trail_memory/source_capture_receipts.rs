// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Request-scoped deposit recovery under the existing trail store owner.

use crate::action::StoredSourceDocument;
use crate::content::FetchedDocument;
use crate::source_capture::{CaptureRequest, DepositReceipt, SourceIdentity};

const PREFIX: &str = "turnstone/source-capture-request/v1/";

fn key(request: &CaptureRequest) -> String {
    format!("{PREFIX}{}", request.id)
}

fn decode(key: &str, bytes: &[u8]) -> Result<DepositReceipt, String> {
    let receipt: DepositReceipt = serde_json::from_slice(bytes)
        .map_err(|error| format!("capture recovery receipt {key} is unreadable: {error}"))?;
    if key != self::key(&receipt.request)
        || receipt.request.source.is_none()
        || (receipt.saved && receipt.stored.is_none())
    {
        return Err(format!(
            "capture recovery receipt {key} has inconsistent request identity"
        ));
    }
    Ok(receipt)
}

fn put(store: &mut dyn eidetic::Store, receipt: &DepositReceipt) -> Result<(), String> {
    let bytes = serde_json::to_vec(receipt)
        .map_err(|error| format!("capture recovery receipt encode: {error}"))?;
    pollster::block_on(store.put(&key(&receipt.request), &bytes))
        .map_err(|error| format!("capture recovery receipt write: {error}"))
}

fn verify_request(
    store: &mut dyn eidetic::Store,
    request: &CaptureRequest,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    super::verify_source_document(store, stored)?;
    let source = request
        .source
        .as_ref()
        .ok_or("capture request has no source identity")?;
    if source.hash != stored.raw_manifest {
        return Err("capture request raw identity differs from its deposit".into());
    }
    let annotation_id = serde_json::from_value(serde_json::json!(stored.annotation_manifest))
        .map_err(|error| format!("capture annotation identity: {error}"))?;
    let record = pollster::block_on(fleece::load_fleece_annotation(store, annotation_id))
        .map_err(|error| format!("capture request annotation: {error}"))?
        .ok_or("capture request annotation is missing")?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or("capture request annotation has no acquisition evidence")?;
    if source.effective_url.as_deref() != Some(&evidence.final_source)
        || source.acquired_at_ms != evidence.captured_at.0
        || source.content_type != evidence.response_content_type
    {
        return Err("capture request acquisition context differs from its deposit".into());
    }
    Ok(())
}

/// Reconstruct a response only from its exact local raw and decoded-body blobs.
/// The original decoder's output is retained independently of response encoding;
/// recovery never guesses a charset, replaces evidence, or fetches the origin.
fn recover_document(
    store: &mut dyn eidetic::Store,
    source: &SourceIdentity,
) -> Result<FetchedDocument, String> {
    let hash: eidetic::Hash = serde_json::from_value(serde_json::json!(source.hash))
        .map_err(|error| format!("capture source identity: {error}"))?;
    let bytes = pollster::block_on(store.get(&format!("blob:{}", hash.to_hex())))
        .map_err(|error| format!("capture retry raw read: {error}"))?
        .ok_or("capture retry requires the original response; no local raw deposit exists")?;
    if eidetic::Hash::of(&bytes) != hash {
        return Err("capture retry raw payload integrity mismatch".into());
    }
    let body_hash: eidetic::Hash = serde_json::from_value(serde_json::json!(source.body_hash))
        .map_err(|error| format!("capture decoded-body identity: {error}"))?;
    let body_bytes = pollster::block_on(store.get(&format!("blob:{}", body_hash.to_hex())))
        .map_err(|error| format!("capture retry decoded-body read: {error}"))?
        .ok_or(
            "capture retry requires the original decoded response; no local body deposit exists",
        )?;
    if eidetic::Hash::of(&body_bytes) != body_hash {
        return Err("capture retry decoded-body payload integrity mismatch".into());
    }
    let body = String::from_utf8(body_bytes)
        .map_err(|error| format!("capture retry decoded body is not UTF-8: {error}"))?;
    let document = FetchedDocument {
        bytes,
        body,
        effective_url: source.effective_url.clone(),
        content_type: source.content_type.clone(),
        acquired_at_ms: source.acquired_at_ms,
    };
    if SourceIdentity::of(&document) != *source {
        return Err("capture retry requires the original decoded response".into());
    }
    Ok(document)
}

/// A request id is an idempotency key, not an artifact identity. Every retry
/// must bind all original owner and response facts before reusing a result.
pub(super) fn capture_request(
    store: &mut dyn eidetic::Store,
    request: &CaptureRequest,
    document: Option<&FetchedDocument>,
) -> Result<StoredSourceDocument, String> {
    let source = request
        .source
        .as_ref()
        .ok_or("capture request has no source identity")?;
    let existing = pollster::block_on(store.get(&key(request)))
        .map_err(|error| format!("capture recovery receipt read: {error}"))?
        .map(|bytes| decode(&key(request), &bytes))
        .transpose()?;
    if let Some(existing) = &existing {
        if existing.request != *request {
            return Err(
                "capture request identity was reused for different owner or source evidence".into(),
            );
        }
        if let Some(stored) = &existing.stored {
            verify_request(store, request, stored)?;
            return Ok(stored.clone());
        }
    }
    let recovered;
    let document = match document {
        Some(document) if SourceIdentity::of(document) == *source => document,
        Some(_) if existing.is_none() => {
            return Err("capture retry response differs from the admitted source identity".into());
        },
        // A partial request owns its retained original response. A newer live
        // response must not prevent its exact local recovery or replace it.
        Some(_) | None => {
            recovered = recover_document(store, source)?;
            &recovered
        },
    };
    if SourceIdentity::of(document) != *source {
        return Err("capture retry response differs from the admitted source identity".into());
    }
    if existing.is_none() {
        put(
            store,
            &DepositReceipt {
                request: request.clone(),
                stored: None,
                saved: false,
            },
        )?;
    }
    // The original decoded string is needed to finish an interrupted deposit
    // without re-decoding raw bytes differently after restart. It shares the
    // existing content-addressed blob namespace and is usually the raw blob.
    let body_hash = eidetic::Hash::of(document.body.as_bytes());
    let body_key = format!("blob:{}", body_hash.to_hex());
    match pollster::block_on(store.get(&body_key))
        .map_err(|error| format!("capture decoded-body deposit read: {error}"))?
    {
        Some(existing) if existing == document.body.as_bytes() => {},
        Some(_) => {
            return Err(
                "capture decoded-body payload integrity mismatch at existing address".into(),
            );
        },
        None => {
            let manifest_key = eidetic::ManifestId::from_hash(body_hash).store_key();
            if pollster::block_on(store.get(&manifest_key))
                .map_err(|error| format!("capture decoded-body manifest check: {error}"))?
                .is_some()
            {
                return Err(
                    "capture decoded-body deposit refuses missing existing artifact payload".into(),
                );
            }
            pollster::block_on(store.put(&body_key, document.body.as_bytes()))
                .map_err(|error| format!("capture decoded-body deposit: {error}"))?;
        },
    }
    let stored = super::capture_source_document_in(store, document)?;
    verify_request(store, request, &stored)?;
    put(
        store,
        &DepositReceipt {
            request: request.clone(),
            stored: Some(stored.clone()),
            saved: false,
        },
    )?;
    Ok(stored)
}

/// Return an already completed legacy capture without depositing new plaintext.
/// An unfinished legacy request requires explicit recovery; it is never
/// continued through the sealed capture path.
pub(super) fn completed_legacy(
    store: &mut dyn eidetic::Store,
    request: &CaptureRequest,
) -> Result<StoredSourceDocument, String> {
    let receipt_key = key(request);
    let bytes = pollster::block_on(store.get(&receipt_key))
        .map_err(|_| "legacy capture receipt unavailable")?
        .ok_or("legacy capture receipt missing")?;
    let receipt = decode(&receipt_key, &bytes)?;
    if receipt.request != *request {
        return Err("legacy capture request identity conflict".into());
    }
    let stored = receipt
        .stored
        .ok_or("unfinished legacy capture needs explicit recovery")?;
    verify_request(store, request, &stored)?;
    Ok(stored)
}

/// Revalidate one completed receipt independently, so one damaged artifact
/// does not hide other decoded requests in recovery inventory. The caller
/// reports this request's error and offers explicit recovery rather than
/// promoting its stored saved flag into current authority.
pub(super) fn verify_receipt(
    store: &mut dyn eidetic::Store,
    receipt: &DepositReceipt,
) -> Result<(), String> {
    let stored = receipt
        .stored
        .as_ref()
        .ok_or("capture receipt has no completed deposit")?;
    verify_request(store, &receipt.request, stored)
}

/// Mark the request saved only after the host has acknowledged its owning
/// facet/graph writes. A lost acknowledgement can safely repeat this call;
/// completed receipts are still revalidated, but never rewritten merely to ack.
pub(super) fn acknowledge_saved(
    store: &mut dyn eidetic::Store,
    request: &CaptureRequest,
) -> Result<(), String> {
    let receipt_key = key(request);
    let bytes = pollster::block_on(store.get(&receipt_key))
        .map_err(|error| format!("capture save receipt read: {error}"))?
        .ok_or("capture save acknowledgement has no deposit receipt")?;
    let mut receipt = decode(&receipt_key, &bytes)?;
    if receipt.request != *request {
        return Err(
            "capture save acknowledgement belongs to different owner or source evidence".into(),
        );
    }
    let stored = receipt
        .stored
        .as_ref()
        .ok_or("capture save acknowledgement has no completed deposit")?;
    verify_request(store, request, stored)?;
    if !receipt.saved {
        receipt.saved = true;
        put(store, &receipt)?;
    }
    Ok(())
}

/// Enumerate request inventory, never the unreferenced artifact corpus. Each
/// recovered deposit needs per-item `verify_receipt` before a saved flag can
/// restore Saved status. Pending deposits also need a session-save acknowledgement.
/// Malformed receipt inventory fails closed with an attributable error; it is
/// never silently treated as an empty successful recovery.
pub(super) fn recover_receipts(
    store: &mut dyn eidetic::Store,
) -> Result<Vec<DepositReceipt>, String> {
    let mut keys = pollster::block_on(store.list(PREFIX))
        .map_err(|error| format!("capture recovery inventory: {error}"))?;
    keys.sort();
    keys.into_iter()
        .map(|key| {
            let bytes = pollster::block_on(store.get(&key))
                .map_err(|error| format!("capture recovery receipt {key}: {error}"))?
                .ok_or_else(|| format!("capture recovery receipt {key} disappeared"))?;
            decode(&key, &bytes)
        })
        .collect()
}
