// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Read-only expansion of a displayed access pair into held source records.
//! Targets use access UUIDs or statement handles, never local edge indexes.
//! Rule-derived display marks need their own owner adapter; these are held facts.
use std::collections::BTreeSet;

use mere::kernel::graph::{
    CoverageNote, EdgeFamily, EdgePayload, Graph, GraphStratum, RelationKey, SemanticStatement,
    SemanticSubKind,
};
use mere::kernel::types::GraphScope;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::access_inspection::AccessSummary;

/// An undirected displayed bundle. Its records retain their own direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkTarget {
    pub first: Uuid,
    pub second: Uuid,
}

impl LinkTarget {
    pub fn new(a: Uuid, b: Uuid) -> Self {
        Self {
            first: a.min(b),
            second: a.max(b),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEndpoint {
    pub id: Uuid,
    /// Missing held endpoint labels are not filled from an equal address.
    pub address: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkOwner {
    pub stratum: GraphStratum,
    pub from: SourceEndpoint,
    pub to: SourceEndpoint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldStatement {
    pub statement_id: String,
    pub predicate: String,
    pub recognized_sub_kind: Option<SemanticSubKind>,
    pub label: Option<String>,
    pub graph_scope: GraphScope,
    pub provenance_iri: Option<String>,
    pub asserted_at_ms: Option<u64>,
}

impl From<&SemanticStatement> for HeldStatement {
    fn from(statement: &SemanticStatement) -> Self {
        Self {
            statement_id: statement.statement_id.clone(),
            predicate: statement.predicate.clone(),
            recognized_sub_kind: statement.recognized_sub_kind,
            label: statement.label.clone(),
            graph_scope: statement.graph_scope.clone(),
            provenance_iri: statement.provenance_iri.clone(),
            asserted_at_ms: statement.asserted_at_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldFamily {
    pub family: EdgeFamily,
    /// Actual typed sidecar fields; no manufactured assertion identity.
    /// Semantic data here is only the compatibility grouping summary.
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub displayed_from: Uuid,
    pub displayed_to: Uuid,
    pub owner: LinkOwner,
    pub statements: Vec<HeldStatement>,
    pub families: Vec<HeldFamily>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkInspection {
    pub first: AccessSummary,
    pub second: AccessSummary,
    /// One entry per contributing edge bucket per displayed direction, including parallels.
    /// Ordinals are presentation only, not reusable source handles.
    pub records: Vec<LinkRecord>,
    pub notes: Vec<String>,
    pub coverage: CoverageNote,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatementInspection {
    pub owner: LinkOwner,
    pub statement: HeldStatement,
}

fn access(graph: &Graph, member: Uuid) -> Result<AccessSummary, String> {
    let (_, node) = graph
        .get_node_by_id(member)
        .ok_or_else(|| format!("The inspected access {member} is no longer held here"))?;
    Ok(AccessSummary {
        member,
        title: node.title.clone(),
        address: node.url().to_owned(),
    })
}

fn owner(graph: &Graph, stratum: GraphStratum, from: Uuid, to: Uuid) -> LinkOwner {
    let endpoint = |id| SourceEndpoint {
        id,
        address: match stratum {
            GraphStratum::Surface => graph
                .get_node_by_id(id)
                .map(|(_, node)| node.url().to_owned()),
            GraphStratum::Resource => graph
                .resource(id)
                .map(|node| node.canonical_iri().to_owned()),
        },
    };
    LinkOwner {
        stratum,
        from: endpoint(from),
        to: endpoint(to),
    }
}

fn families(payload: &EdgePayload) -> Vec<HeldFamily> {
    let mut held = Vec::new();
    let mut push = |family, data| held.push(HeldFamily { family, data });
    if let Some(data) = &payload.semantic {
        push(
            EdgeFamily::Semantic,
            json!({
                "sub_kinds": data.sub_kinds, "label": data.label, "predicate": data.predicate
            }),
        );
    }
    if let Some(data) = &payload.traversal {
        push(
            EdgeFamily::Traversal,
            json!({
                "traversals": data.traversals.iter().map(|event| json!({
                    "timestamp_ms": event.timestamp_ms, "trigger": event.trigger
                })).collect::<Vec<_>>(),
                "metrics": {
                    "total_navigations": data.metrics.total_navigations,
                    "forward_navigations": data.metrics.forward_navigations,
                    "backward_navigations": data.metrics.backward_navigations,
                    "last_navigated_at": data.metrics.last_navigated_at
                }
            }),
        );
    }
    if let Some(data) = &payload.arrangement {
        push(
            EdgeFamily::Arrangement,
            json!({ "sub_kinds": data.sub_kinds }),
        );
    }
    if let Some(data) = &payload.containment {
        push(
            EdgeFamily::Containment,
            json!({ "sub_kinds": data.sub_kinds }),
        );
    }
    if let Some(data) = &payload.imported {
        push(EdgeFamily::Imported, json!({ "sub_kinds": data.sub_kinds }));
    }
    if let Some(data) = &payload.provenance {
        push(
            EdgeFamily::Provenance,
            json!({ "sub_kinds": data.sub_kinds }),
        );
    }
    held
}

/// Held adjacent accesses only, deterministic and deduplicated by member UUID.
/// Self-links yield this same access once. Equal URLs are never merged.
pub fn linked_accesses(graph: &Graph, member: Uuid) -> Result<Vec<AccessSummary>, String> {
    let (key, _) = graph
        .get_node_by_id(member)
        .ok_or_else(|| format!("The inspected access {member} is no longer held here"))?;
    let neighbors: BTreeSet<_> = graph
        .projected_outgoing_relations(key)
        .chain(graph.projected_incoming_relations(key))
        .filter_map(|(neighbor, _, _)| graph.get_node(neighbor).map(|node| node.id))
        .collect();
    neighbors.into_iter().map(|id| access(graph, id)).collect()
}

pub fn inspect_link(graph: &Graph, target: LinkTarget) -> Result<LinkInspection, String> {
    let first = access(graph, target.first)?;
    let second = access(graph, target.second)?;
    let mut records = Vec::new();
    let mut notes = Vec::new();
    let mut resources = BTreeSet::new();
    let mut surfaces = BTreeSet::from([target.first, target.second]);
    for member in &surfaces.clone() {
        let key = graph
            .get_node_key_by_id(*member)
            .expect("access resolved above");
        match graph.shown_resource_id(key) {
            Some(id) => {
                resources.insert(id);
                surfaces.extend(graph.surface_ids_showing_resource(id));
                if graph.resource(id).is_none() {
                    notes.push(format!("The recorded Resource {id} is not held here"));
                }
            },
            None => notes.push(format!(
                "No Resource association is recorded for access {member}"
            )),
        }
    }
    let mut pairs = vec![(target.first, target.second)];
    if target.first != target.second {
        pairs.push((target.second, target.first));
    }
    for (from, to) in pairs {
        let from_key = graph
            .get_node_key_by_id(from)
            .expect("access resolved above");
        let to_key = graph.get_node_key_by_id(to).expect("access resolved above");
        for (key, payload) in graph.projected_relations_between(from_key, to_key) {
            let source = match key {
                RelationKey::Surface(_) => owner(graph, GraphStratum::Surface, from, to),
                RelationKey::Resource(_) => owner(
                    graph,
                    GraphStratum::Resource,
                    graph
                        .shown_resource_id(from_key)
                        .expect("projected Resource source"),
                    graph
                        .shown_resource_id(to_key)
                        .expect("projected Resource target"),
                ),
            };
            let mut statements: Vec<_> = payload
                .semantic_statements()
                .iter()
                .map(HeldStatement::from)
                .collect();
            statements.sort_by(|a, b| a.statement_id.cmp(&b.statement_id));
            records.push(LinkRecord {
                displayed_from: from,
                displayed_to: to,
                owner: source,
                statements,
                families: families(payload),
            });
        }
    }
    // Stable content ordering preserves duplicate buckets without exposing local indexes.
    records
        .sort_by_cached_key(|record| serde_json::to_string(record).expect("inspection serializes"));
    notes.sort();
    notes.dedup();
    let mut coverage = graph.coverage_note();
    coverage.limits.retain(|limit| {
        (limit.resources.is_empty() && limit.surfaces.is_empty())
            || limit.resources.iter().any(|id| resources.contains(id))
            || limit.surfaces.iter().any(|id| surfaces.contains(id))
    });
    Ok(LinkInspection {
        first,
        second,
        records,
        notes,
        coverage,
    })
}

/// Re-resolve the exact held statement independently of the displayed pair.
/// Resource statements remain inspectable after an access changes its binding.
pub fn inspect_statement(graph: &Graph, statement_id: &str) -> Result<StatementInspection, String> {
    let (key, statement) = graph
        .find_semantic_statement(statement_id)
        .ok_or_else(|| format!("Assertion {statement_id} is no longer held here"))?;
    let source = match key {
        RelationKey::Resource(key) => {
            let (_, from, to, _) = graph
                .resource_relations()
                .find(|(held, _, _, _)| *held == key)
                .ok_or("The assertion's Resource endpoints are unavailable")?;
            owner(graph, GraphStratum::Resource, from, to)
        },
        RelationKey::Surface(_) => {
            // Surface rows precede lifted Resource rows; the matching statement
            // necessarily supplies a row. This does not infer current placement.
            let (_, row) = graph
                .projected_relations()
                .find(|(held, _)| *held == key)
                .ok_or("The assertion's access endpoints are unavailable")?;
            owner(
                graph,
                GraphStratum::Surface,
                graph
                    .get_node(row.from)
                    .ok_or("The assertion's source access is unavailable")?
                    .id,
                graph
                    .get_node(row.to)
                    .ok_or("The assertion's target access is unavailable")?
                    .id,
            )
        },
    };
    Ok(StatementInspection {
        owner: source,
        statement: statement.into(),
    })
}

#[cfg(test)]
#[path = "link_inspection/tests.rs"]
mod tests;
