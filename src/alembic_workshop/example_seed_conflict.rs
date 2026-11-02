// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn example_seed_conflict(graph: &Graph) -> bool {
    let snapshot = graph.to_snapshot_at(0);
    EXAMPLES.iter().any(|example| {
        let url = format!(
            "turnstone://alembic-example/{}/{}",
            example.family, example.version
        );
        let canonical = chartulary::canonical_url(&url);
        let resource = chartulary::resource_id_from_canonical_iri(&canonical);
        snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == example.id.to_string())
            .is_some_and(|node| {
                node.url != url
                    || node.title != example.title
                    || node.body.as_deref() != Some(example.body)
            })
            || snapshot
                .resources
                .iter()
                .find(|record| record.canonical_iri == canonical)
                .is_some_and(|record| !record.facets.is_empty())
            || snapshot.shown_resources.iter().any(|shown| {
                shown.resource_id == resource.to_string()
                    && shown.surface_id != example.id.to_string()
            })
    })
}
