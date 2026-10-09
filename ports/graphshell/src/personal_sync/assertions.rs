// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{EdgeAssertion, Graph, PersonalGraphError, PersonalGraphEvent, RelationSelector};
use mere::kernel::graph::{EdgeFamily, SemanticStatement, SemanticSubKind, predicate_iri};
use mere::kernel::types::GraphScope;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use stickleback::Reject;
use uuid::Uuid;

pub(super) fn prepare_event(event: &mut PersonalGraphEvent) {
    if let PersonalGraphEvent::AssertRelation {
        assertion: EdgeAssertion::Semantic { sub_kind, .. },
        statement_id,
        asserted_at_ms,
        ..
    } = event
    {
        if statement_id.is_none() {
            *statement_id = Some(
                SemanticStatement::new(
                    predicate_iri(*sub_kind).into(),
                    Some(*sub_kind),
                    None,
                    GraphScope::Default,
                    None,
                    None,
                )
                .statement_id,
            );
        }
        asserted_at_ms.get_or_insert_with(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        });
    }
}

pub(super) fn validate_event(event: &PersonalGraphEvent) -> Result<(), Reject> {
    let id = match event {
        PersonalGraphEvent::AssertRelation {
            assertion,
            statement_id,
            asserted_at_ms,
            ..
        } => {
            if statement_id.is_some() != asserted_at_ms.is_some()
                || (!matches!(assertion, EdgeAssertion::Semantic { .. }) && statement_id.is_some())
            {
                return Err(Reject::new(
                    "invalid-assertion-metadata",
                    "semantic assertion id and time must be supplied together",
                ));
            }
            statement_id.as_deref()
        },
        PersonalGraphEvent::RetractAssertion { statement_id, .. } => Some(statement_id.as_str()),
        _ => None,
    };
    if id.is_some_and(|id| id.is_empty() || id.chars().any(|c| c.is_whitespace() || c.is_control()))
    {
        return Err(Reject::new(
            "invalid-assertion-id",
            "assertion id must be nonempty and contain no whitespace or control characters",
        ));
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct IdentityIndex(BTreeMap<String, (Uuid, Uuid, SemanticSubKind, String)>);

impl IdentityIndex {
    pub(super) fn observe(
        &mut self,
        event: &PersonalGraphEvent,
        asserter: &str,
        fallback: &str,
    ) -> Result<(), PersonalGraphError> {
        if let PersonalGraphEvent::AssertRelation {
            from,
            to,
            assertion: EdgeAssertion::Semantic { sub_kind, .. },
            statement_id,
            ..
        } = event
        {
            let id = statement_id.as_deref().unwrap_or(fallback);
            let identity = (*from, *to, *sub_kind, asserter.to_owned());
            if let Some(existing) = self.0.get(id) {
                if existing != &identity {
                    return Err(PersonalGraphError::Excluded(format!(
                        "assertion id {id} was reused for another claim"
                    )));
                }
            } else {
                self.0.insert(id.to_owned(), identity);
            }
        }
        Ok(())
    }
}

pub(super) fn assert_statement(
    graph: &mut Graph,
    from: Uuid,
    to: Uuid,
    kind: SemanticSubKind,
    label: Option<String>,
    asserter: &str,
    id: &str,
    at_ms: Option<u64>,
) {
    if let Some((from, to)) = graph
        .get_node_key_by_id(from)
        .zip(graph.get_node_key_by_id(to))
    {
        graph.assert_persisted_semantic_statement(
            from,
            to,
            SemanticStatement {
                statement_id: id.to_owned(),
                predicate: predicate_iri(kind).into(),
                recognized_sub_kind: Some(kind),
                label,
                graph_scope: GraphScope::Default,
                provenance_iri: Some(asserter.to_owned()),
                asserted_at_ms: at_ms,
            },
        );
    }
}

pub(super) fn retract_legacy(
    graph: &mut Graph,
    from: Uuid,
    to: Uuid,
    selector: RelationSelector,
    asserter: &str,
) -> bool {
    let kind = match selector {
        RelationSelector::Semantic(kind) => Some(kind),
        RelationSelector::Family(EdgeFamily::Semantic) => None,
        _ => return false,
    };
    if let Some((from, to)) = graph
        .get_node_key_by_id(from)
        .zip(graph.get_node_key_by_id(to))
    {
        let ids = graph
            .projected_relations_between(from, to)
            .flat_map(|(_, edge)| {
                edge.semantic_statements()
                    .iter()
                    .filter(|s| {
                        s.provenance_iri.as_deref() == Some(asserter)
                            && kind.is_none_or(|kind| s.recognized_sub_kind == Some(kind))
                    })
                    .map(|s| s.statement_id.clone())
            })
            .collect::<Vec<_>>();
        for id in ids {
            graph.retract_semantic_statement(from, to, &id);
        }
    }
    true
}
