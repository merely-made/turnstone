// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn fixture_resource(graph: &Graph, surface: Uuid, resource: Uuid) -> bool {
    let Some(example) = EXAMPLES.iter().find(|example| example.id == surface) else {
        return false;
    };
    let Some((key, node)) = graph.get_node_by_id(surface) else {
        return false;
    };
    let expected_url = format!(
        "turnstone://alembic-example/{}/{}",
        example.family, example.version
    );
    node.url() == expected_url
        && graph.shown_resource_id(key) == Some(resource)
        && graph.to_snapshot_at(0).nodes.iter().any(|persisted| {
            persisted.node_id == surface.to_string()
                && persisted.title == example.title
                && persisted.body.as_deref() == Some(example.body)
        })
}
