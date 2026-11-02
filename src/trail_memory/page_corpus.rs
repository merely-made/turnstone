// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The page table as a corpus the lexical index can mint from: the most recent
/// event that reached each page, with its destination rewritten to the
/// record's address and best title. One tantivy document per page rather than
/// one per visit (the 3x duplication W6a measured), while owner, transition
/// and timestamp stay the real ones rather than invented.
pub(super) fn page_corpus(traces: &[BrowsingTrace], table: &PageTable) -> Vec<BrowsingTrace> {
    let mut latest: BTreeMap<PageFingerprint, (String, TraceEvent)> = BTreeMap::new();
    for trace in traces {
        for event in &trace.events {
            // The table's own memo, so the address is canonicalized once for
            // the whole mint rather than once per fold.
            let Some(fingerprint) = table.by_address.get(&event.to.url).copied() else {
                continue;
            };
            match latest.get_mut(&fingerprint) {
                Some(entry) if event.at_ms >= entry.1.at_ms => {
                    *entry = (trace.owner.clone(), event.clone());
                },
                Some(_) => {},
                None => {
                    latest.insert(fingerprint, (trace.owner.clone(), event.clone()));
                },
            }
        }
    }

    let mut by_owner: BTreeMap<String, Vec<TraceEvent>> = BTreeMap::new();
    for (fingerprint, (owner, mut event)) in latest {
        let Some(record) = table.records.get(&fingerprint) else {
            continue;
        };
        event.to = PageRef {
            url: record.last_url.clone(),
            title: record.title.clone(),
        };
        by_owner.entry(owner).or_default().push(event);
    }
    by_owner
        .into_iter()
        .map(|(owner, mut events)| {
            events.sort_by_key(|event| event.at_ms);
            BrowsingTrace::from_events(owner, events)
        })
        .collect()
}
