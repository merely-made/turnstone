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
    let record: fleece::FleeceAnnotationRecord = pollster::block_on(sealed::load(store, sealer, id))?;
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
    let _: eidetic::models::OpaqueBlob = pollster::block_on(sealed::load(store, sealer, raw))?;
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

#[cfg(test)]
#[path = "sealed_capture/tests.rs"]
mod tests;
