// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Sealed capture primitives. The caller owns a separate store and serializes
//! access under the existing trail actor. Legacy stores are never opened here.
use crate::action::StoredSourceDocument;
use crate::content::FetchedDocument;
use crate::source_capture::{CaptureRequest, DepositReceipt, SourceIdentity};
use eidetic::{Backend, PayloadSealer, TypedPayload};
use serde::{Deserialize, Serialize};

const PREFIX: &str = "turnstone/sealed-source-capture/v1/";
const TAG: &str = "sealed-v1:";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptEnvelope {
    hash: eidetic::Hash,
    marker: eidetic::SealedBlobRef,
    ciphertext: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundReceipt {
    namespace: String,
    receipt: DepositReceipt,
}
fn key(id: uuid::Uuid) -> String {
    format!("{PREFIX}{id}")
}
fn put_receipt(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    receipt: &DepositReceipt,
) -> Result<(), String> {
    let k = key(receipt.request.id);
    let clear = serde_json::to_vec(&BoundReceipt {
        namespace: k.clone(),
        receipt: receipt.clone(),
    })
    .map_err(|_| "sealed capture receipt encoding")?;
    let hash = eidetic::Hash::of(&clear);
    let (ciphertext, marker) = sealer
        .seal(&hash, &clear)
        .map_err(|_| "sealed capture custody refused")?;
    let value = serde_json::to_vec(&ReceiptEnvelope {
        hash,
        marker,
        ciphertext,
    })
    .map_err(|_| "sealed capture envelope encoding")?;
    pollster::block_on(store.put(&k, &value))
        .map_err(|e| format!("sealed capture receipt write: {e}"))
}
fn read_receipt(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    k: &str,
) -> Result<Option<DepositReceipt>, String> {
    let Some(bytes) = pollster::block_on(store.get(k))
        .map_err(|e| format!("sealed capture receipt read: {e}"))?
    else {
        return Ok(None);
    };
    let envelope: ReceiptEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| "sealed capture receipt envelope invalid")?;
    let clear = sealer
        .unseal(&envelope.hash, &envelope.marker, &envelope.ciphertext)
        .map_err(|_| "sealed capture custody refused")?;
    if eidetic::Hash::of(&clear) != envelope.hash {
        return Err("sealed capture receipt digest mismatch".into());
    }
    let held: BoundReceipt =
        serde_json::from_slice(&clear).map_err(|_| "sealed capture receipt invalid")?;
    if held.namespace != k
        || key(held.receipt.request.id) != k
        || held.receipt.request.source.is_none()
        || (held.receipt.saved && held.receipt.stored.is_none())
    {
        return Err("sealed capture receipt owner mismatch".into());
    }
    Ok(Some(held.receipt))
}

/// A sealed-only namespace never adopts or overwrites an unsealed/partial role.
fn save<T: TypedPayload>(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    value: &T,
    _at: u64,
) -> Result<eidetic::ManifestId, String> {
    let clear = value
        .serialize_to_bytes()
        .map_err(|_| "sealed capture payload encoding")?;
    let id = eidetic::ManifestId::of_blob(&clear);
    let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, id))
        .map_err(|e| format!("sealed capture manifest read: {e}"))?;
    let blob_key = format!("blob:{}", id.0.to_hex());
    let existing = pollster::block_on(store.get(&blob_key))
        .map_err(|e| format!("sealed capture blob read: {e}"))?;
    match (manifest, existing) {
        (Some(m), Some(_)) => {
            if m.id != id
                || m.content_hash != id.0
                || m.schema != T::schema_ref()
                || m.privacy != eidetic::PrivacyClass::LocalOnly
                || m.byte_size != clear.len() as u64
                || m.sources.len() != 1
                || !matches!(&m.sources[0], eidetic::BlobSource::Local { key } if key == &blob_key)
                || eidetic::seal::seal_marker(&m)
                    .map_err(|_| "sealed capture marker invalid")?
                    .is_none()
            {
                return Err("sealed capture refuses a conflicting or unsealed artifact".into());
            }
            let value = pollster::block_on(eidetic::load_typed_sealed::<T>(
                store,
                &mut eidetic::NoFetcher,
                Some(sealer),
                id,
            ))
            .map_err(|_| "sealed capture existing artifact refused")?
            .ok_or("sealed capture existing artifact missing")?;
            if value
                .serialize_to_bytes()
                .map_err(|_| "sealed capture payload encoding")?
                != clear
            {
                return Err("sealed capture existing artifact differs".into());
            }
            return Ok(id);
        },
        (None, None) => {},
        _ => return Err("sealed capture incomplete artifact requires repair".into()),
    }
    pollster::block_on(eidetic::save_typed_sealed(
        store,
        Some(sealer),
        value,
        Vec::new(),
        eidetic::PrivacyClass::LocalOnly,
        eidetic::ProvenanceRecord {
            origin: eidetic::ProvenanceOrigin::Generated,
            upstream: Vec::new(),
            tooling: Some("turnstone-sealed-source/v1".into()),
            generated_at: eidetic::Timestamp(0),
        },
        eidetic::TrustEnvelope {
            level: eidetic::TrustLevel::SelfAsserted,
            signatures: Vec::new(),
            moderation_state: eidetic::ModerationState::Unreviewed,
        },
        eidetic::Timestamp(0),
    ))
    .map_err(|_| "sealed capture artifact write refused".into())
}

