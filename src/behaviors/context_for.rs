// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The digest of what woke one body: the entries its wake named, in order.
pub fn context_for(entries: &[CommittedEntry], wake: &servitor::Wake) -> TriggerContext {
    let woken_by = wake
        .matched
        .iter()
        .filter_map(|seq| entries.iter().find(|entry| entry.seq == *seq))
        .map(|entry| TriggerEntry {
            seq: entry.seq,
            author: entry.author.clone(),
            // The scope's last segment is the node itself: ancestry is written
            // outermost-first, so the tail is what actually changed.
            nodes: entry
                .scopes
                .iter()
                .filter_map(|scope| scope.segments().last().cloned())
                .collect(),
        })
        .collect();
    TriggerContext { woken_by }
}
