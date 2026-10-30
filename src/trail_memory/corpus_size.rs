// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Events, indexable bytes and distinct destination addresses across a
/// corpus. Addresses are what the index used to hold one document each of;
/// the page table's record count subtracted from them is what collapsed.
pub(super) fn corpus_size(traces: &[BrowsingTrace]) -> (usize, usize, usize) {
    let mut events = 0;
    let mut bytes = 0;
    let mut urls = HashSet::new();
    for trace in traces {
        events += trace.events.len();
        for event in &trace.events {
            bytes += event.to.url.len() + event.to.title.as_deref().map_or(0, str::len);
            urls.insert(event.to.url.as_str());
        }
    }
    (events, bytes, urls.len())
}
