// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Query definitions are shared Resources, written through the current owner journal.
use crate::app::GraphshellApp;
use mere::kernel::graph::GraphStratum;
use mere::query_view as model;
use mere::query_view::{self, QueryNodeId, QueryProjection, QueryViewSpec};
use muniment::Backend;
use sceno::{LinkKey, LinkMarks, LinkStratum};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
pub fn save<B: Backend>(
    app: &mut GraphshellApp<B>,
    owner: Option<Uuid>,
    expected: Option<&QueryViewSpec>,
    nonce: Uuid,
    spec: &QueryViewSpec,
) -> Result<Uuid, String> {
    let mut candidate = app.host.graph().clone();
    let id = query_view::save_spec(&mut candidate, owner, expected, nonce, spec)?;
    app.host
        .mutate_product_graph(|graph| query_view::save_spec(graph, owner, expected, nonce, spec))
        .map_err(|e| e.to_string())??;
    Ok(id)
}
pub fn freeze<B: Backend>(
    app: &mut GraphshellApp<B>,
    owner: Uuid,
    nonce: Uuid,
) -> Result<Uuid, String> {
    let mut candidate = app.host.graph().clone();
    let id = query_view::freeze_view(&mut candidate, owner, nonce)?;
    app.host
        .mutate_product_graph(|graph| query_view::freeze_view(graph, owner, nonce))
        .map_err(|e| e.to_string())??;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
    use muniment::MemoryBackend;
    #[test]
    fn query_resource_save_freeze_undo_and_reopen_use_current_owner_journal() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let persona = SelectedPersonaRef {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                profile: "profile:graphshell-h3".into(),
            };
            let mut app = GraphshellApp::open_or_fixture(backend.clone(), persona.clone())
                .await
                .unwrap();
            let member = app
                .host
                .create_address("https://query.test/source", "source")
                .unwrap();
            let spec = QueryViewSpec::select(
                "Source query",
                "SELECT ?member WHERE { VALUES ?member { <https://query.test/source> } }",
            );
            let owner = save(&mut app, None, None, Uuid::from_u128(801), &spec).unwrap();
            let iri = app
                .host
                .graph()
                .resource(owner)
                .unwrap()
                .canonical_iri()
                .to_owned();
            let access = app.host.create_address(&iri, "Source query").unwrap();
            assert_eq!(
                query_view::read_spec(app.host.graph(), owner).unwrap(),
                spec
            );
            let projection = query_view::project_view(app.host.graph(), owner).unwrap();
            let resource = projection.members[0];
            projection
                .access_for(app.host.graph(), resource, member)
                .unwrap();
            let frozen = freeze(&mut app, owner, Uuid::from_u128(802)).unwrap();
            app.host.persist(2).await.unwrap();
            let mut reopened = GraphshellApp::open_or_fixture(backend.clone(), persona)
                .await
                .unwrap();
            assert_eq!(
                query_view::read_spec(reopened.host.graph(), owner).unwrap(),
                spec
            );
            assert!(reopened.host.graph().get_node_by_id(access).is_some());
            assert_eq!(
                reopened
                    .host
                    .graph()
                    .open_frozen_selection(frozen)
                    .unwrap()
                    .selection
                    .members(),
                &[resource]
            );
            let mut next = spec.clone();
            next.title = "Changed".into();
            save(&mut reopened, Some(owner), Some(&spec), Uuid::nil(), &next).unwrap();
            reopened
                .host
                .undo_now("graphshell")
                .unwrap()
                .expect("journaled query edit");
            assert_eq!(
                query_view::read_spec(reopened.host.graph(), owner).unwrap(),
                spec
            );
        });
    }
}

