// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Return retained graph URLs ordered by the kernel's visit-history facet.
pub(crate) fn recent_visited_urls(graph: &Graph, limit: usize) -> Vec<String> {
    let mut visited = graph
        .nodes()
        .filter_map(|(key, node)| {
            graph
                .node_last_visited(key)
                .map(|at| (at, node.id, node.url().to_owned()))
        })
        .collect::<Vec<_>>();
    visited.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    visited
        .into_iter()
        .take(limit)
        .map(|(_, _, url)| url)
        .collect()
}
