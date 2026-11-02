// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RecallIndex {
    /// The mint takes no [`RecallConfig`]: both lanes are minted whole and the
    /// only live setting left, the frecency weight, applies at fusion time.
    pub(super) fn mint(
        dir: &Path,
        traces: &[BrowsingTrace],
        texts: &PageTexts,
    ) -> Result<Self, String> {
        let started = Instant::now();
        let (events, corpus_bytes, urls) = corpus_size(traces);
        let table_started = Instant::now();
        let table = page_table_of(traces, texts);
        let documents = recall_documents(&table);
        let page_traces = page_corpus(traces, &table);
        let table_elapsed = table_started.elapsed();
        let lexical_started = Instant::now();
        // The record's own text, not the snapshot's: the page table already
        // resolved which body belongs to the address a document is keyed by.
        let bodies: BTreeMap<&str, &str> = table
            .records
            .values()
            .filter_map(|record| Some((record.last_url.as_str(), record.text.as_deref()?)))
            .collect();
        // Re-minted on every recall and never opened from disk, so the
        // projection stays in memory: the persisted write is pure cost here.
        let lexical = TrailIndex::rebuild_with_config(
            index_dir(dir),
            &page_traces,
            |url| bodies.get(url).map(|text| (*text).to_string()),
            IndexConfig {
                persist: false,
                ..IndexConfig::default()
            },
        )
        .map_err(|err| format!("re-mint: {err}"))?;
        let lexical_elapsed = lexical_started.elapsed();
        let frecency_started = Instant::now();
        let frecency: BTreeMap<String, f64> =
            frecency_by_page(traces, now_ms(), &FrecencyConfig::default(), &table)
                .into_iter()
                .filter_map(|(fingerprint, score)| {
                    table
                        .records
                        .get(&fingerprint)
                        .map(|record| (record.last_url.clone(), score))
                })
                .collect();
        let frecency_elapsed = frecency_started.elapsed();
        // The behavioural lane's narrowing step, over the same text the lane
        // used to substring-scan: every address that resolved to the record,
        // the address a hit opens, and the title.
        let candidates_started = Instant::now();
        let mut frecency_by_id = Vec::with_capacity(table.records.len());
        let candidates = CandidateIndex::build(table.records.values().map(|record| {
            let mut texts: Vec<&str> = record.urls.iter().map(String::as_str).collect();
            texts.push(record.last_url.as_str());
            if let Some(title) = record.title.as_deref() {
                texts.push(title);
            }
            frecency_by_id.push(frecency.get(&record.last_url).copied().unwrap_or(0.0));
            (record.last_url.clone(), texts)
        }));
        let candidates_elapsed = candidates_started.elapsed();
        let index = Self {
            lexical,
            frecency,
            receipt: MintReceipt {
                traces: traces.len(),
                events,
                pages: documents.len(),
                pages_with_text: bodies.len(),
                text_bytes: bodies.values().map(|text| text.len()).sum(),
                collapsed_urls: urls.saturating_sub(documents.len()),
                corpus_bytes,
                lexical: lexical_elapsed,
                frecency: frecency_elapsed,
                page_table: table_elapsed,
                candidates: candidates_elapsed,
                candidate_bytes: candidates.memory_bytes(),
                total: started.elapsed(),
            },
            candidates,
            frecency_by_id,
            documents,
        };
        index.receipt.emit();
        Ok(index)
    }

    /// Pages the typed text could mean, best-frecency first. The case this
    /// lane answers is a typed URL prefix, and the page a prefix should recall
    /// is the one visited most and most recently. Pages with no behavioural
    /// evidence (only redirects and reloads) score zero and stay out — an empty
    /// lane, not a zero-rank one.
    ///
    /// Narrowing is [`CandidateIndex`]'s token prefix, not the whole-query
    /// substring this used to scan every page for: `zette` no longer reaches
    /// `gazette`, and the lane costs a lookup rather than 42k lowercasings.
    pub(super) fn frecency_urls(&self, query: &str, limit: usize) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(u32, f64)> = self
            .candidates
            .candidate_ids(query)
            .into_iter()
            .map(|id| (id, self.frecency_by_id[id as usize]))
            .filter(|(_, score)| score.is_finite() && *score > 0.0)
            .collect();
        // Best score first, id as the tiebreak; only the head is ordered,
        // since a two-character prefix can name thousands of pages.
        let order = |left: &(u32, f64), right: &(u32, f64)| {
            right
                .1
                .total_cmp(&left.1)
                .then_with(|| left.0.cmp(&right.0))
        };
        if scored.len() > limit {
            scored.select_nth_unstable_by(limit - 1, order);
            scored.truncate(limit);
        }
        scored.sort_unstable_by(order);
        scored
            .into_iter()
            .map(|(id, _)| self.candidates.key(id).clone())
            .collect()
    }

    pub(super) fn fused_hits(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<FusedHit>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let candidate_limit = limit.saturating_mul(FUSION_CANDIDATE_MULTIPLIER).max(limit);
        let lexical_hits = self
            .lexical
            .search(query, candidate_limit)
            .map_err(|err| format!("search: {err}"))?;
        let mut seen = HashSet::new();
        let mut lexical_urls = Vec::new();
        for hit in &lexical_hits {
            if seen.insert(hit.url.clone()) {
                lexical_urls.push(hit.url.clone());
            }
        }

        let frecency_urls = if config.frecency_enabled() {
            self.frecency_urls(query, candidate_limit)
        } else {
            Vec::new()
        };

        // Lane order is the contract fusion's named ranks read: lexical, then
        // behavioural.
        Ok(fuse_many(
            &[
                Ranking::new(&lexical_urls, 1.0),
                Ranking::new(&frecency_urls, f64::from(config.frecency_weight)),
            ],
            RRF_K,
        )
        .into_iter()
        .take(limit)
        .collect())
    }

    pub(super) fn search(
        &self,
        query: &str,
        limit: usize,
        config: RecallConfig,
    ) -> Result<Vec<RecallHit>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        // The behavioural lane off is the compatibility path: preserve
        // TrailIndex's ranking and metadata exactly.
        if !config.frecency_enabled() {
            return Ok(self
                .lexical
                .search(query, limit)
                .map_err(|err| format!("search: {err}"))?
                .into_iter()
                .map(|hit| RecallHit {
                    url: hit.url,
                    title: hit.title,
                    at_ms: hit.at_ms,
                })
                .collect());
        }

        Ok(self
            .fused_hits(query, limit, config)?
            .into_iter()
            .filter_map(|fused| self.documents.get(&fused.url).cloned())
            .collect())
    }
}
