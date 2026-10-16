// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{EdgeAssertion, Graph, PersonalGraphError, PersonalGraphEvent, RelationSelector};
use mere::kernel::graph::{EdgeFamily, SemanticStatement, SemanticSubKind, predicate_iri};
use mere::kernel::types::{AssertionBasis, GraphScope};
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
    if let PersonalGraphEvent::RetractAssertion {
        withdrawn_at_ms, ..
    } = event
    {
        withdrawn_at_ms.get_or_insert_with(|| {
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

#[derive(Clone)]
struct AssertionOwner {
    from: Uuid,
    to: Uuid,
    sub_kind: SemanticSubKind,
    asserter: String,
    operation: [u8; 32],
    method_iri: Option<String>,
    evidence_key: Vec<String>,
}

#[derive(Default)]
pub(super) struct IdentityIndex(BTreeMap<String, AssertionOwner>);

impl IdentityIndex {
    pub(super) fn observe(
        &mut self,
        event: &PersonalGraphEvent,
        asserter: &str,
        fallback: &str,
        operation: [u8; 32],
        causally_known: &std::collections::HashSet<[u8; 32]>,
    ) -> Result<(), PersonalGraphError> {
        match event {
            PersonalGraphEvent::AssertRelation {
                from,
                to,
                assertion: EdgeAssertion::Semantic { sub_kind, .. },
                statement_id,
                basis,
                ..
            } => {
                let id = statement_id.as_deref().unwrap_or(fallback);
                let basis = basis.clone().unwrap_or_default();
                let mut evidence_key = basis.evidence.clone();
                evidence_key.sort();
                evidence_key.dedup();
                let identity = AssertionOwner {
                    from: *from,
                    to: *to,
                    sub_kind: *sub_kind,
                    asserter: asserter.to_owned(),
                    operation,
                    method_iri: basis.method.map(|method| method.iri),
                    evidence_key,
                };
                if let Some(existing) = self.0.get(id) {
                    if existing.from != identity.from
                        || existing.to != identity.to
                        || existing.sub_kind != identity.sub_kind
                        || existing.asserter != identity.asserter
                        || existing.method_iri != identity.method_iri
                        || existing.evidence_key != identity.evidence_key
                    {
                        return Err(PersonalGraphError::Excluded(format!(
                            "assertion id {id} was reused for another claim"
                        )));
                    }
                } else {
                    self.0.insert(id.to_owned(), identity);
                }
            },
            PersonalGraphEvent::RetractAssertion {
                from,
                to,
                statement_id,
                ..
            } => match self.0.get(statement_id) {
                Some(owner)
                    if owner.from == *from
                        && owner.to == *to
                        && owner.asserter == asserter
                        && (owner.operation == operation
                            || causally_known.contains(&owner.operation)) => {},
                Some(_) => {
                    return Err(PersonalGraphError::Excluded(format!(
                        "retraction {statement_id} does not match its assertion pair and author"
                    )));
                },
                None => {
                    return Err(PersonalGraphError::Excluded(format!(
                        "retraction {statement_id} precedes or lacks its assertion"
                    )));
                },
            },
            _ => {},
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
    basis: Option<AssertionBasis>,
) {
    if let Some((from, to)) = graph
        .get_node_key_by_id(from)
        .zip(graph.get_node_key_by_id(to))
    {
        let basis = basis.unwrap_or_else(|| AssertionBasis {
            note: label.clone(),
            ..Default::default()
        });
        let label = basis.note.clone();
        graph.assert_persisted_semantic_statement(
            from,
            to,
            SemanticStatement {
                statement_id: id.to_owned(),
                predicate: predicate_iri(kind).into(),
                recognized_sub_kind: Some(kind),
                label,
                basis,
                graph_scope: GraphScope::Default,
                provenance_iri: Some(asserter.to_owned()),
                asserted_at_ms: at_ms,
                first_asserted_ms: at_ms,
                withdrawn: None,
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
                edge.held_semantic_statements()
                    .filter(|s| {
                        s.provenance_iri.as_deref() == Some(asserter)
                            && kind.is_none_or(|kind| s.recognized_sub_kind == Some(kind))
                    })
                    .map(|s| s.statement_id.clone())
            })
            .collect::<Vec<_>>();
        for id in ids {
            graph.apply_imported_semantic_withdrawal_for_pair(
                from,
                to,
                &id,
                None,
                Some(asserter.to_owned()),
            );
        }
    }
    true
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn assertion_retraction_requires_matching_pair_author_and_causal_knowledge() {
        let id = "urn:assertion:one";
        let author = "urn:agent:alice";
        let operation = [1; 32];
        let assert = PersonalGraphEvent::AssertRelation {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
            statement_id: Some(id.into()),
            asserted_at_ms: Some(10),
            basis: None,
        };
        let mut index = IdentityIndex::default();
        let known = HashSet::from([operation]);
        index
            .observe(&assert, author, "fallback", operation, &known)
            .unwrap();

        let retract = |from, to| PersonalGraphEvent::RetractAssertion {
            from,
            to,
            statement_id: id.into(),
            withdrawn_at_ms: Some(11),
        };
        assert!(
            index
                .observe(
                    &retract(Uuid::from_u128(1), Uuid::from_u128(2)),
                    "urn:agent:bob",
                    "",
                    [2; 32],
                    &known
                )
                .is_err()
        );
        assert!(
            index
                .observe(
                    &retract(Uuid::from_u128(3), Uuid::from_u128(2)),
                    author,
                    "",
                    [2; 32],
                    &known
                )
                .is_err()
        );
        assert!(
            index
                .observe(
                    &retract(Uuid::from_u128(1), Uuid::from_u128(2)),
                    author,
                    "",
                    [2; 32],
                    &HashSet::new()
                )
                .is_err()
        );
        assert!(
            index
                .observe(
                    &retract(Uuid::from_u128(1), Uuid::from_u128(2)),
                    author,
                    "",
                    [2; 32],
                    &HashSet::from([[1; 32]])
                )
                .is_ok()
        );
        assert!(
            index
                .observe(
                    &retract(Uuid::from_u128(1), Uuid::from_u128(2)),
                    author,
                    "",
                    [2; 32],
                    &known
                )
                .is_ok()
        );
    }

    #[test]
    fn same_operation_retraction_requires_prior_assertion_event_order() {
        let id = "urn:assertion:ordered";
        let author = "urn:agent:alice";
        let operation = [7; 32];
        let assertion = PersonalGraphEvent::AssertRelation {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
            statement_id: Some(id.into()),
            asserted_at_ms: Some(10),
            basis: None,
        };
        let withdrawal = PersonalGraphEvent::RetractAssertion {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
            statement_id: id.into(),
            withdrawn_at_ms: Some(11),
        };
        let mut ordered = IdentityIndex::default();
        ordered
            .observe(&assertion, author, "fallback", operation, &HashSet::new())
            .unwrap();
        ordered
            .observe(&withdrawal, author, "fallback", operation, &HashSet::new())
            .unwrap();

        let mut reversed = IdentityIndex::default();
        assert!(
            reversed
                .observe(&withdrawal, author, "fallback", operation, &HashSet::new())
                .is_err()
        );
        reversed
            .observe(&assertion, author, "fallback", operation, &HashSet::new())
            .unwrap();
    }

    #[test]
    fn reason_identity_ignores_evidence_order_duplicates_and_method_version() {
        let event = |evidence: Vec<String>, version: &str| PersonalGraphEvent::AssertRelation {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
            assertion: EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: None,
                decay_progress: None,
            },
            statement_id: Some("urn:assertion:same-reason".into()),
            asserted_at_ms: Some(10),
            basis: Some(AssertionBasis {
                method: Some(mere::kernel::types::AssertionMethod {
                    iri: "urn:method:extractor".into(),
                    version: Some(version.into()),
                }),
                evidence,
                ..Default::default()
            }),
        };
        let mut index = IdentityIndex::default();
        let known = HashSet::new();
        index
            .observe(
                &event(vec!["urn:evidence:a".into(), "urn:evidence:b".into()], "v1"),
                "urn:agent:alice",
                "fallback",
                [1; 32],
                &known,
            )
            .unwrap();
        index
            .observe(
                &event(
                    vec![
                        "urn:evidence:b".into(),
                        "urn:evidence:a".into(),
                        "urn:evidence:a".into(),
                    ],
                    "v2",
                ),
                "urn:agent:alice",
                "fallback",
                [2; 32],
                &known,
            )
            .unwrap();
    }

    #[test]
    fn authored_retraction_time_is_stamped_but_old_event_time_remains_absent() {
        let mut authored = PersonalGraphEvent::RetractAssertion {
            from: Uuid::from_u128(1),
            to: Uuid::from_u128(2),
            statement_id: "urn:assertion:old".into(),
            withdrawn_at_ms: None,
        };
        prepare_event(&mut authored);
        assert!(matches!(
            authored,
            PersonalGraphEvent::RetractAssertion {
                withdrawn_at_ms: Some(_),
                ..
            }
        ));
        let replayed: PersonalGraphEvent = serde_json::from_str(
            r#"{"type":"retract_assertion","from":"00000000-0000-0000-0000-000000000001","to":"00000000-0000-0000-0000-000000000002","statement_id":"urn:assertion:old"}"#,
        ).unwrap();
        assert!(matches!(
            replayed,
            PersonalGraphEvent::RetractAssertion {
                withdrawn_at_ms: None,
                ..
            }
        ));
    }
}
