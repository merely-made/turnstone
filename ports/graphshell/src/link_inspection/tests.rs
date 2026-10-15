// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{
    ArrangementSubKind, ContainmentSubKind, CoverageLayer, CoverageLimit, EdgeAssertion,
    ImportedSubKind, NavigationTrigger, ProvenanceSubKind, SemanticStatementSpec, predicate_iri,
};

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
                graph_scope: GraphScope::Custom("urn:graph:review".into()),
                provenance_iri: Some(author.into()),
                asserted_at_ms: Some(17),
            },
        )
        .unwrap()
        .1
        .statement_id
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
    let edge = graph.to_snapshot().resource_edges.remove(0);
    let mut second = edge.clone();
    let statement = &mut second.semantic.as_mut().unwrap().statements[0];
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
fn session_sidecars_are_inspected_without_fabricating_statements() {
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
    assert!(record.statements.is_empty());
    assert_eq!(record.owner.stratum, GraphStratum::Surface);
    assert_eq!(
        record.families[0],
        HeldFamily {
            family: EdgeFamily::Arrangement,
            data: json!({"sub_kinds": ["SplitPair"]}),
        }
    );
}

#[test]
fn mixed_nonsemantic_data_preserves_events_aggregates_and_family_values() {
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
    assert!(
        inspected
            .records
            .iter()
            .all(|record| record.statements.is_empty())
    );
    let families: Vec<_> = inspected
        .records
        .iter()
        .flat_map(|record| &record.families)
        .collect();
    for (family, value) in [
        (
            EdgeFamily::Containment,
            json!({"sub_kinds": ["NotebookSection"]}),
        ),
        (
            EdgeFamily::Imported,
            json!({"sub_kinds": ["SessionImport"]}),
        ),
        (EdgeFamily::Provenance, json!({"sub_kinds": ["CopiedFrom"]})),
    ] {
        assert_eq!(
            families
                .iter()
                .find(|held| held.family == family)
                .unwrap()
                .data,
            value
        );
    }
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
