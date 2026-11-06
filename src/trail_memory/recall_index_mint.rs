// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The recall mint is `eidetic_search::recall_mint::MintedRecall`; this holds
//! the timing receipt Turnstone takes around it.

use super::*;

/// Times the mint's phases. Eidetic reads no clock, so the host takes the
/// instants as each phase reports done.
struct PhaseTimer {
    last: Instant,
    page_table: Duration,
    lexical: Duration,
    frecency: Duration,
    candidates: Duration,
}

impl MintObserver for PhaseTimer {
    fn phase_done(&mut self, phase: MintPhase) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last);
        self.last = now;
        match phase {
            MintPhase::PageTable => self.page_table = elapsed,
            MintPhase::Lexical => self.lexical = elapsed,
            MintPhase::Frecency => self.frecency = elapsed,
            MintPhase::Candidates => self.candidates = elapsed,
        }
    }
}

impl RecallIndex {
    /// The mint takes no [`RecallConfig`]: both lanes are minted whole and the
    /// only live setting left, the frecency weight, applies at fusion time.
    pub(super) fn mint(
        dir: &Path,
        traces: &[BrowsingTrace],
        texts: &PageTexts,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let mut timer = PhaseTimer {
            last: started,
            page_table: Duration::ZERO,
            lexical: Duration::ZERO,
            frecency: Duration::ZERO,
            candidates: Duration::ZERO,
        };
        let minted = MintedRecall::mint(dir, traces, texts, now_ms(), &mut timer)
            .map_err(|err| format!("re-mint: {err}"))?;
        let counts = minted.counts();
        let receipt = MintReceipt {
            traces: counts.traces,
            events: counts.events,
            pages: counts.pages,
            pages_with_text: counts.pages_with_text,
            text_bytes: counts.text_bytes,
            collapsed_urls: counts.collapsed_urls,
            corpus_bytes: counts.corpus_bytes,
            lexical: timer.lexical,
            frecency: timer.frecency,
            // Includes the corpus walk that precedes the page table.
            page_table: timer.page_table,
            candidates: timer.candidates,
            candidate_bytes: counts.candidate_bytes,
            total: started.elapsed(),
        };
        receipt.emit();
        Ok(Self {
            minted,
            #[cfg(test)]
            receipt,
        })
    }

    /// Answer a typed query.
    pub(super) fn search(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<RecallHit>, String> {
        self.minted
            .search(query, limit, config)
            .map(|hits| {
                hits.into_iter()
                    .map(|hit| RecallHit {
                        url: hit.url,
                        title: hit.title,
                        at_ms: hit.at_ms,
                    })
                    .collect()
            })
            .map_err(|err| format!("search: {err}"))
    }

    /// Pages the typed text could mean, best-frecency first.
    #[cfg(test)]
    pub(super) fn frecency_urls(&self, query: &str, limit: usize) -> Vec<String> {
        self.minted.frecency_urls(query, limit)
    }

    /// Every page's frecency score, keyed by the address a hit opens, for the
    /// harnesses that rank them against another browser's.
    #[cfg(test)]
    pub(super) fn frecency_scores(
        &self,
        traces: &[BrowsingTrace],
        texts: &PageTexts,
    ) -> std::collections::BTreeMap<String, f64> {
        eidetic::page_table(traces, texts.lookup())
            .records
            .values()
            .filter_map(|record| {
                Some((
                    record.last_url.clone(),
                    self.minted.frecency_score(&record.last_url)?,
                ))
            })
            .collect()
    }
}
