// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Snapshot the complete tail boundary, projecting only the loaded runtime.
pub(super) fn projected_tail(
    app: &App,
    origin: crate::host_journal::RuntimeOrigin,
    cursor: u64,
) -> Option<(u64, Vec<CommittedEntry>)> {
    if app.behavior_origin() != Some(origin) {
        return None;
    }
    let tail = app.journal.lock().ok()?.tail(origin, cursor);
    let graph = app.graph_runtimes.canvas(origin.graph)?.graph();
    let entries = tail
        .entries
        .iter()
        .map(|entry| {
            let scopes = touched_surface_ids(graph, &entry.delta)
                .into_iter()
                .flat_map(|id| ancestry_scopes(graph, &id))
                .collect();

            CommittedEntry::new(entry.seq, entry.author.id.clone(), scopes)
        })
        .collect();
    Some((tail.high_water, entries))
}
