// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Reserved immutable references in the existing graph assertion owner.
use mere::kernel::graph::{Graph, SemanticStatementSpec};
use mere::kernel::types::{AssertionBasis, AssertionMethod, GraphScope};
use std::collections::BTreeMap;

const PREDICATE: &str = "https://merely.computer/ns/research#property-record-v1";
const METHOD: &str = "https://merely.computer/ns/research#manual-property-v1";
const REFERENCE: &str = "urn:turnstone:sealed-property:v1:";
const RESULT_PREDICATE: &str = "https://merely.computer/ns/research#structured-result-v1";
const RESULT_METHOD: &str = "https://merely.computer/ns/research#structured-query-v1";
const RESULT_REFERENCE: &str = "urn:turnstone:sealed-result:v1:";

/// Reserve enough for a new self-loop and its assertion in both scope scans.
/// The kernel's local ID comprises at most three 16-digit hex values and two separators.
pub(crate) fn attachment_reservation(
    result: bool,
    author: &str,
    manifest: eidetic::ManifestId,
) -> (usize, usize) {
    let (predicate, reference) = if result {
        (RESULT_PREDICATE, RESULT_REFERENCE)
    } else {
        (PREDICATE, REFERENCE)
    };
    (
        4,
        50usize
            .saturating_add(predicate.len())
            .saturating_add(author.len())
            .saturating_add(reference.len())
            .saturating_add(manifest.to_string().len()),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GraphBinding {
    pub entity: uuid::Uuid,
    pub statement_id: String,
    pub manifest: eidetic::ManifestId,
    pub withdrawn: bool,
    pub author: String,
}

fn parse_reference(value: &str, prefix: &str) -> Result<eidetic::ManifestId, String> {
    let text = value
        .strip_prefix(prefix)
        .ok_or("Invalid property reference")?;
    let id: eidetic::ManifestId = serde_json::from_value(serde_json::Value::String(text.into()))
        .map_err(|_| "Invalid property manifest")?;
    if id.to_string() != text {
        return Err("Noncanonical property manifest".into());
    }
    Ok(id)
}

#[derive(Clone, Debug)]
pub(crate) struct Budget {
    remaining_work: usize,
    remaining_records: usize,
    remaining_bytes: usize,
}
impl Budget {
    pub(crate) fn new(settings: &eidetic::records::RecordSettings) -> Self {
        Self {
            remaining_work: settings.max_work,
            remaining_records: settings.max_records,
            remaining_bytes: settings.max_input_bytes,
        }
    }
    pub(crate) fn charge(
        &mut self,
        work: usize,
        records: usize,
        bytes: usize,
    ) -> Result<(), String> {
        self.remaining_work = self
            .remaining_work
            .checked_sub(work)
            .ok_or("Graph enumeration work limit exceeded")?;
        self.remaining_records = self
            .remaining_records
            .checked_sub(records)
            .ok_or("Graph assertion limit exceeded")?;
        self.remaining_bytes = self
            .remaining_bytes
            .checked_sub(bytes)
            .ok_or("Graph metadata byte limit exceeded")?;
        Ok(())
    }
    fn unlimited() -> Self {
        Self {
            remaining_work: usize::MAX,
            remaining_records: usize::MAX,
            remaining_bytes: usize::MAX,
        }
    }
}
fn bindings_for(
    graph: &Graph,
    include_withdrawn: bool,
    predicate: &str,
    method: &str,
    reference: &str,
    budget: &mut Budget,
) -> Result<Vec<GraphBinding>, String> {
    let mut found = BTreeMap::new();
    for (from, to, payload) in graph.resource_edges() {
        budget.charge(1, 0, 0)?;
        for statement in payload.assertions() {
            budget.charge(1, 0, 0)?;
            if statement.predicate != predicate {
                continue;
            }
            if statement.basis.evidence.len() != 1 {
                return Err("Malformed property assertion binding".into());
            }
            let bytes = statement
                .statement_id
                .len()
                .saturating_add(statement.predicate.len())
                .saturating_add(statement.provenance_iri.as_ref().map_or(0, String::len))
                .saturating_add(
                    statement
                        .basis
                        .evidence
                        .iter()
                        .fold(0usize, |total, value| total.saturating_add(value.len())),
                );
            // Bindings include saved history and unselected evidence. Their traversal and
            // metadata are bounded here; max_records applies to eligible query records.
            budget.charge(0, 0, bytes)?;
            if from.id() != to.id()
                || statement.graph_scope != GraphScope::Default
                || statement.recognized_sub_kind.is_some()
                || statement.basis.note.is_some()
                || statement.basis.evidence.len() != 1
                || statement.basis.method.as_ref()
                    != Some(&AssertionMethod {
                        iri: method.into(),
                        version: Some("1".into()),
                    })
            {
                return Err("Malformed property assertion binding".into());
            }
            let author = statement
                .provenance_iri
                .clone()
                .filter(|v| !v.trim().is_empty())
                .ok_or("Property author unavailable")?;
            let binding = GraphBinding {
                entity: from.id(),
                statement_id: statement.statement_id.clone(),
                manifest: parse_reference(&statement.basis.evidence[0], reference)?,
                withdrawn: statement.withdrawn.is_some(),
                author,
            };
            if binding.statement_id.trim().is_empty() {
                return Err("Property assertion identity missing".into());
            }
            if let Some(existing) = found.insert(binding.statement_id.clone(), binding.clone()) {
                if existing != binding {
                    return Err("Conflicting property assertion projection".into());
                }
            }
        }
    }
    for (_, _, _, payload) in graph.surface_relation_buckets() {
        budget.charge(1, 0, 0)?;
        for statement in payload.assertions() {
            budget.charge(1, 0, 0)?;
            if statement.predicate == predicate {
                return Err("Property assertion has surface placement".into());
            }
        }
    }
    let all: Vec<_> = found.into_values().collect();
    let mut manifests = std::collections::BTreeSet::new();
    for binding in &all {
        if !manifests.insert((binding.entity, binding.manifest.to_string())) {
            return Err("Ambiguous property manifest binding".into());
        }
    }
    Ok(all
        .into_iter()
        .filter(|binding| include_withdrawn || !binding.withdrawn)
        .collect())
}
pub(crate) fn bindings(
    graph: &Graph,
    include_withdrawn: bool,
) -> Result<Vec<GraphBinding>, String> {
    bindings_for(
        graph,
        include_withdrawn,
        PREDICATE,
        METHOD,
        REFERENCE,
        &mut Budget::unlimited(),
    )
}
pub(crate) fn result_bindings(graph: &Graph) -> Result<Vec<GraphBinding>, String> {
    bindings_for(
        graph,
        false,
        RESULT_PREDICATE,
        RESULT_METHOD,
        RESULT_REFERENCE,
        &mut Budget::unlimited(),
    )
}
pub(crate) fn bindings_limited(
    graph: &Graph,
    include_withdrawn: bool,
    budget: &mut Budget,
) -> Result<Vec<GraphBinding>, String> {
    bindings_for(
        graph,
        include_withdrawn,
        PREDICATE,
        METHOD,
        REFERENCE,
        budget,
    )
}
pub(crate) fn result_bindings_limited(
    graph: &Graph,
    budget: &mut Budget,
) -> Result<Vec<GraphBinding>, String> {
    bindings_for(
        graph,
        false,
        RESULT_PREDICATE,
        RESULT_METHOD,
        RESULT_REFERENCE,
        budget,
    )
}
pub(crate) fn assertion_for(
    graph: &Graph,
    binding: &GraphBinding,
) -> Result<serde_json::Value, String> {
    let mut matches = graph
        .resource_edges()
        .filter(|(from, to, _)| from.id() == binding.entity && to.id() == binding.entity)
        .flat_map(|(_, _, payload)| payload.assertions())
        .filter(|statement| statement.statement_id == binding.statement_id);
    let statement = matches
        .next()
        .ok_or("Attached property assertion unavailable")?;
    if matches.next().is_some() || statement.recognized_sub_kind.is_some() {
        return Err("Attached property assertion is ambiguous or unsupported".into());
    }
    let persisted = mere::kernel::persistence_edge::PersistedSemanticStatement {
        statement_id: statement.statement_id.clone(),
        predicate: statement.predicate.clone(),
        recognized_sub_kind: None,
        label: statement.label.clone(),
        basis: statement.basis.clone(),
        graph_scope: statement.graph_scope.clone(),
        provenance_iri: statement.provenance_iri.clone(),
        asserted_at_ms: statement.asserted_at_ms,
        first_asserted_ms: statement.first_asserted_ms,
        withdrawn: statement.withdrawn.clone(),
    };
    serde_json::to_value(persisted)
        .map_err(|error| format!("Property assertion encoding failed: {error}"))
}
fn attach_for(
    graph: &mut Graph,
    entity: uuid::Uuid,
    manifest: eidetic::ManifestId,
    predicate: &str,
    method: &str,
    reference: &str,
) -> Result<GraphBinding, String> {
    if graph.resource(entity).is_none() {
        return Err("Property entity unavailable".into());
    }
    let author = graph.write_author().asserter_iri();
    if let Some(existing) = bindings_for(
        graph,
        true,
        predicate,
        method,
        reference,
        &mut Budget::unlimited(),
    )?
    .into_iter()
    .find(|b| b.entity == entity && b.manifest == manifest)
    {
        if existing.withdrawn || existing.author != author {
            return Err("Property binding is withdrawn or owned by another author".into());
        }
        return Ok(existing);
    }
    let spec = SemanticStatementSpec {
        predicate: predicate.into(),
        recognized_sub_kind: None,
        label: None,
        basis: AssertionBasis {
            note: None,
            method: Some(AssertionMethod {
                iri: method.into(),
                version: Some("1".into()),
            }),
            evidence: vec![format!("{reference}{manifest}")],
        },
        graph_scope: GraphScope::Default,
        provenance_iri: Some(author.clone()),
        asserted_at_ms: None,
    };
    let (_, asserted) = graph
        .try_assert_semantic_statement_by_resource_ids(entity, entity, spec)
        .map_err(|e| format!("Property assertion refused: {e:?}"))?;
    let result = bindings_for(
        graph,
        true,
        predicate,
        method,
        reference,
        &mut Budget::unlimited(),
    )?
    .into_iter()
    .find(|b| b.statement_id == asserted.statement_id)
    .ok_or("Property assertion missing after write")?;
    if result.entity != entity
        || result.manifest != manifest
        || result.author != author
        || result.withdrawn
    {
        return Err("Property assertion differs after write".into());
    }
    Ok(result)
}

pub(crate) fn attach(
    graph: &mut Graph,
    entity: uuid::Uuid,
    manifest: eidetic::ManifestId,
) -> Result<GraphBinding, String> {
    attach_for(graph, entity, manifest, PREDICATE, METHOD, REFERENCE)
}
pub(crate) fn attach_result(
    graph: &mut Graph,
    entity: uuid::Uuid,
    manifest: eidetic::ManifestId,
) -> Result<GraphBinding, String> {
    attach_for(
        graph,
        entity,
        manifest,
        RESULT_PREDICATE,
        RESULT_METHOD,
        RESULT_REFERENCE,
    )
}
pub(crate) fn withdraw_owned(graph: &mut Graph, statement_id: &str) -> Result<bool, String> {
    let binding = bindings(graph, true)?
        .into_iter()
        .find(|b| b.statement_id == statement_id)
        .ok_or("Property assertion unavailable")?;
    if binding.author != graph.write_author().asserter_iri() {
        return Err("Property assertion belongs to another author".into());
    }
    if binding.withdrawn {
        return Ok(false);
    }
    if !graph.withdraw_semantic_assertion(statement_id, None, None) {
        return Err("Property withdrawal refused".into());
    }
    Ok(true)
}

#[cfg(test)]
#[path = "research_record_graph_tests.rs"]
mod tests;

/// Restore one receipt-carried assertion by adding it to the current bucket.
/// The receipt must contain the complete canonical statement, never a stale bucket.
pub(crate) fn restore(
    graph: &mut Graph,
    expected: &GraphBinding,
    assertion: &serde_json::Value,
    result: bool,
    settings: &eidetic::records::RecordSettings,
) -> Result<(), String> {
    use mere::kernel::graph::apply::{GraphDelta, GraphDeltaResult, apply_graph_delta};
    use mere::kernel::persistence_edge::{PersistedEdge, PersistedSemanticStatement};
    let statement: PersistedSemanticStatement =
        serde_json::from_value(assertion.clone()).map_err(|_| "Recovery assertion invalid")?;
    let (predicate, method, reference) = if result {
        (RESULT_PREDICATE, RESULT_METHOD, RESULT_REFERENCE)
    } else {
        (PREDICATE, METHOD, REFERENCE)
    };
    if serde_json::to_value(&statement).map_err(|e| e.to_string())? != *assertion
        || statement.statement_id != expected.statement_id
        || statement.predicate != predicate
        || statement.graph_scope != GraphScope::Default
        || statement.recognized_sub_kind.is_some()
        || statement.withdrawn.is_some()
        || statement.label.is_some()
        || statement.basis.note.is_some()
        || statement.basis.method.as_ref()
            != Some(&AssertionMethod {
                iri: method.into(),
                version: Some("1".into()),
            })
        || statement.provenance_iri.as_deref() != Some(expected.author.as_str())
        || statement.basis.evidence.len() != 1
        || parse_reference(&statement.basis.evidence[0], reference)? != expected.manifest
        || graph.resource(expected.entity).is_none()
    {
        return Err("Recovery assertion does not match its owner and receipt".into());
    }
    let current = bindings_for(
        graph,
        true,
        predicate,
        method,
        reference,
        &mut Budget::new(settings),
    )?;
    if let Some(existing) = current
        .iter()
        .find(|item| item.statement_id == expected.statement_id)
    {
        if existing != expected || assertion_for(graph, existing)? != *assertion {
            return Err("Recovery assertion collides with current graph".into());
        }
        return Ok(());
    }
    if graph.find_assertion(&expected.statement_id).is_some()
        || current
            .iter()
            .any(|item| item.entity == expected.entity && item.manifest == expected.manifest)
    {
        return Err("Recovery assertion conflicts with current graph".into());
    }
    let endpoint = expected.entity.to_string();
    let mut edges = graph
        .persisted_resource_edges_between_bounded(
            expected.entity,
            expected.entity,
            settings.max_work,
            settings.max_input_bytes,
        )
        .map_err(|error| format!("Recovery target bucket refused: {error:?}"))?;
    edges.push(PersistedEdge {
        from_node_id: endpoint.clone(),
        to_node_id: endpoint,
        families: vec![],
        semantic: None,
        traversal: None,
        containment: None,
        arrangement: None,
        imported: None,
        provenance: None,
        assertions: Some(vec![statement]),
    });
    if edges.len() > settings.max_work {
        return Err("Recovery target bucket work limit exceeded".into());
    }
    struct ByteLimit(usize);
    impl std::io::Write for ByteLimit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_sub(bytes.len())
                .ok_or_else(|| std::io::Error::other("recovery byte limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(&mut ByteLimit(settings.max_input_bytes), &edges)
        .map_err(|_| "Recovery target bucket byte limit exceeded")?;
    if !matches!(
        apply_graph_delta(
            graph,
            GraphDelta::ReplaySetResourceEdgesByIds {
                from_resource_id: expected.entity,
                to_resource_id: expected.entity,
                edges
            }
        ),
        GraphDeltaResult::Applied
    ) {
        return Err("Recovery assertion replay refused".into());
    }
    let actual = bindings_for(
        graph,
        true,
        predicate,
        method,
        reference,
        &mut Budget::new(settings),
    )?
    .into_iter()
    .find(|item| item.statement_id == expected.statement_id)
    .ok_or("Recovery assertion was not installed")?;
    if actual != *expected || assertion_for(graph, &actual)? != *assertion {
        return Err("Recovery assertion changed during replay".into());
    }
    Ok(())
}
