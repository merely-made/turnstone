// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Durable node references to explicitly captured source documents.
//!
//! The Fleece annotation owns all capture facts. This facet retains only its
//! manifest identities in append order, so it can select a node's captures
//! after restart without reconstructing an association by scanning Eidetic.

use chartulary::{AcceptAll, FacetError, FacetId};
use serde::{Deserialize, Serialize};

/// Identity of the acquired response, independent of the current URL/cursor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub hash: String,
    pub body_hash: String,
    pub effective_url: Option<String>,
    pub content_type: Option<String>,
    pub acquired_at_ms: u64,
}

impl SourceIdentity {
    pub(crate) fn of(document: &crate::content::FetchedDocument) -> Self {
        Self {
            hash: eidetic::Hash::of(&document.bytes).to_string(),
            body_hash: eidetic::Hash::of(document.body.as_bytes()).to_string(),
            effective_url: document.effective_url.clone(),
            content_type: document.content_type.clone(),
            acquired_at_ms: document.acquired_at_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub id: uuid::Uuid,
    pub session: crate::panes::SessionId,
    pub graph: crate::panes::GraphId,
    pub node: uuid::Uuid,
    pub url: String,
    pub source: Option<SourceIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturePhase {
    Depositing,
    Deposited,
    PendingSave,
    Failed,
    Saved,
    Recoverable,
}

#[derive(Clone, Debug)]
pub struct CaptureAttempt {
    pub request: CaptureRequest,
    pub stored: Option<crate::action::StoredSourceDocument>,
    pub phase: CapturePhase,
    pub error: Option<String>,
    pub(crate) recovered: bool,
}

/// A durable request receipt is recovery inventory, never corpus admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DepositReceipt {
    #[serde(default)]
    pub saved: bool,
    pub request: CaptureRequest,
    pub stored: Option<crate::action::StoredSourceDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CaptureAttachment {
    request: CaptureRequest,
    stored: crate::action::StoredSourceDocument,
}

const ATTACHMENTS_FACET: &str = "capture.source-requests/v1";

/// A fork carries source references, never the donor's local operation/ack
/// authority. Its new node can make its own explicit capture requests.
pub(crate) fn discard_copied_request_admission(
    facets: &mut pandect::NodeFacetStore,
    node: uuid::Uuid,
) {
    facets.remove(&node, &FacetId::new(ATTACHMENTS_FACET));
}

/// Read the request ledger only when every occurrence still has an ordered
/// reference. Legacy references may interleave; duplicate manifests count.
fn attachment_records(
    facets: &pandect::NodeFacetStore,
    node: uuid::Uuid,
) -> Result<Vec<CaptureAttachment>, String> {
    let series: Vec<String> =
        match facets.get(&node, &FacetId::new(SOURCE_CAPTURE_REFERENCES_FACET)) {
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|error| format!("capture reference series is unreadable: {error}"))?,
            None => Vec::new(),
        };
    let records: Vec<CaptureAttachment> = match facets.get(&node, &FacetId::new(ATTACHMENTS_FACET))
    {
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| format!("capture request ledger is unreadable: {error}"))?,
        None => Vec::new(),
    };
    let mut ids = std::collections::HashSet::new();
    let mut references = series.iter();
    for record in &records {
        if record.request.node != node
            || !ids.insert(record.request.id)
            || !references.any(|reference| *reference == record.stored.annotation_manifest)
        {
            return Err("capture request ledger and ordered references disagree".into());
        }
    }
    Ok(records)
}

/// Correlates an ordered reference with its explicit request across restart.
pub(crate) fn attached(
    facets: &pandect::NodeFacetStore,
    request: &CaptureRequest,
) -> Option<crate::action::StoredSourceDocument> {
    Some(
        attachment_records(facets, request.node)
            .ok()?
            .into_iter()
            .find(|entry| entry.request == *request)?
            .stored,
    )
}

pub(crate) fn attach_reference(
    facets: &mut pandect::NodeFacetStore,
    node: uuid::Uuid,
    request: &CaptureRequest,
    stored: &crate::action::StoredSourceDocument,
) -> Result<(), String> {
    if node != request.node {
        return Err("capture member identity mismatch".into());
    }
    let key = FacetId::new(ATTACHMENTS_FACET);
    let mut receipts = attachment_records(facets, node)?;
    if let Some(existing) = receipts.iter().find(|entry| entry.request.id == request.id) {
        return if existing.request == *request && existing.stored == *stored {
            Ok(())
        } else {
            Err("capture request identity was reused for different evidence".into())
        };
    }
    // Publish both facets together in memory. Persistence is ordered, fallible,
    // and acknowledged separately; this does not claim a storage transaction.
    let mut replacement = facets.clone();
    append_reference(&mut replacement, node, stored.annotation_manifest.clone())
        .map_err(|error| error.to_string())?;
    receipts.push(CaptureAttachment {
        request: request.clone(),
        stored: stored.clone(),
    });
    replacement
        .set(node, key, serde_json::json!(receipts), &AcceptAll)
        .map_err(|error| error.to_string())?;
    *facets = replacement;
    Ok(())
}

/// A node-scoped ordered series of Eidetic Fleece annotation manifest ids.
pub const SOURCE_CAPTURE_REFERENCES_FACET: &str = "capture.source-annotations/v1";

/// Read one node's source annotation references in capture order.
pub fn references(facets: &pandect::NodeFacetStore, node: uuid::Uuid) -> Vec<String> {
    facets
        .get(&node, &FacetId::new(SOURCE_CAPTURE_REFERENCES_FACET))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}

/// Append exactly one explicit capture reference. Repeated captures remain
/// repeated entries: vector position is the only copied ordering fact.
pub fn append_reference(
    facets: &mut pandect::NodeFacetStore,
    node: uuid::Uuid,
    annotation_manifest: String,
) -> Result<(), FacetError> {
    let mut series = references(facets, node);
    series.push(annotation_manifest);
    facets.set(
        node,
        FacetId::new(SOURCE_CAPTURE_REFERENCES_FACET),
        serde_json::json!(series),
        &AcceptAll,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_survive_facet_round_trip_in_capture_order() {
        let root = std::env::temp_dir().join(format!(
            "turnstone-source-capture-facet-{}",
            uuid::Uuid::new_v4()
        ));
        let node = uuid::Uuid::new_v4();
        let mut facets = pandect::NodeFacetStore::new();
        append_reference(&mut facets, node, "annotation-a".into()).unwrap();
        append_reference(&mut facets, node, "annotation-b".into()).unwrap();
        crate::session::save_node_facets(&root, &facets);

        let reopened = crate::session::load_node_facets(&root).unwrap();
        assert_eq!(
            references(&reopened, node),
            vec!["annotation-a".to_string(), "annotation-b".to_string()],
            "the facet is a durable ordered reference series"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
