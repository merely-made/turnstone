// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A host-supplied dataset as the viewer's graph (mer3ly site canvas plan,
//! S1; Rulings 124, 125 and 128).
//!
//! The page hands the viewer a `scenomise.host-dataset/v1` envelope. The
//! viewer compiles it through its own definition, [`viewer_definition`], and
//! the relationships are routed onto that arrangement by
//! [`HostDatasetV1::compile`]. Each placed occurrence becomes a graph node
//! where the arrangement put it, titled by its label, and each routed
//! relation becomes an edge carrying the disclosed kind as an open
//! predicate, so the canvas draws it. The relations are also returned as
//! text, for the page's semantic tree. Nothing here knows any one host's
//! schema: the viewer reads the generic projection contract (`occurrence_id`
//! as the reading key, a text `label`) and nothing else.

use std::collections::BTreeMap;

use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{
    GraphDelta, add_node, apply_graph_delta, assert_semantic_predicate_in_scope,
};
use mere::kernel::graph::{Graph, NodeKey};
use mere::kernel::types::GraphScope;
use scenomise::host_dataset::HostDatasetV1;
use uuid::Uuid;

use crate::projection_compile::{ProjectionDataset, practice_compiler};
use crate::projection_editor::{
    Appearance, Arrangement, Channel, Encoding, Interaction, PROJECTION_DEFINITION_VERSION,
    ProjectionDefinition, Provenance, Reading, RevisionEvidence, SelectionMode,
};

/// The viewer's recipe id.
pub const VIEWER_DEFINITION_ID: &str = "host-dataset-viewer";

/// One disclosed relationship as the viewer presents it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewedRelation {
    pub id: String,
    pub kind: String,
    pub label: String,
    /// The labels of the two occurrences it joins.
    pub from: String,
    pub to: String,
}

impl ViewedRelation {
    /// What a reader hears for this relation.
    pub fn spoken(&self) -> String {
        format!("{}: {} to {}", self.label, self.from, self.to)
    }
}

/// A host dataset ready for the viewer's canvas.
pub struct HostDatasetView {
    pub graph: Graph,
    /// Where the viewer's arrangement placed each node, in scene units.
    pub positions: Vec<(NodeKey, PortablePoint)>,
    pub relations: Vec<ViewedRelation>,
    pub revision: String,
}

/// The viewer's definition: every occurrence in id order on a spiral,
/// labelled by its `label` field.
pub fn viewer_definition(dataset: &ProjectionDataset) -> ProjectionDefinition {
    ProjectionDefinition {
        dynamics: None,
        version: PROJECTION_DEFINITION_VERSION,
        id: VIEWER_DEFINITION_ID.into(),
        label: "Host dataset".into(),
        source: dataset.source.clone(),
        reading: Reading {
            kind: "nodes".into(),
            key: "occurrence_id".into(),
            value: None,
        },
        encoding: Encoding {
            x: Channel::Field("occurrence_id".into()),
            y: Channel::Field("occurrence_id".into()),
            color: None,
            label: Some(Channel::Field("label".into())),
        },
        arrangement: Arrangement {
            kind: scenomise::catalog::Family::Spiral.id().into(),
            direction: "coordinates".into(),
            spacing: 16,
            options: BTreeMap::new(),
        },
        interaction: Interaction {
            selection: SelectionMode::Single,
            pan: false,
            zoom: false,
        },
        appearance: Appearance {
            realization: "canvas".into(),
            title: "Host dataset".into(),
            theme: "slate".into(),
        },
        provenance: Provenance {
            author: "Graphshell".into(),
            source_revision: Some(dataset.revision.clone()),
            revision_evidence: RevisionEvidence::PublicGeneration,
            note: "The viewer's own arrangement of a host-supplied dataset.".into(),
        },
    }
}

