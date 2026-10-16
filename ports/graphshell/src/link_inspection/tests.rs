// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, CoverageLayer, CoverageLimit, EdgeAssertion,
    ImportedSubKind, NavigationTrigger, ProvenanceSubKind, RelationKind, SemanticStatement,
    SemanticStatementSpec, predicate_iri,
};
use mere::kernel::types::{AssertionBasis, AssertionMethod};

fn fixture() -> (Graph, [Uuid; 4]) {
    let mut graph = Graph::new();
    let ids = [1, 2, 3, 4].map(Uuid::from_u128);
    for (id, address) in ids.into_iter().zip([
        "https://source.test/",
        "https://target.test/",
        "https://source.test/",
        "https://target.test/",
    ]) {
        add_node(&mut graph, Some(id), address.into(), Default::default());
    }
    (graph, ids)
}

fn assert(graph: &mut Graph, from: Uuid, to: Uuid, kind: SemanticSubKind, author: &str) -> String {
    graph
        .try_assert_semantic_statement(
            graph.get_node_key_by_id(from).unwrap(),
            graph.get_node_key_by_id(to).unwrap(),
            SemanticStatementSpec {
                predicate: predicate_iri(kind).into(),
                recognized_sub_kind: Some(kind),
                label: Some(format!("claim by {author}")),
                basis: AssertionBasis::default(),
                graph_scope: GraphScope::Custom("urn:graph:review".into()),
                provenance_iri: Some(author.into()),
                asserted_at_ms: Some(17),
            },
        )
        .unwrap()
        .1
        .statement_id
}

fn stored_assertion(
    graph: &mut Graph,
    from: Uuid,
    to: Uuid,
    id: &str,
    first: Option<u64>,
    last: Option<u64>,
) {
    stored_assertion_kind(graph, from, to, id, SemanticSubKind::Cites, first, last);
}

fn stored_assertion_kind(
    graph: &mut Graph,
    from: Uuid,
    to: Uuid,
    id: &str,
    kind: SemanticSubKind,
    first: Option<u64>,
    last: Option<u64>,
) {
    graph
        .try_assert_persisted_semantic_statement(
            graph.get_node_key_by_id(from).unwrap(),
            graph.get_node_key_by_id(to).unwrap(),
            SemanticStatement {
                statement_id: id.into(),
                predicate: predicate_iri(kind).into(),
                recognized_sub_kind: Some(kind),
                label: Some(format!("note:{id}")),
                basis: AssertionBasis {
                    note: Some(format!("note:{id}")),
                    method: Some(AssertionMethod {
                        iri: "urn:method:extractor".into(),
                        version: Some("v3".into()),
                    }),
                    evidence: vec!["urn:evidence:b".into(), "urn:evidence:a".into()],
                },
                graph_scope: Default::default(),
                provenance_iri: Some("urn:asserter:history".into()),
                first_asserted_ms: first,
                asserted_at_ms: last,
                withdrawn: None,
            },
        )
        .unwrap();
}

#[test]
fn same_predicate_assertions_keep_exact_handles_and_attribution_without_mutation() {
    let (mut graph, [a, b, _, _]) = fixture();
    let alice = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let bob = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:bob");
    let before = serde_json::to_value(graph.to_snapshot()).unwrap();
    let inspected = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    assert!(matches!(
        inspected.source,
        scenograph::link::LinkSource::Unavailable { .. }
    ));
    assert_eq!(inspected.records.len(), 1);
    let statements = &inspected.records[0].statements;
    assert_eq!(statements.len(), 2);
    assert_ne!(alice, bob);
    let first = inspect_statement(&graph, &alice).unwrap();
    let second = inspect_statement(&graph, &bob).unwrap();
    assert_eq!(first.statement.predicate, second.statement.predicate);
    assert_eq!(
        first.statement.provenance_iri.as_deref(),
        Some("urn:author:alice")
    );
    assert_eq!(
        second.statement.provenance_iri.as_deref(),
        Some("urn:author:bob")
    );
    assert_eq!(
        first.statement.graph_scope,
        GraphScope::Custom("urn:graph:review".into())
    );
    assert_eq!(first.statement.asserted_at_ms, Some(17));
    assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), before);
}

#[test]
fn verified_source_context_survives_the_graphshell_adapter_unchanged() {
    let (mut graph, [a, b, _, _]) = fixture();
    assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let source = scenograph::link::LinkSource::Available {
        binding: scenograph::SourceBinding {
            authority: "mere:test".into(),
            domain: "graph".into(),
            resource: "host-owned-graph".into(),
        },
        revision: scenograph::link::LinkSourceRevision::Available {
            revision: scenograph::PublicSourceRevision::new("public-r4"),
        },
    };

    let inspected =
        inspect_link_with_source(&graph, LinkTarget::new(a, b), source.clone()).unwrap();
    assert_eq!(inspected.source, source);
    assert_eq!(inspected.records.len(), 1);
}

