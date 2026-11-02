// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn derived_status_at(derived: &DerivedTextV1, now_ms: u64) -> String {
    let Some(cache) = &derived.cache else {
        return derived.summary.clone();
    };
    let age = format_cache_age(now_ms.saturating_sub(cache.fetched_at_unix_ms));
    let source_label = if cache.sources.len() == 1 {
        "source"
    } else {
        "sources"
    };
    format!(
        "{}\nFetched: {age} ago; {} {source_label}",
        derived.summary,
        cache.sources.len()
    )
}
