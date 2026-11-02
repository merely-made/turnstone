// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl MintReceipt {
    /// One line per mint. Tracing is the receipt's only surface: threading it
    /// through `Update::RecallHits` would widen the omnibar API for a number
    /// the omnibar does not render.
    pub(super) fn emit(&self) {
        tracing::info!(
            traces = self.traces,
            events = self.events,
            pages = self.pages,
            pages_with_text = self.pages_with_text,
            collapsed_urls = self.collapsed_urls,
            corpus_bytes = self.corpus_bytes,
            text_bytes = self.text_bytes,
            page_table_us = self.page_table.as_micros() as u64,
            candidates_us = self.candidates.as_micros() as u64,
            candidate_bytes = self.candidate_bytes,
            lexical_us = self.lexical.as_micros() as u64,
            frecency_us = self.frecency.as_micros() as u64,
            total_us = self.total.as_micros() as u64,
            "trail memory: recall index minted"
        );
    }
}