#[test]
fn lifted_records_share_resource_owner_but_keep_distinct_displayed_access_pairs() {
    let (mut graph, [a, b, c, d]) = fixture();
    assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let ab = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    let cd = inspect_link(&graph, LinkTarget::new(c, d)).unwrap();
    assert_eq!(ab.records[0].owner, cd.records[0].owner);
    assert_eq!(ab.records[0].statements, cd.records[0].statements);
    assert_ne!(ab.records[0].displayed_from, cd.records[0].displayed_from);
    assert_eq!(ab.records[0].owner.stratum, GraphStratum::Resource);
    assert_ne!(ab.records[0].owner.from.id, a);
    assert_eq!(
        linked_accesses(&graph, a)
            .unwrap()
            .iter()
            .map(|a| a.member)
            .collect::<Vec<_>>(),
        vec![b, d]
    );
}

#[test]
fn both_directions_and_both_strata_remain_distinct() {
    let (mut graph, [a, b, _, _]) = fixture();
    let source = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let direct = assert(
        &mut graph,
        b,
        a,
        SemanticSubKind::UserGrouped,
        "urn:author:bob",
    );
    let inspected = inspect_link(&graph, LinkTarget::new(b, a)).unwrap();
    assert_eq!(inspected.records.len(), 2);
    let content = inspected
        .records
        .iter()
        .find(|r| r.owner.stratum == GraphStratum::Resource)
        .unwrap();
    let surface = inspected
        .records
        .iter()
        .find(|r| r.owner.stratum == GraphStratum::Surface)
        .unwrap();
    assert_eq!((content.displayed_from, content.displayed_to), (a, b));
    assert_eq!((surface.displayed_from, surface.displayed_to), (b, a));
    assert_eq!((surface.owner.from.id, surface.owner.to.id), (b, a));
    assert_eq!(content.statements[0].statement_id, source);
    assert_eq!(surface.statements[0].statement_id, direct);
    assert_eq!(
        inspect_statement(&graph, &direct).unwrap().owner,
        surface.owner
    );
}

#[test]
fn parallel_buckets_and_self_links_are_not_dropped_or_double_counted() {
    let (mut graph, [a, b, _, _]) = fixture();
    let id = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let owner = inspect_statement(&graph, &id).unwrap().owner;
    let edge = graph
        .to_snapshot()
        .resource_edges
        .into_iter()
        .find(|edge| {
            edge.retained_assertions()
                .iter()
                .any(|statement| statement.statement_id == id)
        })
        .expect("the authored assertion is retained in its Resource bucket");
    let mut second = edge.clone();
    let statement = &mut second.retained_assertions_mut()[0];
    statement.statement_id = "urn:statement:parallel".into();
    statement.provenance_iri = Some("urn:author:bob".into());
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetResourceEdgesByIds {
            from_resource_id: owner.from.id,
            to_resource_id: owner.to.id,
            edges: vec![edge, second],
        },
    );
    assert_eq!(
        inspect_link(&graph, LinkTarget::new(a, b))
            .unwrap()
            .records
            .len(),
        2
    );
    assert(
        &mut graph,
        a,
        a,
        SemanticSubKind::UserGrouped,
        "urn:author:alice",
    );
    let looped = inspect_link(&graph, LinkTarget::new(a, a)).unwrap();
    assert_eq!(looped.records.len(), 1);
    assert_eq!(
        (
            looped.records[0].displayed_from,
            looped.records[0].displayed_to
        ),
        (a, a)
    );
    assert_eq!(
        linked_accesses(&graph, a)
            .unwrap()
            .iter()
            .filter(|s| s.member == a)
            .count(),
        1
    );
}

