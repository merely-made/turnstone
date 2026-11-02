// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(crate) fn example_choices(graph: &Graph) -> Vec<SourceChoice> {
    let snapshot = graph.to_snapshot_at(0);
    EXAMPLES
        .iter()
        .map(|example| {
            let present = graph.get_node_by_id(example.id).is_some_and(|(key, _)| {
                graph.shown_resource_id(key).is_some()
                    && snapshot.nodes.iter().any(|persisted| {
                        persisted.node_id == example.id.to_string()
                            && persisted.title == example.title
                            && persisted.body.as_deref() == Some(example.body)
                    })
            });
            SourceChoice {
                key: example.id.to_string(),
                title: example.title.into(),
                version: example.version.into(),
                selected: false,
                available: present,
                refusal: (!present).then(|| {
                    "Load the public example to retain this version in the current graph.".into()
                }),
            }
        })
        .collect()
}
