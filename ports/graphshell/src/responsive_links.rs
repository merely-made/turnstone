// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Deterministic real-link reference fixture for responsive path evaluation.
//!
//! This is an in-memory Graphshell fixture, not persisted source truth. It
//! obtains real P1 LinkPairReadings from a small kernel graph and sends those
//! readings through Pictograph's adapter into Numen. The model reports path
//! geometry and its source correspondence; it does not observe force vectors
//! or allocate any share of the graph's spring force.

use std::collections::BTreeMap;

use mere::canvas::responsive_link_adapter::{
    AppearanceCompatibility, FieldRegistry, PathField, ResponsivePath, ResponsivePathConfig,
    ScalarField, SourceBucketRef, VectorField, eval_vector, evaluate_link_appearances,
};
use mere::kernel::graph::{Graph, GraphStratum, SemanticStatement, apply::add_node};
use scenograph::link::{LinkKey, LinkPairReading, LinkSource, LinkSourceRevision, LinkStratum};
use scenograph::{PublicSourceRevision, SourceBinding};
use uuid::Uuid;

const FIXTURE_AUTHORITY: &str = "graphshell.reference-fixture";
const FIXTURE_DOMAIN: &str = "responsive-links";
const FIXTURE_RESOURCE: &str = "fixed-six-endpoints";
const FIXTURE_REVISION: &str = "responsive-links-fixture-v1";
const PREDICATE: &str = "urn:graphshell:responsive-link-fixture:related";
const COMPATIBLE_GROUP: &str = "urn:graphshell:responsive-link-fixture:compatible-a";
const INCOMPATIBLE_GROUP: &str = "urn:graphshell:responsive-link-fixture:incompatible";

/// The three reproducible analytic fields used by the visible fixture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FieldKind {
    #[default]
    Attractor,
    Flow,
    Vortex,
}

/// Six fixed displayed endpoints, each retained with its real graph identity.
#[derive(Clone, Debug, PartialEq)]
pub struct FixedEndpoint {
    pub id: Uuid,
    pub label: &'static str,
    pub position: (f32, f32),
}

/// Source references and temporary geometry for one visible path appearance.
#[derive(Clone, Debug)]
pub struct ResponsivePathAppearance<'a> {
    pub label: &'static str,
    pub compatibility: Option<&'static str>,
    pub key: &'a LinkKey,
    pub source: &'a LinkSource,
    pub buckets: Vec<&'a scenograph::link::LinkBucket>,
    pub assertion_ids: Vec<&'a str>,
    pub path: ResponsivePath,
    /// Maximum sample-point distance from the fixed endpoint chord, in fixture units.
    pub max_chord_deviation: f32,
    pub endpoints_preserved: bool,
    pub source_bucket_refs: Vec<SourceBucketRef>,
}

/// One raw configured field vector for the parent view to scale as a direction arrow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfiguredFieldSample {
    pub position: (f32, f32),
    pub vector: (f32, f32),
}

/// Explicit disclosure that spring allocation is not observable in this probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpringShareAvailability {
    Unavailable,
}

/// One immutable-input evaluation with its declared controls and observed paths.
#[derive(Clone, Debug)]
pub struct ResponsiveLinksSnapshot<'a> {
    pub field: FieldKind,
    pub field_response_enabled: bool,
    pub compatible_attraction_enabled: bool,
    pub settings: ResponsivePathConfig,
    pub field_strength: f32,
    pub field_description: &'static str,
    pub field_samples: Vec<ConfiguredFieldSample>,
    pub endpoints: &'a [FixedEndpoint],
    pub paths: Vec<ResponsivePathAppearance<'a>>,
    pub spring_share: SpringShareAvailability,
    pub spring_share_note: &'static str,
}