fn load<T: TypedPayload>(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    id: eidetic::ManifestId,
) -> Result<T, String> {
    let m = pollster::block_on(eidetic::manifest::load_manifest(store, id))
        .map_err(|_| "sealed source manifest read failed")?
        .ok_or("sealed source manifest missing")?;
    let blob_key = format!("blob:{}", id.0.to_hex());
    if m.id != id
        || m.content_hash != id.0
        || m.schema != T::schema_ref()
        || m.privacy != eidetic::PrivacyClass::LocalOnly
        || m.sources.len() != 1
        || !matches!(&m.sources[0], eidetic::BlobSource::Local { key } if key == &blob_key)
        || eidetic::seal::seal_marker(&m)
            .map_err(|_| "sealed source marker invalid")?
            .is_none()
    {
        return Err("sealed source refuses unsealed or conflicting artifact".into());
    }
    let value = pollster::block_on(eidetic::load_typed_sealed::<T>(
        store,
        &mut eidetic::NoFetcher,
        Some(sealer),
        id,
    ))
    .map_err(|_| "sealed source custody or integrity refusal")?
    .ok_or("sealed source payload missing")?;
    let encoded = value
        .serialize_to_bytes()
        .map_err(|_| "sealed source encoding invalid")?;
    if encoded.len() as u64 != m.byte_size || eidetic::Hash::of(&encoded) != id.0 {
        return Err("sealed source manifest size or digest mismatch".into());
    }
    Ok(value)
}
fn parse_id(s: &str) -> Result<eidetic::ManifestId, String> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| "sealed source identity invalid".into())
}
fn annotation(document: &FetchedDocument) -> Result<fleece::FleeceAnnotationRecord, String> {
    use fleece::eidetic_bridge::{CaptureDomMode, CaptureEvidenceV1};
    if document.acquired_at_ms == 0
        || !document.content_type.as_deref().is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/html"))
        })
    {
        return Err("sealed capture needs observed HTML acquisition".into());
    }
    let url = document
        .effective_url
        .clone()
        .ok_or("sealed capture needs final URL")?;
    let parsed = genet_static_dom::StaticDocument::parse(&document.body);
    let extracted = fleece::extract_document(&parsed);
    let anchor = fleece::anchor_for_range(
        &extracted.page.text,
        fleece::TextPositionSelector {
            start: 0,
            end: extracted.page.text.chars().count() as u64,
        },
        extracted.contract.quote_context,
    )
    .ok_or("sealed capture has no canonical text")?;
    let raw = eidetic::ManifestId::of_blob(&document.bytes);
    let evidence = CaptureEvidenceV1::new(
        url,
        eidetic::Hash::of(&document.bytes),
        eidetic::Timestamp(document.acquired_at_ms),
        document.content_type.clone(),
        CaptureDomMode::Source,
        Some(raw),
        None,
    )
    .map_err(|_| "sealed capture evidence invalid")?;
    fleece::FleeceAnnotationRecord::from_fleece(
        fleece::CaptureIdentity::from_evidence(evidence)
            .map_err(|_| "sealed capture identity invalid")?,
        &extracted,
        &anchor,
    )
    .map_err(|_| "sealed capture annotation invalid".into())
}

