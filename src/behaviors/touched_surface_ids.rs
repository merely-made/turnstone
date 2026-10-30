// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Surfaces affected in the graph state visible to the after-dispatch drain.
///
/// Record writes reach every current view of the resource. A tag concept's
/// record also changes the tag label projected onto the resources that point
/// to it with taggedWith. Only that exact predicate adds this dependency;
/// arbitrary incoming semantic relations do not propagate content changes.
///
/// Pair replacements name both resource endpoints even after retraction has
/// removed the relation. No Resource UUID is used as a Surface scope fallback.
/// This does not reconstruct mappings or ancestry from before the dispatch.
pub(super) fn touched_surface_ids(graph: &Graph, delta: &CapturedDelta) -> Vec<String> {
    use CapturedDelta as D;
    use mere::kernel::graph::resource::TAGGED_WITH_IRI;

    let mut resources = HashSet::new();
    match delta {
        D::ReplaySetResourceRecordById { resource_id, .. } => {
            if let Ok(id) = resource_id.parse::<uuid::Uuid>() {
                resources.insert(id);
                // Test the relation itself, not the concept's current facet:
                // removing the facet also changes its incoming tag projections.
                for (from, to, payload) in graph.resource_edges() {
                    if to.id() == id
                        && payload
                            .semantic_statements()
                            .iter()
                            .any(|statement| statement.predicate == TAGGED_WITH_IRI)
                    {
                        resources.insert(from.id());
                    }
                }
            }
        },
        D::ReplaySetResourceEdgesByIds {
            from_resource_id,
            to_resource_id,
            ..
        } => {
            for id in [from_resource_id, to_resource_id] {
                if let Ok(id) = id.parse::<uuid::Uuid>() {
                    resources.insert(id);
                }
            }
        },
        // Exhaustive classification is retained in touched_ids above.
        _ => {},
    }
    let mut surfaces: Vec<String> = touched_ids(delta)
        .into_iter()
        .map(str::to_owned)
        .chain(resources.into_iter().flat_map(|id| {
            graph
                .surface_ids_showing_resource(id)
                .into_iter()
                .map(|id| id.to_string())
        }))
        .collect();
    surfaces.sort_unstable();
    surfaces.dedup();
    surfaces
}
