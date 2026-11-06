// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's side of the capture core promoted into `eidetic::capture`
//! (memory-gates plan S6, D17): the Fleece annotator, the conversions between
//! Turnstone's capture types and Eidetic's neutral ones, and the sync entry
//! points the trail actor calls.
//!
//! Fleece sits above Eidetic, so Eidetic cannot name the annotation record;
//! [`FleeceAnnotator`] is the seam it calls instead. Its `annotate` is pure,
//! because the record's manifest id is its content hash and a retry has to
//! land on the same address.

use crate::action::StoredSourceDocument;
use crate::research_storage::SourceReadError;
use crate::source_capture::{CaptureRequest, DepositReceipt, SourceIdentity};
use eidetic::capture as core;
use eidetic::capture::{AcquisitionEvidence, CaptureError, SourceAnnotator};
use eidetic::{Hash, ManifestId, Store, Timestamp};
use fleece::eidetic_bridge::{CaptureDomMode, CaptureEvidenceV1};

/// The annotation seam over `fleece::FleeceAnnotationRecord`.
pub(crate) struct FleeceAnnotator;

#[async_trait::async_trait(?Send)]
impl SourceAnnotator for FleeceAnnotator {
    type Record = fleece::FleeceAnnotationRecord;

    fn schema_manifest(&self) -> ManifestId {
        fleece::FLEECE_ANNOTATION_SCHEMA_REF.0
    }

    fn annotate(&self, document: &core::FetchedDocument) -> Result<Self::Record, CaptureError> {
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
        .ok_or("Fleece produced no canonical text for a whole-document anchor")?;
        let raw = ManifestId::of_blob(&document.bytes);
        let evidence = CaptureEvidenceV1::new(
            url,
            Hash::of(&document.bytes),
            Timestamp(document.acquired_at_ms),
            document.content_type.clone(),
            CaptureDomMode::Source,
            Some(raw),
            None,
        )
        .map_err(|error| format!("capture evidence: {error}"))?;
        fleece::FleeceAnnotationRecord::from_fleece(
            fleece::CaptureIdentity::from_evidence(evidence)
                .map_err(|error| format!("capture identity: {error}"))?,
            &extracted,
            &anchor,
        )
        .map_err(|error| format!("Fleece annotation: {error}").into())
    }

    fn validate(&self, record: &Self::Record) -> Result<(), CaptureError> {
        record.validate_integrity().map_err(Into::into)
    }

    fn evidence(&self, record: &Self::Record) -> Option<AcquisitionEvidence> {
        let evidence = record.extraction.capture.evidence.as_ref()?;
        Some(AcquisitionEvidence {
            final_source: evidence.final_source.clone(),
            capture_hash: evidence.capture_hash,
            captured_at: evidence.captured_at,
            response_content_type: evidence.response_content_type.clone(),
            raw_manifest: evidence.raw_manifest,
            source_dom: evidence.dom_mode == CaptureDomMode::Source,
            has_replay_manifest: evidence.replay_manifest.is_some(),
            authoritative: evidence.has_authoritative_acquisition_context(),
        })
    }

    fn association(&self, record: &Self::Record) -> (String, Hash) {
        let capture = &record.extraction.capture;
        (capture.canonical_source.clone(), capture.capture_hash)
    }

    async fn bootstrap_schema(&self, store: &mut dyn Store) -> Result<(), CaptureError> {
        fleece::bootstrap_fleece_annotation_schema(store)
            .await
            .map_err(Into::into)
    }

