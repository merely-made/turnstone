// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// A deterministic set of ordinary graph edits. The owner must apply these to
/// a detached graph, persist the candidate through its existing session writer,
/// and install it only after that save succeeds.
pub(crate) fn example_seed_deltas(graph: &Graph) -> Vec<GraphDelta> {
    EXAMPLES
        .iter()
        .flat_map(|example| {
            let url = format!(
                "turnstone://alembic-example/{}/{}",
                example.family, example.version
            );
            let canonical = chartulary::canonical_url(&url);
            let resource = chartulary::resource_id_from_canonical_iri(&canonical);
            let add = graph
                .get_node_by_id(example.id)
                .is_none()
                .then(|| GraphDelta::AddNode {
                    id: Some(example.id),
                    url,
                    position: Default::default(),
                });
            std::iter::once(add).flatten().chain([
                GraphDelta::ReplaySetNodeTitleById {
                    node_id: example.id,
                    title: example.title.into(),
                },
                GraphDelta::ReplaySetNodeBodyById {
                    node_id: example.id,
                    body: Some(example.body.into()),
                },
                GraphDelta::ReplaySetNodeMimeHintById {
                    node_id: example.id,
                    mime_hint: Some("text/plain; charset=utf-8".into()),
                },
                GraphDelta::ReplaySetResourceRecordById {
                    resource_id: resource,
                    record: Some(PersistedResourceRecord {
                        canonical_iri: canonical,
                        facets: Vec::new(),
                    }),
                },
                GraphDelta::ReplaySetShownResourceById {
                    surface_id: example.id,
                    resource_id: Some(resource),
                },
            ])
        })
        .collect()
}
