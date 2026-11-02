// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Store the exact acquired bytes and the Fleece annotation that describes a
/// canonical-text projection of those bytes. This is invoked only by the
/// explicit action path, never by fetch enrichment or trail recall.
#[cfg(test)]
pub(super) fn capture_source_document(
    store: &mut FjallStore,
    document: &crate::content::FetchedDocument,
) -> Result<StoredSourceDocument, String> {
    capture_source_document_in(store, document).map_err(|error| error.to_string())
}
