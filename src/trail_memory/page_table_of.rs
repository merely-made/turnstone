// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The page table over a corpus, taking each page's body from the store's
/// snapshot (W6c). A page with no stored text still gets a record; its
/// fingerprint falls back to the canonical URL, which the record names.
pub(super) fn page_table_of(traces: &[BrowsingTrace], texts: &PageTexts) -> PageTable {
    page_table(traces, texts.lookup())
}
