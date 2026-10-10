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
pub use scenomise::grouping::GroupViewState;
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
    /// Original assertions, with their explanations and source provenance.
    pub witnesses: Vec<scenomise::projection::DisclosedRelationship>,
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
    pub occurrences: BTreeMap<String, NodeKey>,
}

/// The viewer's definition: every occurrence in id order on a spiral,
/// labelled by its `label` field.
pub fn viewer_definition(dataset: &ProjectionDataset) -> ProjectionDefinition {
    ProjectionDefinition {
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
    let mut occurrences = BTreeMap::new();
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
        occurrences.insert(occurrence.clone(), key);
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
            witnesses: vec![relationship.disclosure.clone()],
        });
    }
    Ok(HostDatasetView {
        graph,
        positions,
        relations,
        revision: envelope.dataset.revision.as_str().to_owned(),
        occurrences,
    })
}

/// A shared host-input controller. It compiles the complete arrangement once;
/// opening a group changes disclosure, never the remaining nodes' coordinates.
pub struct GroupedHostDataset {
    envelope: HostDatasetV1,
    pub hierarchy: scenomise::grouping::GroupHierarchy,
    pub state: scenomise::grouping::GroupViewState,
    positions: BTreeMap<String, PortablePoint>,
    labels: BTreeMap<String, String>,
}

impl GroupedHostDataset {
    pub fn new(envelope: HostDatasetV1, membership_kind: &str) -> Result<Self, String> {
        let hierarchy = scenomise::grouping::GroupHierarchy::new(
            &envelope.dataset,
            &envelope.relationships,
            membership_kind,
        )?;
        let complete = host_dataset_view(&envelope)?;
        let position_by_key: BTreeMap<_, _> = complete.positions.into_iter().collect();
        let positions = complete
            .occurrences
            .iter()
            .map(|(id, key)| (id.clone(), position_by_key[key]))
            .collect();
        let labels = complete
            .occurrences
            .iter()
            .map(|(id, key)| (id.clone(), complete.graph.node_display_label(*key)))
            .collect();
        Ok(Self {
            envelope,
            hierarchy,
            state: Default::default(),
            positions,
            labels,
        })
    }

    pub fn label(&self, id: &str) -> &str {
        &self.labels[id]
    }
    pub fn total_occurrences(&self) -> usize {
        self.envelope.dataset.occurrences.len()
    }

    /// Remember current coordinates, including a user's drag, before changing
    /// the visible graph. Hidden nodes retain their last remembered position.
    pub fn remember_positions(
        &mut self,
        positions: impl IntoIterator<Item = (String, PortablePoint)>,
    ) {
        for (id, position) in positions {
            if self.positions.contains_key(&id) && position.x.is_finite() && position.y.is_finite()
            {
                self.positions.insert(id, position);
            }
        }
    }

    pub fn projection(&self) -> Result<scenomise::grouping::GroupProjection, String> {
        self.hierarchy.project(&self.state)
    }

    /// Apply disclosure while retaining the viewport, surviving focus and the
    /// current coordinates of every visible source identity. Physics pauses.
    pub fn apply_to_canvas(
        &mut self,
        canvas: &mut mere::canvas::Canvas,
    ) -> Result<Vec<ViewedRelation>, String> {
        self.remember_positions(canvas.graph().nodes().filter_map(|(key, node)| {
            let id = node.url().strip_prefix("urn:host-dataset:")?.to_owned();
            Some((id, canvas.world_position_of(key)?))
        }));
        let view = self.view()?;
        let viewport = canvas.viewport();
        let selected = canvas.focused_url().map(str::to_owned);
        canvas.set_physics_paused(true);
        canvas.set_graph(view.graph);
        canvas.apply_strategy_positions(&view.positions);
        canvas.land_paused_positions(&view.positions);
        canvas.set_viewport(viewport);
        if let Some(url) = selected {
            canvas.select_by_url(&url);
        }
        Ok(view.relations)
    }

