// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Current application titles available to the derived trail index. Graph
/// nodes and recycle-bin records stay authoritative; the trail port receives
/// a value projection with each recall request.
pub(super) fn recall_sources(app: &crate::app::App) -> Vec<crate::trail_memory::RecallSource> {
    let graph = app.graph_runtimes.graph();
    let mut sources = graph
        .nodes()
        .filter_map(|(_, node)| {
            crate::trail_memory::RecallSource::new(node.url(), node.title.clone())
        })
        .collect::<Vec<_>>();
    sources.extend(app.removed.iter().filter_map(|record| {
        crate::trail_memory::RecallSource::new(&record.url, record.title.as_deref()?)
    }));
    sources
}
