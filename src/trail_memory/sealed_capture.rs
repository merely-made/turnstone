// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Sealed capture. The write order, receipts and verification are
//! `eidetic::capture::sealed` (memory-gates plan S6, D17); these are the sync
//! entry points the trail actor calls, over Turnstone's types and its Fleece
//! annotator. The caller owns a separate store and serializes access under
//! the existing trail actor. Legacy stores are never opened here.
//!
//! [`content`] stays: it reads canonical text out of the annotation, which
//! the neutral core does not expose yet.
use super::capture_core::{FleeceAnnotator, neutral_document};
use crate::action::StoredSourceDocument;
use crate::content::FetchedDocument;
use crate::research_storage::SourceReadError as HostError;
use crate::source_capture::{CaptureRequest, DepositReceipt};
use eidetic::PayloadSealer;
use eidetic::capture::sealed;

mod kept_document;
pub(super) use kept_document::read_kept_document;

pub(super) fn capture_request(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    document: Option<&FetchedDocument>,
) -> Result<StoredSourceDocument, HostError> {
    let document = document.map(neutral_document);
    pollster::block_on(sealed::capture_request(
        store,
        sealer,
        &FleeceAnnotator,
        &request.into(),
        document.as_ref(),
    ))
    .map(Into::into)
    .map_err(Into::into)
}

pub(super) fn verify(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
    stored: &StoredSourceDocument,
) -> Result<(), HostError> {
    pollster::block_on(sealed::verify(
        store,
        sealer,
        &FleeceAnnotator,
        &request.into(),
        &stored.clone().into(),
    ))
    .map_err(Into::into)
}

pub(super) fn acknowledge(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    request: &CaptureRequest,
) -> Result<(), HostError> {
    pollster::block_on(sealed::acknowledge(
        store,
        sealer,
        &FleeceAnnotator,
        &request.into(),
    ))
    .map_err(Into::into)
}

pub(super) fn receipts(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
) -> Result<Vec<DepositReceipt>, HostError> {
    pollster::block_on(sealed::receipts(store, sealer))
        .map(|held| held.into_iter().map(Into::into).collect())
        .map_err(Into::into)
}

pub(super) fn content(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    admission: &crate::saved_sources::Admission,
) -> Result<crate::saved_sources::Content, crate::research_storage::SourceReadError> {
    content_for_open(store, sealer, admission, false)
}

pub(super) fn content_for_open(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    admission: &crate::saved_sources::Admission,
    include_edition: bool,
) -> Result<crate::saved_sources::Content, HostError> {
    let (record, raw, raw_bytes) = verified_response(store, sealer, admission)?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or("sealed source acquisition missing")?;
    let captured_at_ms = evidence.captured_at.0;
    let peer = fetch::peer::is_address(&admission.url);
    let title = if peer {
        // The graph node's title can already describe a newer version of its name.
        let body = std::str::from_utf8(&raw_bytes).map_err(|error| error.to_string())?;
        fleece::extract_title(&genet_static_dom::StaticDocument::parse(body))
            .unwrap_or_else(|| evidence.final_source.clone())
    } else {
        admission.title.clone()
    };
    let edition = if include_edition && peer {
        Some(std::sync::Arc::new(super::peer_edition::read(
            store,
            sealer,
            &raw.to_string(),
            &evidence.final_source,
            captured_at_ms,
            raw_bytes,
        )?))
    } else {
        None
    };
    let extraction = record.extraction;
    let text = extraction.canonical_text_record.canonical_text;
    let text_hash = extraction.canonical_text_hash.to_string();
    if eidetic::Hash::of(text.as_bytes()).to_string() != text_hash {
        return Err("sealed source text identity mismatch".into());
    }
    Ok(crate::saved_sources::Content {
        edition,
        title,
        origin: extraction.capture.canonical_source,
        captured_at_ms,
        text,
        text_hash,
        raw_manifest: raw.to_string(),
    })
}

/// Open the original HTML response behind one currently admitted annotation.
pub(super) fn verified_response(
    store: &mut dyn eidetic::Store,
    sealer: &dyn PayloadSealer,
    admission: &crate::saved_sources::Admission,
) -> Result<(fleece::FleeceAnnotationRecord, eidetic::ManifestId, Vec<u8>), HostError> {
    if admission.error.is_some() {
        return Err("sealed source admission refused".into());
    }
    let id = sealed::parse_id(
        admission
            .key
            .annotation_manifest
            .strip_prefix(sealed::SEALED_TAG)
            .ok_or("sealed source marker missing")?,
    )?;
    let record: fleece::FleeceAnnotationRecord =
        pollster::block_on(sealed::load(store, sealer, id))?;
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
    let eidetic::models::OpaqueBlob(raw_bytes) =
        pollster::block_on(sealed::load(store, sealer, raw))?;
    Ok((record, raw, raw_bytes))
}

#[cfg(test)]
#[path = "sealed_capture/tests.rs"]
mod tests;