#[test]
fn session_sidecar_projection_preserves_its_canonical_assertion() {
    let (mut graph, [a, b, _, _]) = fixture();
    let from = graph.get_node_key_by_id(a).unwrap();
    let to = graph.get_node_key_by_id(b).unwrap();
    assert_relation(
        &mut graph,
        from,
        to,
        EdgeAssertion::Arrangement {
            sub_kind: ArrangementSubKind::SplitPair,
        },
    );
    let inspected = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    assert_eq!(inspected.records.len(), 1);
    let record = &inspected.records[0];
    let canonical = graph
        .get_edge(graph.find_edge_key(from, to).unwrap())
        .unwrap()
        .assertions()
        .iter()
        .find(|statement| statement.recognized_sub_kind.is_none())
        .expect("the session Arrangement claim has an exact runtime assertion");
    assert_eq!(record.statements.len(), 1);
    assert_eq!(record.statements[0].statement_id, canonical.statement_id);
    assert_eq!(record.statements[0].predicate, canonical.predicate);
    assert_eq!(record.statements[0].recognized_sub_kind, None);
    assert_eq!(
        record.statements[0].provenance_iri,
        canonical.provenance_iri
    );
    assert_eq!(
        record.statements[0].asserted_at_ms,
        canonical.asserted_at_ms
    );
    assert_eq!(
        record.statements[0].first_asserted_ms,
        canonical.first_asserted_ms
    );
    assert!(
        graph.to_snapshot().edges.iter().all(|edge| {
            edge.retained_assertions()
                .iter()
                .all(|statement| statement.statement_id != canonical.statement_id)
        }),
        "durable snapshots still omit session SplitPair assertions"
    );
    assert_eq!(record.owner.stratum, GraphStratum::Surface);
    assert!(
        record.families.is_empty(),
        "Arrangement is represented by its exact assertion"
    );
    assert_eq!(
        canonical.predicate,
        RelationKind::Arrangement(ArrangementSubKind::SplitPair)
            .predicate_iri()
            .unwrap()
    );
}

#[test]
fn mixed_family_data_preserves_canonical_assertions_events_and_aggregates() {
    let (mut graph, [a, b, _, _]) = fixture();
    let from = graph.get_node_key_by_id(a).unwrap();
    let to = graph.get_node_key_by_id(b).unwrap();
    for assertion in [
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::NotebookSection,
        },
        EdgeAssertion::Imported {
            sub_kind: ImportedSubKind::SessionImport,
        },
        EdgeAssertion::Provenance {
            sub_kind: ProvenanceSubKind::CopiedFrom,
        },
    ] {
        assert_relation(&mut graph, from, to, assertion);
    }
    apply_graph_delta(
        &mut graph,
        GraphDelta::AppendTraversal {
            from,
            to,
            trigger: NavigationTrigger::LinkClick,
            timestamp_ms: Some(42),
        },
    );
    let inspected = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    let snapshot = graph.to_snapshot();
    let canonical_ids: BTreeSet<_> = snapshot
        .edges
        .iter()
        .flat_map(|edge| edge.retained_assertions())
        .map(|statement| statement.statement_id.as_str())
        .collect();
    let statements: Vec<_> = inspected
        .records
        .iter()
        .flat_map(|record| &record.statements)
        .collect();
    assert_eq!(statements.len(), 3);
    assert!(statements.iter().all(|statement| {
        statement.recognized_sub_kind.is_none()
            && canonical_ids.contains(statement.statement_id.as_str())
    }));
    let families: Vec<_> = inspected
        .records
        .iter()
        .flat_map(|record| &record.families)
        .collect();
    for kind in [
        RelationKind::Containment(ContainmentSubKind::NotebookSection),
        RelationKind::Imported(ImportedSubKind::SessionImport),
        RelationKind::Provenance(ProvenanceSubKind::CopiedFrom),
    ] {
        assert_eq!(
            statements
                .iter()
                .filter(|statement| statement.predicate == kind.predicate_iri().unwrap())
                .count(),
            1
        );
    }
    assert!(
        families
            .iter()
            .all(|family| family.family == EdgeFamily::Traversal),
        "only event-shaped Traversal retains a legacy summary"
    );
    let traversal = &families
        .iter()
        .find(|held| held.family == EdgeFamily::Traversal)
        .unwrap()
        .data;
    assert_eq!(
        traversal["traversals"],
        json!([{"timestamp_ms": 42, "trigger": "LinkClick"}])
    );
    assert_eq!(traversal["metrics"]["total_navigations"], 1);
    assert_eq!(traversal["metrics"]["forward_navigations"], 1);
    assert_eq!(traversal["metrics"]["backward_navigations"], 0);
    assert_eq!(traversal["metrics"]["last_navigated_at"], 42);
}

