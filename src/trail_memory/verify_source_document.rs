// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Admit only the exact locally addressed raw/annotation pair. Eidetic checks
/// typed payload hashes and Fleece checks annotation facts; the host additionally
/// binds the requested key to the manifest identity and authoritative raw source.
/// No fetcher can repair missing or damaged saved evidence at this boundary.
pub(crate) fn verify_source_document(
    store: &mut dyn eidetic::Store,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    verify_source_document_typed(store, stored).map_err(|error| error.to_string())
}

pub(crate) fn verify_source_document_typed(
    store: &mut dyn eidetic::Store,
    stored: &StoredSourceDocument,
) -> Result<(), crate::research_storage::SourceReadError> {
    use eidetic::{ManifestId, PrivacyClass, ProvenanceOrigin};
    use fleece::eidetic_bridge::CaptureDomMode;

    fn manifest_id(value: &str) -> Result<ManifestId, String> {
        serde_json::from_value(serde_json::Value::String(value.to_owned()))
            .map_err(|error| format!("source manifest identity: {error}"))
    }

    fn checked_manifest(
        store: &mut dyn eidetic::Store,
        id: ManifestId,
    ) -> Result<eidetic::BlobManifest, crate::research_storage::SourceReadError> {
        let manifest = pollster::block_on(eidetic::manifest::load_manifest(store, id))
            .map_err(|error| {
                crate::research_storage::SourceReadError::from(error)
                    .context("source manifest read")
            })?
            .ok_or_else(|| format!("source manifest {id} is missing"))?;
        if manifest.id != id || manifest.content_hash != id.0 {
            return Err(format!("source manifest identity mismatch at {id}").into());
        }
        if manifest.privacy != PrivacyClass::LocalOnly {
            return Err(format!("source manifest {id} is not private local evidence").into());
        }
        let key = format!("blob:{}", id.0.to_hex());
        if !manifest.sources.iter().any(
            |source| matches!(source, eidetic::BlobSource::Local { key: local } if local == &key),
        ) {
            return Err(format!("source manifest {id} has no exact local payload").into());
        }
        let bytes = pollster::block_on(store.get(&key))
            .map_err(|error| {
                crate::research_storage::SourceReadError::from(error).context("source payload read")
            })?
            .ok_or_else(|| format!("source payload {id} is missing"))?;
        if eidetic::Hash::of(&bytes) != id.0 || bytes.len() as u64 != manifest.byte_size {
            return Err(format!("source payload integrity mismatch at {id}").into());
        }
        Ok(manifest)
    }

    let raw_id = manifest_id(&stored.raw_manifest)?;
    let annotation_id = manifest_id(&stored.annotation_manifest)?;
    let raw_manifest = checked_manifest(store, raw_id)?;
    let annotation_manifest = checked_manifest(store, annotation_id)?;
    let record = pollster::block_on(fleece::load_fleece_annotation(store, annotation_id))
        .map_err(|error| {
            crate::research_storage::SourceReadError::from(error)
                .context("source annotation validation")
        })?
        .ok_or_else(|| "source annotation is missing".to_string())?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or_else(|| "source annotation lacks acquisition evidence".to_string())?;
    let html = evidence
        .response_content_type
        .as_deref()
        .is_some_and(|content_type| {
            content_type
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/html"))
        });
    if !evidence.has_authoritative_acquisition_context()
        || evidence.dom_mode != CaptureDomMode::Source
        || !html
        || evidence.raw_manifest != Some(raw_id)
        || evidence.capture_hash != raw_id.0
        || evidence.replay_manifest.is_some()
        // A raw blob is content addressed: identical bytes acquired from two
        // URLs/times share it. Acquisition identity belongs to the annotation,
        // not the mutable metadata of that shared raw manifest.
        || !matches!(raw_manifest.provenance.origin, ProvenanceOrigin::Imported { .. })
        || annotation_manifest.created_at != evidence.captured_at
        || annotation_manifest.provenance.generated_at != evidence.captured_at
        || annotation_manifest.provenance.origin != ProvenanceOrigin::Generated
    {
        return Err("source annotation and raw acquisition provenance disagree".into());
    }
    pollster::block_on(eidetic::load_typed::<eidetic::models::OpaqueBlob>(
        store,
        &mut eidetic::NoFetcher,
        raw_id,
    ))
    .map_err(|error| {
        crate::research_storage::SourceReadError::from(error).context("raw source validation")
    })?
    .ok_or_else(|| "raw source is missing".to_string())?;
    Ok(())
}