    async fn save(
        &self,
        store: &mut dyn Store,
        record: &Self::Record,
        at_ms: u64,
    ) -> Result<(), CaptureError> {
        fleece::save_fleece_annotation(store, record, at_ms)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn load(
        &self,
        store: &mut dyn Store,
        id: ManifestId,
    ) -> Result<Option<Self::Record>, CaptureError> {
        fleece::load_fleece_annotation(store, id)
            .await
            .map_err(Into::into)
    }
}

impl From<CaptureError> for SourceReadError {
    fn from(value: CaptureError) -> Self {
        match value {
            CaptureError::Storage(error) => Self::Storage(error),
            CaptureError::Context { label, cause } => Self::Context {
                label,
                cause: Box::new((*cause).into()),
            },
            CaptureError::Other(error) => Self::Other(error),
        }
    }
}

/// Eidetic's neutral document for a fetched one. `FetchedDocument` is a
/// foreign type here, so this is a function rather than a `From`.
pub(crate) fn neutral_document(document: &crate::content::FetchedDocument) -> core::FetchedDocument {
    core::FetchedDocument {
        bytes: document.bytes.clone(),
        body: document.body.clone(),
        effective_url: document.effective_url.clone(),
        content_type: document.content_type.clone(),
        acquired_at_ms: document.acquired_at_ms,
    }
}

impl From<SourceIdentity> for core::SourceIdentity {
    fn from(value: SourceIdentity) -> Self {
        Self {
            hash: value.hash,
            body_hash: value.body_hash,
            effective_url: value.effective_url,
            content_type: value.content_type,
            acquired_at_ms: value.acquired_at_ms,
        }
    }
}

impl From<core::SourceIdentity> for SourceIdentity {
    fn from(value: core::SourceIdentity) -> Self {
        Self {
            hash: value.hash,
            body_hash: value.body_hash,
            effective_url: value.effective_url,
            content_type: value.content_type,
            acquired_at_ms: value.acquired_at_ms,
        }
    }
}

impl From<&CaptureRequest> for core::CaptureRequest {
    fn from(value: &CaptureRequest) -> Self {
        Self {
            id: value.id,
            session: value.session.0,
            graph: value.graph.0,
            node: value.node,
            url: value.url.clone(),
            source: value.source.clone().map(Into::into),
            keep: value.keep,
        }
    }
}

impl From<core::CaptureRequest> for CaptureRequest {
    fn from(value: core::CaptureRequest) -> Self {
        Self {
            id: value.id,
            session: crate::panes::SessionId(value.session),
            graph: crate::panes::GraphId(value.graph),
            node: value.node,
            url: value.url,
            source: value.source.map(Into::into),
            keep: value.keep,
        }
    }
}

impl From<StoredSourceDocument> for core::StoredSourceDocument {
    fn from(value: StoredSourceDocument) -> Self {
        Self {
            raw_manifest: value.raw_manifest,
            annotation_manifest: value.annotation_manifest,
        }
    }
}

impl From<core::StoredSourceDocument> for StoredSourceDocument {
    fn from(value: core::StoredSourceDocument) -> Self {
        Self {
            raw_manifest: value.raw_manifest,
            annotation_manifest: value.annotation_manifest,
        }
    }
}

impl From<&DepositReceipt> for core::DepositReceipt {
    fn from(value: &DepositReceipt) -> Self {
        Self {
            saved: value.saved,
            request: (&value.request).into(),
            stored: value.stored.clone().map(Into::into),
        }
    }
}

impl From<core::DepositReceipt> for DepositReceipt {
    fn from(value: core::DepositReceipt) -> Self {
        Self {
            saved: value.saved,
            request: value.request.into(),
            stored: value.stored.map(Into::into),
        }
    }
}

/// Store the exact acquired bytes and the Fleece annotation that describes a
/// canonical-text projection of them, in an unsealed store.
pub(crate) fn capture_source_document_in(
    store: &mut dyn Store,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, SourceReadError> {
    pollster::block_on(core::capture_source_document_in(
        store,
        &FleeceAnnotator,
        &neutral_document(document),
    ))
    .map(Into::into)
    .map_err(Into::into)
}

/// Admit only the exact locally addressed raw/annotation pair.
pub(crate) fn verify_source_document_typed(
    store: &mut dyn Store,
    stored: &StoredSourceDocument,
) -> Result<(), SourceReadError> {
    pollster::block_on(core::verify_source_document_typed(
        store,
        &FleeceAnnotator,
        &stored.clone().into(),
    ))
    .map_err(Into::into)
}

/// [`verify_source_document_typed`] with the error rendered to a string.
pub(crate) fn verify_source_document(
    store: &mut dyn Store,
    stored: &StoredSourceDocument,
) -> Result<(), String> {
    verify_source_document_typed(store, stored).map_err(|error| error.to_string())
}