pub(super) fn capture_request(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    document: Option<&FetchedDocument>,
) -> Result<StoredSourceDocument, String> {
    let source = request
        .source
        .as_ref()
        .ok_or("sealed capture missing source identity")?;
    let existing = read_receipt(store, sealer, &key(request.id))?;
    if let Some(held) = &existing {
        if held.request != *request {
            return Err("sealed capture request identity conflict".into());
        }
        if let Some(stored) = &held.stored {
            verify(store, sealer, request, stored)?;
            return Ok(stored.clone());
        }
    }
    let recovered;
    let document = match document {
        Some(doc) if SourceIdentity::of(doc) == *source => doc,
        _ if existing.is_some() => {
            let raw: eidetic::models::OpaqueBlob = load(store, sealer, parse_id(&source.hash)?)?;
            let body: eidetic::models::OpaqueBlob =
                load(store, sealer, parse_id(&source.body_hash)?)?;
            recovered = FetchedDocument {
                bytes: raw.0,
                body: String::from_utf8(body.0)
                    .map_err(|_| "sealed capture decoded body invalid")?,
                effective_url: source.effective_url.clone(),
                content_type: source.content_type.clone(),
                acquired_at_ms: source.acquired_at_ms,
            };
            if SourceIdentity::of(&recovered) != *source {
                return Err("sealed capture recovery identity mismatch".into());
            }
            &recovered
        },
        _ => return Err("sealed capture request does not match live source".into()),
    };
    // Refuse oversized input before extraction or any storage write.
    const MAX_CUSTODY_PAYLOAD: usize = 512 * 1024;
    if document.bytes.len() > MAX_CUSTODY_PAYLOAD || document.body.len() > MAX_CUSTODY_PAYLOAD {
        return Err("Private source exceeds the custody payload limit".into());
    }
    let record = annotation(document)?;
    let annotation_bytes = record
        .serialize_to_bytes()
        .map_err(|_| "sealed annotation encoding")?;
    if annotation_bytes.len() > MAX_CUSTODY_PAYLOAD {
        return Err("Private source exceeds the custody payload limit".into());
    }
    let raw = eidetic::ManifestId::of_blob(&document.bytes);
    let annotation_id = eidetic::ManifestId::of_blob(&annotation_bytes);
    let body_id = eidetic::ManifestId::of_blob(document.body.as_bytes());
    if raw == annotation_id
        || body_id == annotation_id
        || raw == fleece::FLEECE_ANNOTATION_SCHEMA_REF.0
        || body_id == fleece::FLEECE_ANNOTATION_SCHEMA_REF.0
        || annotation_id == fleece::FLEECE_ANNOTATION_SCHEMA_REF.0
    {
        return Err("sealed capture artifact roles collide".into());
    }
    if existing.is_none() {
        put_receipt(
            store,
            sealer,
            &DepositReceipt {
                request: request.clone(),
                stored: None,
                saved: false,
            },
        )?;
    }
    save(
        store,
        sealer,
        &eidetic::models::OpaqueBlob(document.bytes.clone()),
        document.acquired_at_ms,
    )?;
    save(
        store,
        sealer,
        &eidetic::models::OpaqueBlob(document.body.as_bytes().to_vec()),
        document.acquired_at_ms,
    )?;
    save(store, sealer, &record, document.acquired_at_ms)?;
    let stored = StoredSourceDocument {
        raw_manifest: raw.to_string(),
        annotation_manifest: format!("{TAG}{annotation_id}"),
    };
    verify(store, sealer, request, &stored)?;
    put_receipt(
        store,
        sealer,
        &DepositReceipt {
            request: request.clone(),
            stored: Some(stored.clone()),
            saved: false,
        },
    )?;
    Ok(stored)
}