/// The fixed source graph and view-local response settings for the fixture.
#[derive(Clone)]
pub struct ResponsiveLinksFixture {
    graph: Graph,
    readings: Vec<LinkPairReading>,
    endpoints: Vec<FixedEndpoint>,
    positions: BTreeMap<String, (f32, f32)>,
    field: FieldKind,
    field_response_enabled: bool,
    compatible_attraction_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponsiveLinksFixtureError {
    GraphConstruction,
    MissingEndpoint,
    MissingFixtureAssertion,
    Adapter(mere::canvas::responsive_link_adapter::ResponsiveLinkAdapterError),
}

impl ResponsiveLinksFixture {
    pub fn new() -> Result<Self, ResponsiveLinksFixtureError> {
        let endpoint_specs = [
            (0x5101_u128, "A · start", (80.0, 150.0)),
            (0x5102_u128, "A · end", (680.0, 150.0)),
            (0x5103_u128, "B · start", (80.0, 210.0)),
            (0x5104_u128, "B · end", (680.0, 210.0)),
            (0x5105_u128, "C · start", (80.0, 400.0)),
            (0x5106_u128, "C · end", (680.0, 400.0)),
        ];
        let endpoints: Vec<_> = endpoint_specs
            .iter()
            .map(|(id, label, position)| FixedEndpoint {
                id: Uuid::from_u128(*id),
                label,
                position: *position,
            })
            .collect();
        let mut graph = Graph::new();
        let mut node_keys = Vec::with_capacity(endpoints.len());
        for endpoint in &endpoints {
            node_keys.push(add_node(
                &mut graph,
                Some(endpoint.id),
                format!("https://responsive-fixture.test/{}", endpoint.id),
                Default::default(),
            ));
        }
        graph
            .declare_predicate(PREDICATE, GraphStratum::Surface)
            .map_err(|_| ResponsiveLinksFixtureError::GraphConstruction)?;

        let pairs = [(0, 1, "path-a"), (2, 3, "path-b"), (4, 5, "path-c")];
        let mut readings = Vec::with_capacity(pairs.len());
        let source = LinkSource::Available {
            binding: SourceBinding {
                authority: FIXTURE_AUTHORITY.into(),
                domain: FIXTURE_DOMAIN.into(),
                resource: FIXTURE_RESOURCE.into(),
            },
            revision: LinkSourceRevision::Available {
                revision: PublicSourceRevision::new(FIXTURE_REVISION),
            },
        };
        for (index, &(from_index, to_index, handle)) in pairs.iter().enumerate() {
            let from = *node_keys
                .get(from_index)
                .ok_or(ResponsiveLinksFixtureError::MissingEndpoint)?;
            let to = *node_keys
                .get(to_index)
                .ok_or(ResponsiveLinksFixtureError::MissingEndpoint)?;
            graph
                .try_assert_persisted_semantic_statement(
                    from,
                    to,
                    SemanticStatement {
                        statement_id: format!("fixture:v1:assertion:{handle}"),
                        predicate: PREDICATE.into(),
                        recognized_sub_kind: None,
                        label: Some(format!("Reference link {}", index + 1)),
                        basis: mere::kernel::types::AssertionBasis::default(),
                        first_asserted_ms: Some(1_791_000_000_000 + index as u64),
                        withdrawn: None,
                        graph_scope: mere::kernel::types::GraphScope::Custom(
                            "urn:graphshell:responsive-link-fixture".into(),
                        ),
                        provenance_iri: Some(FIXTURE_AUTHORITY.into()),
                        asserted_at_ms: Some(1_791_000_000_000 + index as u64),
                    },
                )
                .map_err(|_| ResponsiveLinksFixtureError::GraphConstruction)?;
            let mut reading = mere::canvas::link_reading::read_link(
                &graph,
                endpoints[from_index].id,
                endpoints[to_index].id,
                source.clone(),
            )
            .map_err(|_| ResponsiveLinksFixtureError::GraphConstruction)?;
            reading.links.retain(|link| {
                link.key.stratum == LinkStratum::Surface
                    && link
                        .buckets
                        .iter()
                        .flat_map(|bucket| &bucket.assertions)
                        .any(|assertion| assertion.id == format!("fixture:v1:assertion:{handle}"))
            });
            if reading.links.len() != 1 {
                return Err(ResponsiveLinksFixtureError::MissingFixtureAssertion);
            }
            readings.push(reading);
        }
        let positions = endpoints
            .iter()
            .map(|endpoint| (endpoint.id.to_string(), endpoint.position))
            .collect();
        Ok(Self {
            graph,
            readings,
            endpoints,
            positions,
            field: FieldKind::Attractor,
            field_response_enabled: true,
            compatible_attraction_enabled: false,
        })
    }