#[test]
fn exact_assertion_survives_rebinding_but_missing_targets_never_retarget() {
    let (mut graph, [a, b, _, _]) = fixture();
    let id = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    let held = inspect_statement(&graph, &id).unwrap();
    let key = graph.get_node_key_by_id(a).unwrap();
    apply_graph_delta(
        &mut graph,
        GraphDelta::NavigateNode {
            key,
            url: "https://new.test/".into(),
        },
    );
    assert!(
        inspect_link(&graph, LinkTarget::new(a, b))
            .unwrap()
            .records
            .is_empty()
    );
    assert_eq!(inspect_statement(&graph, &id).unwrap(), held);
    assert!(graph.retract_assertion(&id));
    assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    assert!(inspect_statement(&graph, &id).is_err());
    apply_graph_delta(&mut graph, GraphDelta::RemoveNode { key });
    add_node(
        &mut graph,
        Some(Uuid::from_u128(99)),
        "https://new.test/".into(),
        Default::default(),
    );
    assert!(inspect_link(&graph, LinkTarget::new(a, b)).is_err());
    assert!(linked_accesses(&graph, a).is_err());
}

#[test]
fn missing_bindings_and_scoped_coverage_are_disclosed() {
    let (mut graph, [a, b, c, _]) = fixture();
    let resource = graph
        .shown_resource_id(graph.get_node_key_by_id(a).unwrap())
        .unwrap();
    let mut source = CoverageLimit::new(CoverageLayer::Residency, "more assertions may exist");
    source.resources.push(resource);
    let mut sibling = CoverageLimit::new(CoverageLayer::Disclosure, "another access is partial");
    sibling.surfaces.push(c);
    let mut unrelated = CoverageLimit::new(CoverageLayer::Residency, "unrelated");
    unrelated.resources.push(Uuid::from_u128(99));
    let global = CoverageLimit::new(CoverageLayer::Synchronization, "offline");
    graph.set_known_coverage(CoverageNote {
        limits: vec![source.clone(), sibling.clone(), unrelated, global.clone()],
    });
    assert_eq!(
        inspect_link(&graph, LinkTarget::new(a, b))
            .unwrap()
            .coverage
            .limits,
        vec![source, sibling, global]
    );
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetShownResourceById {
            surface_id: a,
            resource_id: None,
        },
    );
    let inspected = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    assert_eq!(inspected.notes.len(), 1);
    assert!(inspected.records.is_empty());
}

#[test]
fn stable_bundle_and_statement_targets_roundtrip_through_graph_snapshot() {
    let (mut graph, [a, b, _, _]) = fixture();
    let id = assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:alice");
    assert(&mut graph, a, b, SemanticSubKind::Cites, "urn:author:bob");
    let target = LinkTarget::new(a, b);
    let inspected = inspect_link(&graph, target).unwrap();
    let exact = inspect_statement(&graph, &id).unwrap();
    let reopened = Graph::from_snapshot(&graph.to_snapshot());
    assert_eq!(inspect_link(&reopened, target).unwrap(), inspected);
    assert_eq!(inspect_statement(&reopened, &id).unwrap(), exact);
    assert_eq!(
        serde_json::from_value::<LinkInspection>(serde_json::to_value(&inspected).unwrap())
            .unwrap(),
        inspected
    );
}

#[test]
fn history_is_opt_in_preserves_lifecycle_and_sorts_undated_entries_last() {
    let (mut graph, [a, b, _, _]) = fixture();
    stored_assertion(&mut graph, a, b, "z-undated", None, None);
    stored_assertion(&mut graph, a, b, "c-held", Some(20), Some(24));
    stored_assertion(&mut graph, a, b, "b-held", Some(10), Some(15));
    stored_assertion(&mut graph, a, b, "a-withdrawn", Some(10), Some(18));
    let from = graph.get_node_key_by_id(a).unwrap();
    let to = graph.get_node_key_by_id(b).unwrap();
    assert!(graph.apply_imported_semantic_withdrawal_for_pair(
        from,
        to,
        "a-withdrawn",
        Some(25),
        Some("urn:withdrawer:reviewer".into()),
    ));

    // The existing inspector contract remains held-only.
    let held = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    let held_ids: Vec<_> = held.records[0]
        .statements
        .iter()
        .map(|statement| statement.statement_id.as_str())
        .collect();
    assert_eq!(held_ids, ["b-held", "c-held", "z-undated"]);
    assert!(inspect_statement(&graph, "a-withdrawn").is_err());

    let history = inspect_link_history(&graph, LinkTarget::new(a, b)).unwrap();
    assert!(matches!(
        history.source,
        scenograph::link::LinkSource::Unavailable { .. }
    ));
    let ids: Vec<_> = history
        .entries
        .iter()
        .map(|entry| entry.assertion.id.as_str())
        .collect();
    assert_eq!(ids, ["a-withdrawn", "b-held", "c-held", "z-undated"]);

    let withdrawn = &history.entries[0].assertion;
    assert_eq!(withdrawn.first_asserted_ms, Some(10));
    assert_eq!(withdrawn.asserted_at_ms, Some(18));
    assert_eq!(
        withdrawn.withdrawn,
        Some(scenograph::link::LinkWithdrawal {
            at_ms: Some(25),
            by: Some("urn:withdrawer:reviewer".into()),
        })
    );
    assert_eq!(withdrawn.basis.note.as_deref(), Some("note:a-withdrawn"));
    assert_eq!(
        withdrawn
            .basis
            .method
            .as_ref()
            .map(|method| method.iri.as_str()),
        Some("urn:method:extractor")
    );
    assert_eq!(
        withdrawn.basis.evidence,
        ["urn:evidence:b", "urn:evidence:a"]
    );
    assert_eq!(history.entries[1].assertion.first_asserted_ms, Some(10));
    assert_eq!(history.entries[2].assertion.first_asserted_ms, Some(20));
    assert_eq!(history.entries[3].assertion.first_asserted_ms, None);
}

