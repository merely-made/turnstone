// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Clone the authoritative trace corpus and fill missing page titles from the
/// application projection. Only the clone reaches the derived index.
pub(super) fn traces_with_titles(
    memory: &BrowsingMemory,
    sources: &[RecallSource],
) -> Vec<BrowsingTrace> {
    let titles: HashMap<&str, &str> = sources
        .iter()
        .map(|source| (source.url.as_str(), source.title.as_str()))
        .collect();
    memory
        .traces()
        .cloned()
        .map(|mut trace| {
            for event in &mut trace.events {
                fill_title(&mut event.to, &titles);
                if let Some(from) = &mut event.from {
                    fill_title(from, &titles);
                }
                for candidate in &mut event.candidates {
                    fill_title(candidate, &titles);
                }
            }
            trace
        })
        .collect()
}
