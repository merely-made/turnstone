// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One document per page record, keyed by the address a hit opens (the page's
/// most recent URL). Replaces the URL-string dedup this fold used to do: two
/// addresses for one page are now one document before the index sees them.
pub(super) fn recall_documents(table: &PageTable) -> BTreeMap<String, RecallHit> {
    table
        .records
        .values()
        .map(|record| {
            (
                record.last_url.clone(),
                RecallHit {
                    url: record.last_url.clone(),
                    title: record.title.clone(),
                    at_ms: record.last_seen_ms,
                },
            )
        })
        .collect()
}