/// Compile `envelope` for the viewer and build its graph.
pub fn host_dataset_view(envelope: &HostDatasetV1) -> Result<HostDatasetView, String> {
    let compiled = envelope
        .compile(practice_compiler(), &viewer_definition(&envelope.dataset))
        .map_err(|issues| {
            issues
                .iter()
                .map(|issue| format!("{}: {}", issue.field, issue.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    let projection = &compiled.projection;
    let mut graph = Graph::new();
    let mut keys = std::collections::HashMap::new();
    let mut positions = Vec::new();
    for (index, item) in projection.scene.items.iter().enumerate() {
        let instance = sceno::InstanceId(index as u32);
        let occurrence = &projection.occurrence_by_instance[&instance];
        let url = format!("urn:host-dataset:{occurrence}");
        let at = item.transform.translate;
        let key = add_node(
            &mut graph,
            Some(Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())),
            url,
            PortablePoint::new(at.x, at.y),
        );
        apply_graph_delta(
            &mut graph,
            GraphDelta::SetNodeTitle {
                key,
                title: projection.labels[&instance].clone(),
            },
        );
        keys.insert(instance, key);
        positions.push((key, PortablePoint::new(at.x, at.y)));
    }
    let mut relations = Vec::new();
    for (relation, relationship) in projection
        .scene
        .relations
        .iter()
        .zip(&compiled.relationships)
    {
        let kind = relation.kind.clone().unwrap_or_default();
        assert_semantic_predicate_in_scope(
            &mut graph,
            keys[&relation.from],
            keys[&relation.to],
            kind.clone(),
            GraphScope::Source,
        )
        .ok_or_else(|| format!("relation {} could not be drawn", relationship.disclosure.id))?;
        relations.push(ViewedRelation {
            id: relationship.disclosure.id.clone(),
            kind,
            label: relationship.disclosure.label.clone(),
            from: projection.labels[&relation.from].clone(),
            to: projection.labels[&relation.to].clone(),
        });
    }
    Ok(HostDatasetView {
        graph,
        positions,
        relations,
        revision: envelope.dataset.revision.as_str().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenomise::host_dataset::parse_host_dataset;

    /// The served relations scenario's dataset.
    fn served() -> HostDatasetV1 {
        parse_host_dataset(include_str!("../web/fixtures/host-dataset-relations.json")).unwrap()
    }

    fn assert_canvas_projects_all_relations(view: &HostDatasetView) {
        // Content assertions live on Resources; the former Surface-only read is empty.
        assert_eq!(view.graph.relations().count(), 0);
        assert_eq!(
            view.graph.projected_relations().count(),
            view.relations.len()
        );
        let positions: std::collections::HashMap<_, _> = view.positions.iter().copied().collect();
        let projection = mere::canvas::underlay::projection_from_positions(&view.graph, |key| {
            positions.get(&key).copied()
        });
        assert_eq!(projection.nodes.len(), positions.len());
        for node in &projection.nodes {
            assert_eq!(node.position, positions[&node.node]);
        }
        // The canvas draws one stroke per undirected pair, preserving every
        // disclosed connection while collapsing parallel/directional strokes.
        let pair = |from: NodeKey, to: NodeKey| {
            let (a, b) = (from.index(), to.index());
            (a.min(b), a.max(b))
        };
        let expected: std::collections::BTreeSet<_> = view
            .graph
            .projected_relations()
            .filter(|(_, row)| row.from != row.to)
            .map(|(_, row)| pair(row.from, row.to))
            .collect();
        let actual: std::collections::BTreeSet<_> = projection
            .edges
            .iter()
            .map(|edge| pair(edge.from, edge.to))
            .collect();
        assert!(
            !expected.is_empty(),
            "the fixture exercises drawn relationships"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn nodes_and_edges_follow_the_compiled_scene() {
        let envelope = served();
        let view = host_dataset_view(&envelope).unwrap();
        assert_eq!(view.graph.node_count(), envelope.dataset.occurrences.len());
        assert_canvas_projects_all_relations(&view);
        assert_eq!(view.relations.len(), envelope.relationships.len());
        let compiled = envelope
            .compile(practice_compiler(), &viewer_definition(&envelope.dataset))
            .unwrap();
        for index in 0..compiled.projection.scene.items.len() {
            let instance = sceno::InstanceId(index as u32);
            let url = format!(
                "urn:host-dataset:{}",
                compiled.projection.occurrence_by_instance[&instance]
            );
            let (key, _) = view
                .graph
                .nodes()
                .find(|(_, node)| node.url() == url)
                .unwrap();
            assert_eq!(
                view.graph.node_display_label(key),
                compiled.projection.labels[&instance]
            );
        }
    }

    #[test]
    fn relations_are_spoken_by_their_labels_and_endpoints() {
        let view = host_dataset_view(&served()).unwrap();
        let spoken: Vec<_> = view.relations.iter().map(ViewedRelation::spoken).collect();
        assert!(spoken.contains(&"Depends on: Projection grammar to Graph kernel".to_string()));
    }

    #[test]
    fn a_dataset_without_the_label_contract_is_refused() {
        let mut envelope = served();
        envelope.dataset.fields.remove("label");
        for occurrence in &mut envelope.dataset.occurrences {
            occurrence.values.remove("label");
        }
        assert!(
            host_dataset_view(&envelope)
                .err()
                .unwrap()
                .contains("encoding.label")
        );
    }

    #[test]
    fn the_served_refusal_fixtures_are_refused() {
        use scenomise::host_dataset::HostDatasetError;
        assert!(matches!(
            parse_host_dataset(include_str!("../web/fixtures/host-dataset-stale.json")),
            Err(HostDatasetError::StaleRevision { .. })
        ));
        assert!(matches!(
            parse_host_dataset(include_str!(
                "../web/fixtures/host-dataset-unknown-field.json"
            )),
            Err(HostDatasetError::UnknownKey { .. })
        ));
    }

    /// The site exporter's output (merelyllc.com `9df5e96`), pinned by its
    /// sha256 so a changed export is a deliberate fixture update.
    #[test]
    fn the_site_export_compiles_with_every_relation_drawn() {
        use sha2::{Digest, Sha256};
        let bytes = include_bytes!("../web/fixtures/site-repository-host-dataset.json");
        let digest: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            digest,
            "1b16d990958c3fd1fe9cd17a3fa4a98bc62441f28d23fb92ecd7f9d7abf7eca0"
        );
        let envelope = parse_host_dataset(std::str::from_utf8(bytes).unwrap()).unwrap();
        let view = host_dataset_view(&envelope).unwrap();
        assert_eq!(view.graph.node_count(), 21);
        assert_eq!(view.positions.len(), 21);
        assert_eq!(view.relations.len(), 30);
        assert_canvas_projects_all_relations(&view);
    }
}
