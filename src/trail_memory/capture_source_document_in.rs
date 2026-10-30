// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn capture_source_document_in(
    store: &mut dyn eidetic::Store,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, String> {
    use eidetic::models::OpaqueBlob;
    use eidetic::{
        ModerationState, PrivacyClass, ProvenanceOrigin, ProvenanceRecord, Timestamp,
        TrustEnvelope, TrustLevel,
    };
    use fleece::eidetic_bridge::{CaptureDomMode, CaptureEvidenceV1};

    if document.acquired_at_ms == 0 {
        return Err("source capture requires an observed acquisition time".to_string());
    }
    if !document
        .content_type
        .as_deref()
        .is_some_and(|content_type| {
            content_type
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
        })
    {
        return Err(
            "source capture requires an observed text/html response content type".to_string(),
        );
    }
    let captured_at = Timestamp(document.acquired_at_ms);
    let effective_url = document
        .effective_url
        .clone()
        .ok_or_else(|| "source capture requires an observed final response URL".to_string())?;
    let parsed = genet_static_dom::StaticDocument::parse(&document.body);
    let extracted = fleece::extract_document(&parsed);
    let character_count = extracted.page.text.chars().count() as u64;
    let anchor = fleece::anchor_for_range(
        &extracted.page.text,
        fleece::TextPositionSelector {
            start: 0,
            end: character_count,
        },
        extracted.contract.quote_context,
    )
    .ok_or_else(|| "Fleece produced no canonical text for a whole-document anchor".to_string())?;
    let raw_manifest = eidetic::ManifestId::of_blob(&document.bytes);
    let evidence = CaptureEvidenceV1::new(
        effective_url,
        eidetic::Hash::of(&document.bytes),
        captured_at,
        document.content_type.clone(),
        CaptureDomMode::Source,
        Some(raw_manifest),
        None,
    )
    .map_err(|error| format!("source capture evidence: {error}"))?;
    let record = fleece::FleeceAnnotationRecord::from_fleece(
        fleece::CaptureIdentity::from_evidence(evidence)
            .map_err(|error| format!("source capture identity: {error}"))?,
        &extracted,
        &anchor,
    )
    .map_err(|error| format!("Fleece annotation: {error}"))?;
    // Eidetic IDs address payload bytes, not (schema, bytes). Plan and check
    // every typed role before any deposit, so one role cannot replace another
    // valid artifact's mutable manifest at the same content address.
    use eidetic::TypedPayload;
    let annotation_bytes = record
        .serialize_to_bytes()
        .map_err(|error| format!("Fleece annotation encoding: {error}"))?;
    let annotation_manifest = eidetic::ManifestId::of_blob(&annotation_bytes);
    let schema_manifest = fleece::FLEECE_ANNOTATION_SCHEMA_REF.0;
    if raw_manifest == annotation_manifest
        || raw_manifest == schema_manifest
        || annotation_manifest == schema_manifest
    {
        return Err("source capture has incompatible typed roles at one content address".into());
    }
    let raw_exists = preflight_source_artifact(
        store,
        raw_manifest,
        OpaqueBlob::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&document.bytes),
    )?;
    let annotation_exists = preflight_source_artifact(
        store,
        annotation_manifest,
        fleece::FleeceAnnotationRecord::schema_ref(),
        PrivacyClass::LocalOnly,
        Some(&annotation_bytes),
    )?;
    preflight_source_artifact(
        store,
        schema_manifest,
        *eidetic::META_SCHEMA_REF,
        PrivacyClass::PublicPortable,
        None,
    )?;
    if raw_exists {
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, raw_manifest))
            .map_err(|error| format!("raw source preflight: {error}"))?
            .unwrap();
        if !matches!(
            manifest.provenance.origin,
            ProvenanceOrigin::Imported { .. }
        ) {
            return Err(
                "raw source preflight: incompatible existing acquisition provenance".into(),
            );
        }
    }
    if annotation_exists {
        let stored = StoredSourceDocument {
            raw_manifest: raw_manifest.to_string(),
            annotation_manifest: annotation_manifest.to_string(),
        };
        verify_source_document(store, &stored)?;
    }
    if !raw_exists {
        pollster::block_on(eidetic::save_typed(
            store,
            &OpaqueBlob(document.bytes.clone()),
            Vec::new(),
            PrivacyClass::LocalOnly,
            ProvenanceRecord {
                origin: ProvenanceOrigin::Imported {
                    source: document.effective_url.clone().unwrap(),
                },
                upstream: Vec::new(),
                tooling: Some("turnstone-source-document-capture/v1".into()),
                generated_at: captured_at,
            },
            TrustEnvelope {
                level: TrustLevel::SelfAsserted,
                signatures: Vec::new(),
                moderation_state: ModerationState::Unreviewed,
            },
            captured_at,
        ))
        .map_err(|error| format!("raw source deposit: {error}"))?;
    }
    pollster::block_on(fleece::bootstrap_fleece_annotation_schema(store))
        .map_err(|error| format!("Fleece schema bootstrap: {error}"))?;
    if !annotation_exists {
        pollster::block_on(fleece::save_fleece_annotation(
            store,
            &record,
            document.acquired_at_ms,
        ))
        .map_err(|error| format!("Fleece annotation deposit: {error}"))?;
    } else {
        // Reusing a complete annotation preserves its exact manifest metadata.
        // Its resource association is independently idempotent and may have
        // been the interrupted final write of the previous deposit attempt.
        pollster::block_on(eidetic::ResourceCaptureStore::new(&*store).record(
            &record.extraction.capture.canonical_source,
            record.extraction.capture.capture_hash,
            eidetic::CaptureContent::Acquired,
            document.acquired_at_ms,
        ))
        .map_err(|error| format!("Fleece capture association: {error}"))?;
    }
    let stored = StoredSourceDocument {
        raw_manifest: raw_manifest.to_string(),
        annotation_manifest: annotation_manifest.to_string(),
    };
    verify_source_document(store, &stored)?;
    Ok(stored)
}