/// Explicit acceptance fixture; ordinary saved graphs never call this path.
pub async fn seed_fixture<B: Backend + Clone>(app: &mut GraphshellApp<B>) -> Result<(), String> {
    let a = app
        .host
        .create_address("https://query.example/source", "Query source")
        .map_err(|e| e.to_string())?;
    app.host
        .mutate_product_graph(|graph| {
            let key = mere::kernel::graph::apply::add_node(
                graph,
                Some(Uuid::new_v4()),
                "https://query.example/source".into(),
                Default::default(),
            );
            mere::kernel::graph::apply::apply_graph_delta(
                graph,
                mere::kernel::graph::apply::GraphDelta::SetNodeTitle {
                    key,
                    title: "Independent source access".into(),
                },
            );
        })
        .map_err(|e| e.to_string())?;
    let b = app
        .host
        .create_address("https://query.example/target", "Query target")
        .map_err(|e| e.to_string())?;
    app.host
        .mutate_product_graph(|graph| -> Result<(), String> {
            use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
            use mere::kernel::graph::resource_tags::TagConcept;
            use mere::kernel::graph::{EdgeAssertion, SemanticSubKind};
            let from = graph.get_node_key_by_id(a).ok_or("Missing source")?;
            let to = graph.get_node_key_by_id(b).ok_or("Missing target")?;
            let resource = graph
                .shown_resource_id(from)
                .ok_or("Missing shown Resource")?;
            for (owner, iri) in [
                ("urn:author:alice", "urn:tag:alice"),
                ("urn:author:bob", "urn:tag:bob"),
            ] {
                graph
                    .tag_resource_with_concept(
                        resource,
                        iri,
                        TagConcept {
                            owner_iri: owner.into(),
                            label: "Music".into(),
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            for kind in [SemanticSubKind::Cites, SemanticSubKind::UserGrouped] {
                apply_graph_delta(
                    graph,
                    GraphDelta::AssertRelation {
                        from,
                        to,
                        assertion: EdgeAssertion::Semantic {
                            sub_kind: kind,
                            label: Some("query fixture".into()),
                            decay_progress: None,
                        },
                        asserter_iri: "urn:author:fixture".into(),
                    },
                );
            }
            Ok(())
        })
        .map_err(|e| e.to_string())??;
    app.host.persist(7).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct PresentedLink {
    pub key: LinkKey,
    pub representative: usize,
    pub marks: LinkMarks,
    pub strength: usize,
    pub rows: usize,
}

fn endpoint_label(projection: &QueryProjection, endpoint: QueryNodeId) -> Option<String> {
    match endpoint.stratum {
        GraphStratum::Surface => Some(endpoint.id.to_string()),
        GraphStratum::Resource => projection
            .nodes
            .iter()
            .find(|node| node.identity == endpoint)
            .map(|node| node.address.clone())
            .filter(|address| !address.is_empty()),
    }
}

fn legacy_members(payload: &serde_json::Value) -> BTreeSet<(String, Option<String>)> {
    let mut members = BTreeSet::new();
    for family in ["arrangement", "containment", "imported", "provenance"] {
        let Some(data) = payload.get(family).filter(|data| !data.is_null()) else {
            continue;
        };
        if let Some(kinds) = data.get("sub_kinds").and_then(serde_json::Value::as_array) {
            for kind in kinds.iter().filter_map(serde_json::Value::as_str) {
                members.insert((family.to_owned(), Some(kind.to_owned())));
            }
        }
    }
    if payload
        .get("traversal")
        .is_some_and(|traversal| !traversal.is_null())
    {
        members.insert(("traversal".into(), None));
    }
    members
}

/// Collapse the query projection's repeated/lifted rows into one display
/// appearance per authoritative source stratum and endpoint pair. Shown
/// bindings are deliberately left outside this grouping.
pub fn presented_links(projection: &QueryProjection) -> Vec<PresentedLink> {
    struct Accumulator {
        presented: PresentedLink,
        assertions: BTreeSet<(String, String)>,
        legacy: BTreeSet<(String, Option<String>)>,
        supports: bool,
        contradicts: bool,
    }

    let mut grouped: BTreeMap<LinkKey, Accumulator> = BTreeMap::new();
    for (index, relation) in projection.relations.iter().enumerate() {
        if relation.kind == model::VisualRelationKind::ShownBinding
            || relation.source_from.stratum != relation.source_to.stratum
        {
            continue;
        }
        let stratum = match relation.source_from.stratum {
            GraphStratum::Resource => LinkStratum::Resource,
            GraphStratum::Surface => LinkStratum::Surface,
        };
        let (Some(from), Some(to)) = (
            endpoint_label(projection, relation.source_from),
            endpoint_label(projection, relation.source_to),
        ) else {
            continue;
        };
        let key = LinkKey::new(stratum, &from, &to);
        let entry = grouped.entry(key.clone()).or_insert_with(|| Accumulator {
            presented: PresentedLink {
                key: key.clone(),
                representative: index,
                marks: LinkMarks::default(),
                strength: 0,
                rows: 0,
            },
            assertions: BTreeSet::new(),
            legacy: BTreeSet::new(),
            supports: false,
            contradicts: false,
        });
        entry.presented.rows += 1;
        let relation_is_base_resource_row = relation.kind
            == model::VisualRelationKind::ResourceLink
            && relation.from.stratum == GraphStratum::Resource
            && relation.to.stratum == GraphStratum::Resource;
        let current_is_base_resource_row = projection.relations[entry.presented.representative]
            .kind
            == model::VisualRelationKind::ResourceLink
            && projection.relations[entry.presented.representative]
                .from
                .stratum
                == GraphStratum::Resource
            && projection.relations[entry.presented.representative]
                .to
                .stratum
                == GraphStratum::Resource;
        if relation_is_base_resource_row && !current_is_base_resource_row {
            entry.presented.representative = index;
        }

        let forward = from == key.first && to == key.second;
        let mut eligible = false;
        let exact_records = relation.assertions.is_some();
        let records = relation
            .assertions
            .as_deref()
            // Compatibility for query projections serialized before the
            // all-family field existed.
            .unwrap_or(&relation.statements);
        for statement in records.iter().filter(|statement| statement.is_held()) {
            eligible = true;
            entry.supports |= statement.predicate == "https://mere.computer/ns/rel#supports";
            entry.contradicts |= statement.predicate == "https://mere.computer/ns/rel#contradicts";
            let scope = serde_json::to_string(&statement.graph_scope).unwrap_or_default();
            entry
                .assertions
                .insert((statement.statement_id.clone(), scope));
        }
        let members = legacy_members(&relation.families);
        if exact_records {
            // Traversal remains a separate summary input until its own
            // assertion-ledger migration. Family rows are represented by
            // canonical assertions and must not be counted a second time.
            eligible |= members.iter().any(|(family, _)| family == "traversal");
            entry.legacy.extend(
                members
                    .into_iter()
                    .filter(|(family, _)| family == "traversal"),
            );
        } else {
            eligible |= !members.is_empty();
            entry.legacy.extend(members);
        }
        if eligible {
            if forward {
                entry.presented.marks.forward = true;
            } else {
                entry.presented.marks.reverse = true;
            }
        }
    }
    grouped
        .into_values()
        .map(|mut entry| {
            entry.presented.marks.opposed = entry.supports && entry.contradicts;
            entry.presented.strength = entry.assertions.len() + entry.legacy.len();
            entry.presented
        })
        .filter(|link| link.strength > 0)
        .collect()
}

#[cfg(test)]
mod grouping_tests {
    use super::*;
    use mere::kernel::graph::{ContainmentSubKind, SemanticStatement, SemanticSubKind};
    use mere::kernel::types::AssertionWithdrawal;
    use mere::kernel::types::GraphScope;
    use mere::query_view::{QueryNode, QueryRelation};

    fn statement(id: &str, kind: SemanticSubKind) -> SemanticStatement {
        let predicate = match kind {
            SemanticSubKind::Supports => "https://mere.computer/ns/rel#supports",
            SemanticSubKind::Contradicts => "https://mere.computer/ns/rel#contradicts",
            _ => "https://mere.computer/ns/rel#cites",
        };
        let mut statement = SemanticStatement::new(
            predicate.into(),
            Some(kind),
            None,
            GraphScope::Default,
            Some("https://author.test/a".into()),
            Some(10),
        );
        statement.statement_id = id.into();
        statement
    }

    fn relation(
        from: QueryNodeId,
        to: QueryNodeId,
        source_from: QueryNodeId,
        source_to: QueryNodeId,
        kind: model::VisualRelationKind,
        statements: Vec<SemanticStatement>,
    ) -> QueryRelation {
        QueryRelation {
            from,
            to,
            kind,
            source_from,
            source_to,
            assertions: Some(statements.clone()),
            statements,
            families: serde_json::json!({}),
            hop_cost: if kind == model::VisualRelationKind::ShownBinding {
                0
            } else {
                1
            },
        }
    }

    #[test]
    fn grouping_keeps_strata_separate_deduplicates_lifted_rows_and_preserves_directions() {
        let resource_a = QueryNodeId::resource(Uuid::from_u128(1));
        let resource_b = QueryNodeId::resource(Uuid::from_u128(2));
        let surface_a = QueryNodeId::surface(Uuid::from_u128(1));
        let surface_b = QueryNodeId::surface(Uuid::from_u128(2));
        let surface_alias_a = QueryNodeId::surface(Uuid::from_u128(3));
        let surface_alias_b = QueryNodeId::surface(Uuid::from_u128(4));
        let nodes = vec![
            QueryNode {
                identity: resource_a,
                title: "A resource".into(),
                address: "https://resource.test/a".into(),
                member: true,
                tags: vec![],
                held_accesses: vec![surface_a.id],
            },
            QueryNode {
                identity: resource_b,
                title: "B resource".into(),
                address: "https://resource.test/b".into(),
                member: true,
                tags: vec![],
                held_accesses: vec![surface_b.id],
            },
            QueryNode {
                identity: surface_a,
                title: "A surface".into(),
                address: String::new(),
                member: true,
                tags: vec![],
                held_accesses: vec![],
            },
            QueryNode {
                identity: surface_b,
                title: "B surface".into(),
                address: String::new(),
                member: true,
                tags: vec![],
                held_accesses: vec![],
            },
            QueryNode {
                identity: surface_alias_a,
                title: "A alias".into(),
                address: String::new(),
                member: true,
                tags: vec![],
                held_accesses: vec![],
            },
            QueryNode {
                identity: surface_alias_b,
                title: "B alias".into(),
                address: String::new(),
                member: true,
                tags: vec![],
                held_accesses: vec![],
            },
        ];
        let projection = QueryProjection {
            owner: Uuid::from_u128(9),
            source_revision: 1,
            spec: QueryViewSpec::select("fixture", "SELECT ?member WHERE {}"),
            members: vec![],
            missing_resources: vec![],
            coverage: Default::default(),
            nodes,
            relations: vec![
                relation(
                    resource_a,
                    resource_b,
                    resource_a,
                    resource_b,
                    model::VisualRelationKind::ResourceLink,
                    vec![statement("support", SemanticSubKind::Supports)],
                ),
                relation(
                    resource_b,
                    resource_a,
                    resource_b,
                    resource_a,
                    model::VisualRelationKind::ResourceLink,
                    vec![statement("contradiction", SemanticSubKind::Contradicts)],
                ),
                relation(
                    surface_alias_a,
                    surface_alias_b,
                    resource_a,
                    resource_b,
                    model::VisualRelationKind::ResourceLink,
                    vec![statement("support", SemanticSubKind::Supports)],
                ),
                relation(
                    surface_a,
                    surface_b,
                    surface_a,
                    surface_b,
                    model::VisualRelationKind::SurfaceLink,
                    vec![statement("surface-support", SemanticSubKind::Supports)],
                ),
                relation(
                    resource_a,
                    surface_a,
                    resource_a,
                    surface_a,
                    model::VisualRelationKind::ShownBinding,
                    vec![],
                ),
            ],
        };

        let groups = presented_links(&projection);
        assert_eq!(groups.len(), 2);
        let resource = groups
            .iter()
            .find(|group| group.key.stratum == LinkStratum::Resource)
            .unwrap();
        assert_eq!(resource.rows, 3);
        assert_eq!(resource.strength, 2);
        assert!(resource.marks.forward && resource.marks.reverse && resource.marks.opposed);
        assert_eq!(
            projection.relations[resource.representative].from.stratum,
            GraphStratum::Resource,
            "prefer the canonical Resource-to-Resource row for opening"
        );
        let surface = groups
            .iter()
            .find(|group| group.key.stratum == LinkStratum::Surface)
            .unwrap();
        assert_eq!(surface.rows, 1);
        assert_eq!(surface.strength, 1);
        assert!(surface.marks.forward && !surface.marks.reverse && !surface.marks.opposed);
        assert_eq!(
            projection
                .relations
                .iter()
                .filter(|relation| relation.kind == model::VisualRelationKind::ShownBinding)
                .count(),
            1,
            "shown-binding rows remain separate and are never folded into a link"
        );
    }

    #[test]
    fn all_family_reasons_contribute_exact_held_strength_without_double_counting() {
        let from = QueryNodeId::resource(Uuid::from_u128(1));
        let to = QueryNodeId::resource(Uuid::from_u128(2));
        let mut held_a = SemanticStatement::new(
            "https://mere.computer/ns/rel#containment.collection-member".into(),
            None,
            None,
            GraphScope::User,
            Some("https://author.test/a".into()),
            Some(10),
        );
        held_a.statement_id = "urn:statement:collection-a".into();
        held_a.basis.evidence.push("urn:evidence:a".into());
        let mut held_b = held_a.clone();
        held_b.statement_id = "urn:statement:collection-b".into();
        held_b.basis.evidence = vec!["urn:evidence:b".into()];
        let mut withdrawn = held_a.clone();
        withdrawn.statement_id = "urn:statement:collection-withdrawn".into();
        withdrawn.withdrawn = Some(AssertionWithdrawal {
            at_ms: Some(30),
            by: Some("https://author.test/a".into()),
        });

        let mut projection = QueryProjection {
            owner: Uuid::from_u128(9),
            source_revision: 1,
            spec: QueryViewSpec::select("fixture", "SELECT ?member WHERE {}"),
            members: vec![],
            missing_resources: vec![],
            coverage: Default::default(),
            nodes: vec![
                QueryNode {
                    identity: from,
                    title: "A".into(),
                    address: "https://resource.test/a".into(),
                    member: true,
                    tags: vec![],
                    held_accesses: vec![],
                },
                QueryNode {
                    identity: to,
                    title: "B".into(),
                    address: "https://resource.test/b".into(),
                    member: true,
                    tags: vec![],
                    held_accesses: vec![],
                },
            ],
            relations: vec![relation(
                from,
                to,
                from,
                to,
                model::VisualRelationKind::ResourceLink,
                vec![],
            )],
        };
        projection.relations[0].assertions = Some(vec![held_a, held_b, withdrawn]);
        // This compatibility summary describes the same family and must not
        // inflate strength alongside the exact canonical records.
        projection.relations[0].families = serde_json::json!({
            "containment": {"sub_kinds": [ContainmentSubKind::CollectionMember]},
            "traversal": {"occurrences": []}
        });

        let groups = presented_links(&projection);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].rows, 1);
        assert_eq!(groups[0].strength, 3);
        assert!(groups[0].marks.forward);
        assert!(!groups[0].marks.reverse && !groups[0].marks.opposed);
        let assertions = projection.relations[0].assertions.as_ref().unwrap();
        assert_eq!(assertions.len(), 3);
        assert_eq!(
            assertions[2].statement_id,
            "urn:statement:collection-withdrawn"
        );
        for assertion in projection.relations[0].assertions.as_mut().unwrap() {
            assertion.withdrawn = Some(AssertionWithdrawal {
                at_ms: Some(40),
                by: Some("https://author.test/a".into()),
            });
        }
        projection.relations[0].families["traversal"] = serde_json::Value::Null;
        assert!(
            presented_links(&projection).is_empty(),
            "a pair with only retained withdrawals has no resting stroke"
        );
        assert_eq!(
            projection.relations[0].assertions.as_ref().unwrap().len(),
            3,
            "presentation keeps all source history records"
        );
    }
}
