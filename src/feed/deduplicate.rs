// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn deduplicate(entries: Vec<FeedEntry>) -> Vec<FeedEntry> {
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .filter(|entry| seen.insert(entry_identity(entry)))
        .collect()
}