    pub fn field(&self) -> FieldKind {
        self.field
    }

    pub fn field_response_enabled(&self) -> bool {
        self.field_response_enabled
    }

    pub fn compatible_attraction_enabled(&self) -> bool {
        self.compatible_attraction_enabled
    }

    pub fn set_field(&mut self, field: FieldKind) {
        self.field = field;
    }

    pub fn set_field_response_enabled(&mut self, enabled: bool) {
        self.field_response_enabled = enabled;
    }

    pub fn set_compatible_attraction_enabled(&mut self, enabled: bool) {
        self.compatible_attraction_enabled = enabled;
    }

    pub fn settings(&self) -> ResponsivePathConfig {
        ResponsivePathConfig {
            control_points: 17,
            iterations: 24,
            time: 0.0,
            field_response: self.field_response_enabled,
            compatible_attraction: self.compatible_attraction_enabled,
            field_gain: 0.08,
            compatibility_gain: 0.08,
            smoothing: 0.08,
            max_step: 4.0,
        }
    }

    pub fn source_readings(&self) -> &[LinkPairReading] {
        &self.readings
    }

    /// Evaluate once and return complete geometry plus reversible P1 provenance.
    pub fn evaluate(&self) -> Result<ResponsiveLinksSnapshot<'_>, ResponsiveLinksFixtureError> {
        let field = field_expression(self.field);
        let registry = FieldRegistry::default();
        let fields = [PathField {
            field: &field,
            strength: field_strength(self.field),
        }];
        let key_a = &self.readings[0].links[0].key;
        let key_b = &self.readings[1].links[0].key;
        let key_c = &self.readings[2].links[0].key;
        let compatibility = [
            AppearanceCompatibility {
                reading_index: 0,
                link_key: key_a,
                displayed_first: self.readings[0].links[0].buckets[0].displayed_from.as_str(),
                displayed_second: self.readings[0].links[0].buckets[0].displayed_to.as_str(),
                compatibility: COMPATIBLE_GROUP,
            },
            AppearanceCompatibility {
                reading_index: 1,
                link_key: key_b,
                displayed_first: self.readings[1].links[0].buckets[0].displayed_from.as_str(),
                displayed_second: self.readings[1].links[0].buckets[0].displayed_to.as_str(),
                compatibility: COMPATIBLE_GROUP,
            },
            AppearanceCompatibility {
                reading_index: 2,
                link_key: key_c,
                displayed_first: self.readings[2].links[0].buckets[0].displayed_from.as_str(),
                displayed_second: self.readings[2].links[0].buckets[0].displayed_to.as_str(),
                compatibility: INCOMPATIBLE_GROUP,
            },
        ];
        let batch = evaluate_link_appearances(
            &self.readings,
            &compatibility,
            |id| self.positions.get(id).copied(),
            &fields,
            &registry,
            self.settings(),
        )
        .map_err(ResponsiveLinksFixtureError::Adapter)?;
        let sample_positions = [
            (160.0, 150.0),
            (360.0, 150.0),
            (560.0, 210.0),
            (360.0, 400.0),
        ];
        let field_samples = sample_positions
            .into_iter()
            .map(|position| ConfiguredFieldSample {
                position,
                vector: eval_vector(&field, &registry, position.0, position.1, 0.0),
            })
            .collect();
        let paths = batch
            .paths
            .into_iter()
            .zip(batch.appearances.iter())
            .map(|(path, appearance)| {
                let reading = &self.readings[appearance.reading_index];
                let link = &reading.links[appearance.link_index];
                let buckets = appearance
                    .buckets
                    .iter()
                    .map(|bucket| &reading.links[bucket.link_index].buckets[bucket.bucket_index])
                    .collect::<Vec<_>>();
                let assertion_ids = buckets
                    .iter()
                    .flat_map(|bucket| {
                        bucket
                            .assertions
                            .iter()
                            .map(|assertion| assertion.id.as_str())
                    })
                    .collect();
                let (start, end) =
                    self.positions_for(appearance.displayed_start, appearance.displayed_end)?;
                let max_chord_deviation = max_chord_deviation(&path, start, end);
                let endpoints_preserved =
                    path.points.first() == Some(&start) && path.points.last() == Some(&end);
                let label = match appearance.reading_index {
                    0 => "Compatible path A",
                    1 => "Compatible path B",
                    _ => "Incompatible control path",
                };
                let compatibility = match appearance.compatibility.as_deref() {
                    Some(COMPATIBLE_GROUP) => Some("Declared compatible group"),
                    Some(INCOMPATIBLE_GROUP) => Some("Declared incompatible control"),
                    _ => None,
                };
                Ok(ResponsivePathAppearance {
                    label,
                    compatibility,
                    key: &link.key,
                    source: &reading.source,
                    buckets,
                    assertion_ids,
                    path,
                    max_chord_deviation,
                    endpoints_preserved,
                    source_bucket_refs: appearance.buckets.clone(),
                })
            })
            .collect::<Result<Vec<_>, ResponsiveLinksFixtureError>>()?;
        Ok(ResponsiveLinksSnapshot {
            field: self.field,
            field_response_enabled: self.field_response_enabled,
            compatible_attraction_enabled: self.compatible_attraction_enabled,
            settings: batch.settings,
            field_strength: field_strength(self.field),
            field_description: field_description(self.field),
            field_samples,
            endpoints: &self.endpoints,
            paths,
            spring_share: SpringShareAvailability::Unavailable,
            spring_share_note: "Spring allocation is unavailable: this fixture evaluates temporary path geometry and does not observe or apportion the graph controller's spring force.",
        })
    }

