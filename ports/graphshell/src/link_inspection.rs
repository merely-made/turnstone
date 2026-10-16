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
    CoverageLayer, CoverageLimit, CoverageNote, EdgeFamily, Graph, GraphStratum, RelationKey,
    SemanticStatement, SemanticSubKind,
};
use mere::kernel::types::GraphScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
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
    #[serde(default)]
    pub first_asserted_ms: Option<u64>,
    #[serde(default)]
    pub basis: scenograph::link::LinkAssertionBasis,
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
            first_asserted_ms: statement.first_asserted_ms,
            basis: scenograph::link::LinkAssertionBasis {
                note: statement.basis.note.clone(),
                method: statement.basis.method.as_ref().map(|method| {
                    scenograph::link::LinkAssertionMethod {
                        iri: method.iri.clone(),
                        version: method.version.clone(),
                    }
                }),
                evidence: statement.basis.evidence.clone(),
            },
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
    /// The graph-only caller cannot identify the owning public source, so that
    /// limitation is explicit until a host supplies the context.
    pub source: scenograph::link::LinkSource,
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

/// One exact assertion in the explicitly requested retained history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkHistoryEntry {
    pub displayed_from: Uuid,
    pub displayed_to: Uuid,
    pub owner: LinkOwner,
    pub assertion: scenograph::link::LinkAssertion,
}

/// Retained assertion lifecycle for a displayed pair in the supplied graph
/// snapshot. This is not a journal-prefix replay or a write capability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkHistoryInspection {
    pub first: AccessSummary,
    pub second: AccessSummary,
    pub source: scenograph::link::LinkSource,
    pub entries: Vec<LinkHistoryEntry>,
    pub notes: Vec<String>,
    pub coverage: CoverageNote,
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

fn decode<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, String> {
    serde_json::from_value(Value::String(value.to_owned())).map_err(|error| error.to_string())
}

fn portable_scope(scope: scenograph::link::LinkGraphScope) -> GraphScope {
    use scenograph::link::LinkGraphScope as Scope;
    match scope {
        Scope::Default => GraphScope::Default,
        Scope::Source => GraphScope::Source,
        Scope::User => GraphScope::User,
        Scope::Agent => GraphScope::Agent,
        Scope::Moot => GraphScope::Moot,
        Scope::Custom(value) => GraphScope::Custom(value),
    }
}

fn coverage_layer(layer: scenograph::link::LinkCoverageLayer) -> CoverageLayer {
    use scenograph::link::LinkCoverageLayer as Layer;
    match layer {
        Layer::Possession => CoverageLayer::Possession,
        Layer::Residency => CoverageLayer::Residency,
        Layer::Disclosure => CoverageLayer::Disclosure,
        Layer::Synchronization => CoverageLayer::Synchronization,
        Layer::Projection => CoverageLayer::Projection,
    }
}

fn kernel_coverage(coverage: scenograph::link::LinkCoverage) -> Result<CoverageNote, String> {
    let mut note = CoverageNote::default();
    for item in coverage.limits {
        let mut limit = CoverageLimit::new(coverage_layer(item.layer), item.reason);
        limit.resources = item
            .resources
            .into_iter()
            .map(|id| id.parse().map_err(|_| "Invalid coverage resource identity"))
            .collect::<Result<_, _>>()?;
        limit.surfaces = item
            .surfaces
            .into_iter()
            .map(|id| id.parse().map_err(|_| "Invalid coverage surface identity"))
            .collect::<Result<_, _>>()?;
        limit.count = item.count;
        note.push(limit);
    }
    Ok(note)
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
    inspect_link_with_source(
        graph,
        target,
        scenograph::link::LinkSource::Unavailable {
            reason: "Graphshell receives a kernel graph without its owning source authority or public revision".into(),
        },
    )
}