#[test]
fn exact_containment_ids_are_independently_inspectable_and_withdrawable() {
    let (mut graph, [a, b, _, _]) = fixture();
    let predicate = RelationKind::Containment(ContainmentSubKind::CollectionMember)
        .predicate_iri()
        .expect("canonical containment predicate")
        .to_owned();
    for (id, author) in [
        ("collection-claim-a", "urn:author:a"),
        ("collection-claim-b", "urn:author:b"),
    ] {
        graph
            .try_assert_persisted_relation_statement(
                graph.get_node_key_by_id(a).unwrap(),
                graph.get_node_key_by_id(b).unwrap(),
                SemanticStatement {
                    statement_id: id.into(),
                    predicate: predicate.clone(),
                    recognized_sub_kind: None,
                    label: None,
                    basis: AssertionBasis {
                        note: Some(format!("independent {id}")),
                        ..Default::default()
                    },
                    graph_scope: GraphScope::Custom("urn:collection:owners".into()),
                    provenance_iri: Some(author.into()),
                    first_asserted_ms: Some(30),
                    asserted_at_ms: Some(31),
                    withdrawn: None,
                },
            )
            .unwrap();
    }
    let before = inspect_link(&graph, LinkTarget::new(a, b)).unwrap();
    let ids = before.records[0]
        .statements
        .iter()
        .map(|statement| statement.statement_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from(["collection-claim-a", "collection-claim-b"])
    );
    assert_eq!(before.records[0].statements[0].predicate, predicate);

    graph.retract_assertion("collection-claim-b");
    assert!(inspect_statement(&graph, "collection-claim-a").is_ok());
    assert!(inspect_statement(&graph, "collection-claim-b").is_err());
    let history = inspect_link_history(&graph, LinkTarget::new(a, b)).unwrap();
    let retained = history
        .entries
        .iter()
        .filter(|entry| entry.assertion.predicate == predicate)
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 2);
    assert!(
        retained
            .iter()
            .any(|entry| entry.assertion.id == "collection-claim-b")
    );
}

#[test]
fn held_inspection_omits_surface_and_resource_tombstone_only_buckets() {
    let (mut graph, [a, b, _, _]) = fixture();
    stored_assertion_kind(
        &mut graph,
        a,
        b,
        "surface-tombstone",
        SemanticSubKind::UserGrouped,
        Some(10),
        Some(15),
    );
    stored_assertion_kind(
        &mut graph,
        a,
        b,
        "resource-tombstone",
        SemanticSubKind::Cites,
        Some(20),
        Some(25),
    );
    let from = graph.get_node_key_by_id(a).unwrap();
    let to = graph.get_node_key_by_id(b).unwrap();
    assert!(graph.apply_imported_semantic_withdrawal_for_pair(
        from,
        to,
        "surface-tombstone",
        Some(30),
        Some("urn:withdrawer:surface".into()),
    ));
    assert!(graph.apply_imported_semantic_withdrawal_for_pair(
        from,
        to,
        "resource-tombstone",
        Some(31),
        Some("urn:withdrawer:resource".into()),
    ));

    let target = LinkTarget::new(a, b);
    let held = inspect_link(&graph, target).unwrap();
    assert!(held.records.is_empty());

    let history = inspect_link_history(&graph, target).unwrap();
    let mut entries: Vec<_> = history
        .entries
        .iter()
        .map(|entry| (entry.assertion.id.as_str(), entry.owner.stratum))
        .collect();
    entries.sort_by_key(|(id, _)| *id);
    assert_eq!(
        entries,
        [
            ("resource-tombstone", GraphStratum::Resource),
            ("surface-tombstone", GraphStratum::Surface),
        ]
    );
}