pub(super) fn verify(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    let source = request
        .source
        .as_ref()
        .ok_or("sealed capture source missing")?;
    if stored.raw_manifest != source.hash {
        return Err("sealed capture raw identity mismatch".into());
    }
    let id = parse_id(
        stored
            .annotation_manifest
            .strip_prefix(TAG)
            .ok_or("sealed source marker missing")?,
    )?;
    let record: fleece::FleeceAnnotationRecord = load(store, sealer, id)?;
    record
        .validate_integrity()
        .map_err(|_| "sealed source annotation invalid")?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or("sealed source acquisition evidence missing")?;
    let raw_id = parse_id(&source.hash)?;
    if !evidence.has_authoritative_acquisition_context()
        || evidence.dom_mode != fleece::eidetic_bridge::CaptureDomMode::Source
        || evidence.replay_manifest.is_some()
        || !evidence.response_content_type.as_deref().is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/html"))
        })
        || evidence.raw_manifest != Some(raw_id)
        || evidence.capture_hash != raw_id.0
        || source.effective_url.as_deref() != Some(&evidence.final_source)
        || source.acquired_at_ms != evidence.captured_at.0
        || source.content_type != evidence.response_content_type
    {
        return Err("sealed capture acquisition mismatch".into());
    }
    let _: eidetic::models::OpaqueBlob = load(store, sealer, raw_id)?;
    let _: eidetic::models::OpaqueBlob = load(store, sealer, parse_id(&source.body_hash)?)?;
    Ok(())
}

pub(super) fn content(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    admission: &crate::saved_sources::Admission,
) -> Result<crate::saved_sources::Content, String> {
    if admission.error.is_some() {
        return Err("sealed source admission refused".into());
    }
    let id = parse_id(
        admission
            .key
            .annotation_manifest
            .strip_prefix(TAG)
            .ok_or("sealed source marker missing")?,
    )?;
    let record: fleece::FleeceAnnotationRecord = load(store, sealer, id)?;
    record
        .validate_integrity()
        .map_err(|_| "sealed source annotation invalid")?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or("sealed source acquisition missing")?;
    let raw = evidence
        .raw_manifest
        .ok_or("sealed source raw identity missing")?;
    if evidence.capture_hash != raw.0
        || !evidence.has_authoritative_acquisition_context()
        || evidence.dom_mode != fleece::eidetic_bridge::CaptureDomMode::Source
        || evidence.replay_manifest.is_some()
        || !evidence.response_content_type.as_deref().is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("text/html"))
        })
    {
        return Err("sealed source acquisition identity mismatch".into());
    }
    let _: eidetic::models::OpaqueBlob = load(store, sealer, raw)?;
    let captured_at_ms = evidence.captured_at.0;
    let extraction = record.extraction;
    let text = extraction.canonical_text_record.canonical_text;
    let text_hash = extraction.canonical_text_hash.to_string();
    if eidetic::Hash::of(text.as_bytes()).to_string() != text_hash {
        return Err("sealed source text identity mismatch".into());
    }
    Ok(crate::saved_sources::Content {
        title: admission.title.clone(),
        origin: extraction.capture.canonical_source,
        captured_at_ms,
        text,
        text_hash,
        raw_manifest: raw.to_string(),
    })
}
pub(super) fn acknowledge(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
) -> Result<(), String> {
    let mut receipt =
        read_receipt(store, sealer, &key(request.id))?.ok_or("sealed capture receipt missing")?;
    if receipt.request != *request {
        return Err("sealed capture acknowledgement conflict".into());
    }
    verify(
        store,
        sealer,
        request,
        receipt
            .stored
            .as_ref()
            .ok_or("sealed capture deposit incomplete")?,
    )?;
    if !receipt.saved {
        receipt.saved = true;
        put_receipt(store, sealer, &receipt)?;
    }
    Ok(())
}
pub(super) fn receipts(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
) -> Result<Vec<DepositReceipt>, String> {
    let mut keys = pollster::block_on(store.list(PREFIX))
        .map_err(|_| "sealed capture inventory unavailable")?;
    keys.sort();
    keys.into_iter()
        .map(|key| {
            read_receipt(store, sealer, &key)?
                .ok_or_else(|| "sealed capture receipt disappeared".into())
        })
        .collect()
}

#[cfg(test)]
#[path = "sealed_capture/tests.rs"]
mod tests;