    pub fn view(&self) -> Result<HostDatasetView, String> {
        let projected = self.projection()?;
        let mut graph = Graph::new();
        let mut occurrences = BTreeMap::new();
        let mut positions = Vec::new();
        for id in &projected.visible {
            let url = format!("urn:host-dataset:{id}");
            let position = self.positions[id];
            let key = add_node(
                &mut graph,
                Some(Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes())),
                url,
                position,
            );
            apply_graph_delta(
                &mut graph,
                GraphDelta::SetNodeTitle {
                    key,
                    title: self.labels[id].clone(),
                },
            );
            occurrences.insert(id.clone(), key);
            positions.push((key, position));
        }
        let by_id: BTreeMap<_, _> = self
            .envelope
            .relationships
            .iter()
            .map(|r| (&r.id, r))
            .collect();
        let mut relations = Vec::new();
        for bundle in &projected.relations {
            assert_semantic_predicate_in_scope(
                &mut graph,
                occurrences[&bundle.from],
                occurrences[&bundle.to],
                bundle.kind.clone(),
                GraphScope::Custom("grouped-view".into()),
            )
            .ok_or("Could not draw a grouped relationship")?;
            let witnesses: Vec<_> = bundle
                .relationship_ids
                .iter()
                .map(|id| by_id[id].clone())
                .collect();
            let id_bytes =
                serde_json::to_vec(&bundle.relationship_ids).map_err(|e| e.to_string())?;
            let id = format!("bundle:{}", Uuid::new_v5(&Uuid::NAMESPACE_URL, &id_bytes));
            let label = if witnesses.len() == 1 {
                witnesses[0].label.clone()
            } else {
                format!(
                    "{} ({} disclosed relationships)",
                    witnesses[0].label,
                    witnesses.len()
                )
            };
            relations.push(ViewedRelation {
                id,
                kind: bundle.kind.clone(),
                label,
                from: self.labels[&bundle.from].clone(),
                to: self.labels[&bundle.to].clone(),
                witnesses,
            });
        }
        Ok(HostDatasetView {
            graph,
            positions,
            relations,
            revision: self.envelope.dataset.revision.as_str().into(),
            occurrences,
        })
    }

    pub fn relationship(&self, id: &str) -> Option<&scenomise::projection::DisclosedRelationship> {
        self.envelope.relationships.iter().find(|r| r.id == id)
    }
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
    fn repository_expansion_keeps_source_identity_witnesses_and_remembered_placement() {
        let envelope = parse_host_dataset(include_str!(
            "../web/fixtures/grouped-manifest-components.json"
        ))
        .unwrap();
        let original = envelope.clone();
        let mut grouped = GroupedHostDataset::new(envelope, "contains").unwrap();
        let closed = grouped.view().unwrap();
        assert_eq!(closed.graph.node_count(), 3);
        assert_eq!(closed.relations.len(), 3);
        let key = closed.occurrences["repo:genet"];
        let identity = closed.graph.get_node(key).unwrap().id;
        let remembered = PortablePoint::new(123.0, 456.0);
        grouped.remember_positions([("repo:genet".into(), remembered)]);
        grouped
            .state
            .expanded
            .extend(["repo:mere".into(), "workspace:mere:root".into()]);
        let expanded = grouped.view().unwrap();
        assert_eq!(expanded.graph.node_count(), 10);
        let key = expanded.occurrences["repo:genet"];
        assert_eq!(expanded.graph.get_node(key).unwrap().id, identity);
        assert_eq!(
            expanded
                .positions
                .iter()
                .find(|(k, _)| *k == key)
                .unwrap()
                .1,
            remembered
        );
        for relation in &expanded.relations {
            for witness in &relation.witnesses {
                assert_eq!(
                    Some(witness),
                    original.relationships.iter().find(|r| r.id == witness.id)
                );
            }
        }
        grouped.state.expanded.clear();
        assert_eq!(grouped.view().unwrap().graph.node_count(), 3);
        assert_eq!(grouped.envelope, original);
    }

    #[test]
    fn entering_a_workspace_exposes_member_crates_and_external_repository_context() {
        let envelope = parse_host_dataset(include_str!(
            "../web/fixtures/grouped-manifest-components.json"
        ))
        .unwrap();
        let mut grouped = GroupedHostDataset::new(envelope, "contains").unwrap();
        grouped.state.entered = Some("workspace:mere:root".into());
        let projected = grouped.projection().unwrap();
        assert_eq!(projected.breadcrumbs, ["repo:mere", "workspace:mere:root"]);
        assert_eq!(projected.boundary.len(), 2);
        assert!(projected.visible.contains("crate:mere:scenomise"));
        assert!(projected.visible.contains("repo:genet"));
        assert!(projected.visible.contains("repo:woodshed"));
        assert_eq!(grouped.view().unwrap().graph.node_count(), 9);
    }

    #[test]
    fn a_canvas_group_swap_preserves_dragged_coordinates_camera_and_surviving_focus() {
        let envelope = parse_host_dataset(include_str!(
            "../web/fixtures/grouped-manifest-components.json"
        ))
        .unwrap();
        let mut grouped = GroupedHostDataset::new(envelope, "contains").unwrap();
        let view = grouped.view().unwrap();
        let mut canvas = mere::canvas::Canvas::with_graph(view.graph);
        canvas.set_physics_paused(true);
        let key = view.occurrences["repo:genet"];
        let mut positions = view.positions;
        let dragged = PortablePoint::new(317.0, -211.0);
        positions.iter_mut().find(|(k, _)| *k == key).unwrap().1 = dragged;
        canvas.land_paused_positions(&positions);
        let mut camera = canvas.camera();
        camera.zoom = 1.7;
        camera.offset = (29.0, 83.0);
        canvas.set_camera(camera);
        canvas.select_by_url("urn:host-dataset:repo:genet");
        grouped.state.expanded.insert("repo:mere".into());
        grouped.apply_to_canvas(&mut canvas).unwrap();
        assert_eq!(canvas.camera().zoom, 1.7);
        assert_eq!(canvas.camera().offset, (29.0, 83.0));
        assert_eq!(canvas.focused_url(), Some("urn:host-dataset:repo:genet"));
        let (key, _) = canvas
            .graph()
            .nodes()
            .find(|(_, n)| n.url() == "urn:host-dataset:repo:genet")
            .unwrap();
        assert_eq!(canvas.world_position_of(key), Some(dragged));
        assert!(canvas.physics_paused());
        grouped.state.expanded.clear();
        grouped.apply_to_canvas(&mut canvas).unwrap();
        let (key, _) = canvas
            .graph()
            .nodes()
            .find(|(_, n)| n.url() == "urn:host-dataset:repo:genet")
            .unwrap();
        assert_eq!(canvas.world_position_of(key), Some(dragged));
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
