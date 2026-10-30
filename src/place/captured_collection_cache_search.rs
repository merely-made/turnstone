// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CapturedCollectionCache {
    /// Search the current derived view. The index is transient and one result
    /// represents one exact canonical-text group.
    pub fn search(&self, query: &str, limit: usize) -> Vec<CapturedSearchHit> {
        use eidetic_search::{DocumentIndex, DocumentIndexConfig, SearchDocument};

        let index = DocumentIndex::from_documents(
            DocumentIndexConfig::default(),
            self.groups.iter().enumerate().map(|(position, group)| {
                let latest = group
                    .contributions
                    .last()
                    .expect("a captured content group always has a contribution");
                let mut aliases: Vec<_> = group
                    .contributions
                    .iter()
                    .map(|contribution| contribution.source.clone())
                    .filter(|source| source != &latest.source)
                    .collect();
                aliases.sort();
                aliases.dedup();
                SearchDocument {
                    key: position,
                    primary_address: latest.source.clone(),
                    aliases,
                    title: Some(latest.title.clone()),
                    body: Some(group.canonical_text.clone()),
                }
            }),
        );
        index
            .search(query, limit)
            .into_iter()
            .filter_map(|hit| {
                self.groups.get(hit.key).map(|group| CapturedSearchHit {
                    canonical_text_hash: group.canonical_text_hash.clone(),
                    title: group
                        .contributions
                        .last()
                        .map(|contribution| contribution.title.clone())
                        .unwrap_or_default(),
                    source: hit.primary_address,
                    contribution_count: group.contributions.len(),
                })
            })
            .collect()
    }
}