    fn positions_for(
        &self,
        start: &str,
        end: &str,
    ) -> Result<((f32, f32), (f32, f32)), ResponsiveLinksFixtureError> {
        Ok((
            self.positions
                .get(start)
                .copied()
                .ok_or(ResponsiveLinksFixtureError::MissingEndpoint)?,
            self.positions
                .get(end)
                .copied()
                .ok_or(ResponsiveLinksFixtureError::MissingEndpoint)?,
        ))
    }

    /// The fixture graph is exposed read-only for host source inspection.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }
}

fn field_expression(kind: FieldKind) -> VectorField {
    match kind {
        FieldKind::Attractor => {
            VectorField::gradient_of(ScalarField::gaussian_at(360.0, 80.0, 260.0))
        },
        FieldKind::Flow => VectorField::const_vec(0.0, -1.0),
        FieldKind::Vortex => VectorField::Perp(Box::new(VectorField::gradient_of(
            ScalarField::gaussian_at(360.0, 260.0, 260.0),
        ))),
    }
}

fn field_strength(kind: FieldKind) -> f32 {
    match kind {
        FieldKind::Attractor | FieldKind::Vortex => 10_000.0,
        FieldKind::Flow => 20.0,
    }
}

fn field_description(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Attractor => "Gaussian gradient centered at (360, 80), sigma 260.",
        FieldKind::Flow => "Uniform configured flow vector (0, -1).",
        FieldKind::Vortex => "Perpendicular Gaussian gradient centered at (360, 260), sigma 260.",
    }
}

fn max_chord_deviation(path: &ResponsivePath, start: (f32, f32), end: (f32, f32)) -> f32 {
    let chord = (end.0 - start.0, end.1 - start.1);
    let length = chord.0.hypot(chord.1);
    if length == 0.0 {
        return 0.0;
    }
    path.points
        .iter()
        .map(|point| {
            let relative = (point.0 - start.0, point.1 - start.1);
            (chord.0 * relative.1 - chord.1 * relative.0).abs() / length
        })
        .fold(0.0, f32::max)
}

#[cfg(test)]
#[path = "responsive_links_tests.rs"]
mod tests;