/// Inspect a displayed pair when the owning host can provide verified source
/// authority and revision context.
pub fn inspect_link_with_source(
    graph: &Graph,
    target: LinkTarget,
    source: scenograph::link::LinkSource,
) -> Result<LinkInspection, String> {
    let first = access(graph, target.first)?;
    let second = access(graph, target.second)?;
    let mut notes = Vec::new();
    for member in [target.first, target.second] {
        let key = graph
            .get_node_key_by_id(member)
            .expect("access resolved above");
        match graph.shown_resource_id(key) {
            Some(id) if graph.resource(id).is_some() => {},
            Some(id) => notes.push(format!("The recorded Resource {id} is not held here")),
            None => notes.push(format!(
                "No Resource association is recorded for access {member}"
            )),
        }
    }
    let reading = mere::canvas::link_reading::read_link_with_options(
        graph,
        target.first,
        target.second,
        source,
        mere::canvas::link_reading::LinkReadOptions {
            scope: mere::canvas::link_reading::LinkReadScope::Held,
            include_legacy_summaries: true,
        },
    )?;
    let source = reading.source.clone();
    let mut records = Vec::new();
    for link in reading.links {
        let stratum = match link.key.stratum {
            scenograph::link::LinkStratum::Surface => GraphStratum::Surface,
            scenograph::link::LinkStratum::Resource => GraphStratum::Resource,
        };
        for bucket in link.buckets {
            let displayed_from = bucket
                .displayed_from
                .parse()
                .map_err(|_| "Invalid displayed source identity")?;
            let displayed_to = bucket
                .displayed_to
                .parse()
                .map_err(|_| "Invalid displayed target identity")?;
            let source_from = bucket
                .source_from
                .id
                .parse()
                .map_err(|_| "Invalid source endpoint identity")?;
            let source_to = bucket
                .source_to
                .id
                .parse()
                .map_err(|_| "Invalid target endpoint identity")?;
            let mut statements = bucket
                .assertions
                .into_iter()
                .map(|statement| {
                    Ok(HeldStatement {
                        statement_id: statement.id,
                        predicate: statement.predicate,
                        recognized_sub_kind: statement
                            .recognized_kind
                            .as_deref()
                            .map(decode)
                            .transpose()?,
                        label: statement.label,
                        graph_scope: portable_scope(statement.scope),
                        provenance_iri: statement.asserter,
                        asserted_at_ms: statement.asserted_at_ms,
                        first_asserted_ms: statement.first_asserted_ms,
                        basis: statement.basis,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            statements.sort_by(|a, b| a.statement_id.cmp(&b.statement_id));
            let families = bucket
                .legacy_summaries
                .into_iter()
                .map(|family| {
                    Ok(HeldFamily {
                        family: decode(&family.family)?,
                        data: family.data,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            records.push(LinkRecord {
                displayed_from,
                displayed_to,
                owner: LinkOwner {
                    stratum,
                    from: SourceEndpoint {
                        id: source_from,
                        address: bucket.source_from.address,
                    },
                    to: SourceEndpoint {
                        id: source_to,
                        address: bucket.source_to.address,
                    },
                },
                statements,
                families,
            });
        }
    }
    // Stable content ordering preserves duplicate buckets without exposing local indexes.
    records
        .sort_by_cached_key(|record| serde_json::to_string(record).expect("inspection serializes"));
    notes.sort();
    notes.dedup();
    let coverage = kernel_coverage(reading.coverage)?;
    Ok(LinkInspection {
        first,
        second,
        source,
        records,
        notes,
        coverage,
    })
}

/// Explicit retained-history view for a displayed pair. The default
/// [`inspect_link`] remains held-only.
pub fn inspect_link_history(
    graph: &Graph,
    target: LinkTarget,
) -> Result<LinkHistoryInspection, String> {
    inspect_link_history_with_source(
        graph,
        target,
        scenograph::link::LinkSource::Unavailable {
            reason: "Graphshell receives a kernel graph without its owning source authority or public revision".into(),
        },
    )
}

/// Read held and withdrawn semantic assertions retained in this supplied
/// graph snapshot. This does not replay a session journal or authorize writes.
pub fn inspect_link_history_with_source(
    graph: &Graph,
    target: LinkTarget,
    source: scenograph::link::LinkSource,
) -> Result<LinkHistoryInspection, String> {
    let first = access(graph, target.first)?;
    let second = access(graph, target.second)?;
    let mut notes = Vec::new();
    for member in [target.first, target.second] {
        let key = graph
            .get_node_key_by_id(member)
            .expect("access resolved above");
        match graph.shown_resource_id(key) {
            Some(id) if graph.resource(id).is_some() => {},
            Some(id) => notes.push(format!("The recorded Resource {id} is not held here")),
            None => notes.push(format!(
                "No Resource association is recorded for access {member}"
            )),
        }
    }
    let reading =
        mere::canvas::link_reading::read_link_history(graph, target.first, target.second, source)?;
    let mut entries = Vec::new();
    for link in reading.links {
        let stratum = match link.key.stratum {
            scenograph::link::LinkStratum::Surface => GraphStratum::Surface,
            scenograph::link::LinkStratum::Resource => GraphStratum::Resource,
        };
        for bucket in link.buckets {
            let displayed_from = bucket
                .displayed_from
                .parse()
                .map_err(|_| "Invalid displayed source identity")?;
            let displayed_to = bucket
                .displayed_to
                .parse()
                .map_err(|_| "Invalid displayed target identity")?;
            let source_from = bucket
                .source_from
                .id
                .parse()
                .map_err(|_| "Invalid source endpoint identity")?;
            let source_to = bucket
                .source_to
                .id
                .parse()
                .map_err(|_| "Invalid target endpoint identity")?;
            for assertion in bucket.assertions {
                entries.push(LinkHistoryEntry {
                    displayed_from,
                    displayed_to,
                    owner: LinkOwner {
                        stratum,
                        from: SourceEndpoint {
                            id: source_from,
                            address: bucket.source_from.address.clone(),
                        },
                        to: SourceEndpoint {
                            id: source_to,
                            address: bucket.source_to.address.clone(),
                        },
                    },
                    assertion,
                });
            }
        }
    }
    entries.sort_by(|a, b| {
        let a_assertion = &a.assertion;
        let b_assertion = &b.assertion;
        let time_order = match (a_assertion.first_asserted_ms, b_assertion.first_asserted_ms) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        time_order
            .then_with(|| a_assertion.id.cmp(&b_assertion.id))
            .then_with(|| {
                let order = |stratum| match stratum {
                    GraphStratum::Resource => 0,
                    GraphStratum::Surface => 1,
                };
                order(a.owner.stratum).cmp(&order(b.owner.stratum))
            })
            .then_with(|| a.owner.from.id.cmp(&b.owner.from.id))
            .then_with(|| a.owner.to.id.cmp(&b.owner.to.id))
            .then_with(|| a.displayed_from.cmp(&b.displayed_from))
            .then_with(|| a.displayed_to.cmp(&b.displayed_to))
    });
    let coverage = kernel_coverage(reading.coverage)?;
    Ok(LinkHistoryInspection {
        first,
        second,
        source: reading.source,
        entries,
        notes,
        coverage,
    })
}

/// Re-resolve the exact held statement independently of the displayed pair.
/// Resource statements remain inspectable after an access changes its binding.
pub fn inspect_statement(graph: &Graph, statement_id: &str) -> Result<StatementInspection, String> {
    let (key, statement) = graph
        .find_held_semantic_statement(statement_id)
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
