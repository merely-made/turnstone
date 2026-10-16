// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::app::GraphshellApp;
use crate::mere_host::{
    FIXTURE_PERSONA_ADDRESS, LINK_REST_A1_ADDRESS, LINK_REST_B1_ADDRESS, LINK_REST_B2_ADDRESS,
    SelectedPersonaRef,
};
use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
use mere::kernel::graph::{
    ContainmentSubKind, Graph as KernelGraph, GraphStratum, RelationKind, SemanticStatement,
};
use mere::kernel::types::{AssertionBasis, AssertionMethod, GraphScope};
use muniment::MemoryBackend;
use scenograph::link::{LinkKey, LinkSource, LinkStratum};
use std::collections::BTreeSet;

const PRIMARY: &str = "fixture:p7:collection-primary";
const INDEPENDENT: &str = "fixture:p7:collection-independent";

#[test]
fn simultaneous_metadata_change_and_withdrawal_keep_both_drift_signals() {
    let (_backend, mut app, key, first, second) = fixture();
    let source = app.host.link_source();
    let owner = freeze(&mut app, &key, &source, Uuid::from_u128(0x7040)).unwrap();
    app.host
        .mutate_product_graph(|graph| {
            assert!(graph.reconfirm_semantic_assertion(
                PRIMARY,
                Some("changed before withdrawal".into()),
                Some(900)
            ));
            assert!(graph.withdraw_semantic_assertion(PRIMARY, Some(901), None));
        })
        .unwrap();
    let inspection = inspect(app.host.graph(), owner, &source).unwrap();
    assert_eq!(inspection.drift.withdrawn, vec![PRIMARY.to_owned()]);
    assert_eq!(inspection.drift.changed, vec![PRIMARY.to_owned()]);
    let mut invalid = serde_json::to_value(&inspection.frozen).unwrap();
    let duplicate = invalid["statements"][0]["assertions"][0].clone();
    invalid["statements"][0]["assertions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    let invalid: FrozenLink = serde_json::from_value(invalid).unwrap();
    assert_eq!(compare(&inspection.frozen, &invalid).status, "unavailable");
    let _ = (first, second);
}

fn selected_persona() -> SelectedPersonaRef {
    SelectedPersonaRef {
        persona: FIXTURE_PERSONA_ADDRESS.into(),
        profile: "profile:graphshell-p7-promoted-link-tests".into(),
    }
}

fn fixture() -> (
    MemoryBackend,
    GraphshellApp<MemoryBackend>,
    LinkKey,
    Uuid,
    Uuid,
) {
    let backend = MemoryBackend::new();
    let mut app = GraphshellApp::fixture(backend.clone(), selected_persona()).expect("fixture app");
    app.host
        .seed_link_promotion_fixture()
        .expect("seed P7 assertions through the current host journal");
    let graph = app.host.graph();
    let first = graph
        .get_node_by_url(LINK_REST_A1_ADDRESS)
        .expect("A1 source")
        .1
        .id;
    let second = graph
        .get_node_by_url(LINK_REST_B1_ADDRESS)
        .expect("B1 target")
        .1
        .id;
    let key = LinkKey::new(LinkStratum::Surface, first.to_string(), second.to_string());
    (backend, app, key, first, second)
}

fn target(
    source: LinkSource,
    link: LinkKey,
    from: Uuid,
    to: Uuid,
    id: &str,
) -> cambium::LinkMemberTarget {
    cambium::LinkMemberTarget {
        source,
        link,
        from: from.to_string(),
        to: to.to_string(),
        assertion_id: id.into(),
    }
}

fn retained_ids(link: &mere::kernel::graph::FrozenLink) -> BTreeSet<String> {
    link.statements()
        .iter()
        .flat_map(|edge| edge.retained_assertions())
        .map(|claim| claim.statement_id.clone())
        .collect()
}

#[test]
fn freezes_independent_resource_reopens_and_keeps_copy_after_exact_withdrawal() {
    pollster::block_on(async {
        let (backend, mut app, key, first, second) = fixture();
        let source = app.host.link_source();
        let owner = freeze(&mut app, &key, &source, Uuid::from_u128(0x7001)).unwrap();
        let independent_freeze = freeze(&mut app, &key, &source, Uuid::from_u128(0x7002)).unwrap();
        assert_ne!(owner, independent_freeze);
        assert_eq!(list(app.host.graph()).len(), 2);

        let before = inspect(app.host.graph(), owner, &source).unwrap();
        let frozen_copy = before.frozen.clone();
        assert_eq!(before.drift.status, "unchanged");
        assert_eq!(
            retained_ids(&frozen_copy),
            BTreeSet::from([PRIMARY.to_string(), INDEPENDENT.to_string()])
        );
        assert_eq!(frozen_copy.stratum(), GraphStratum::Surface);

        comment(
            &mut app,
            owner,
            "review stays on this promoted Resource",
            Uuid::from_u128(0x7003),
            700,
        )
        .unwrap();
        assert_eq!(
            inspect(app.host.graph(), owner, &source)
                .unwrap()
                .comments
                .len(),
            1
        );
        assert!(
            inspect(app.host.graph(), independent_freeze, &source)
                .unwrap()
                .comments
                .is_empty()
        );

        let primary = target(source.clone(), key.clone(), first, second, PRIMARY);
        assert!(can_withdraw(&app, &primary));
        withdraw(&mut app, &primary, 710).unwrap();
        assert!(app.host.graph().find_held_assertion(PRIMARY).is_none());
        assert!(app.host.graph().find_held_assertion(INDEPENDENT).is_some());

        let after = inspect(app.host.graph(), owner, &source).unwrap();
        assert_eq!(after.drift.status, "changed");
        assert_eq!(after.drift.withdrawn, vec![PRIMARY.to_string()]);
        assert_eq!(
            after.frozen, frozen_copy,
            "live withdrawal cannot mutate the frozen copy"
        );
        assert_eq!(after.comments.len(), 1);

        app.host
            .persist(71)
            .await
            .expect("persist source and promotions");
        let reopened = GraphshellApp::open_or_fixture(backend, selected_persona())
            .await
            .expect("reopen actual GraphSession journal");
        assert!(reopened.host.graph().find_held_assertion(PRIMARY).is_none());
        assert!(
            reopened
                .host
                .graph()
                .find_held_assertion(INDEPENDENT)
                .is_some()
        );
        let reopened_primary = inspect(reopened.host.graph(), owner, &reopened.host.link_source())
            .expect("reopen promoted Resource");
        assert_eq!(reopened_primary.frozen, frozen_copy);
        assert_eq!(reopened_primary.drift.withdrawn, vec![PRIMARY.to_string()]);
        assert_eq!(reopened_primary.comments.len(), 1);
        assert!(
            inspect(
                reopened.host.graph(),
                independent_freeze,
                &reopened.host.link_source()
            )
            .unwrap()
            .comments
            .is_empty()
        );
    });
}

#[test]
fn exact_withdrawal_refuses_stale_source_foreign_method_wrong_pair_and_unknown_id() {
    let (_backend, mut app, key, first, second) = fixture();
    let source = app.host.link_source();
    let generated_id = "fixture:p7:method-generated";
    app.host
        .mutate_product_graph(|graph| {
            let from = graph.get_node_key_by_id(first).unwrap();
            let to = graph.get_node_key_by_id(second).unwrap();
            let predicate = RelationKind::Containment(ContainmentSubKind::CollectionMember)
                .predicate_iri()
                .unwrap()
                .to_owned();
            graph
                .try_assert_persisted_relation_statement(
                    from,
                    to,
                    SemanticStatement {
                        statement_id: "fixture:p7:method-generated".into(),
                        predicate,
                        recognized_sub_kind: None,
                        label: Some("method-backed claim".into()),
                        basis: AssertionBasis {
                            note: Some("computed claim".into()),
                            method: Some(AssertionMethod {
                                iri: "urn:graphshell:test:method".into(),
                                version: Some("1".into()),
                            }),
                            evidence: vec!["urn:graphshell:test:evidence".into()],
                        },
                        graph_scope: GraphScope::User,
                        provenance_iri: Some(graph.write_author().asserter_iri()),
                        first_asserted_ms: Some(12),
                        asserted_at_ms: Some(12),
                        withdrawn: None,
                    },
                )
                .expect("insert method-backed assertion for the refusal control");
        })
        .unwrap();

    let mut stale_source = source.clone();
    if let LinkSource::Available { binding, .. } = &mut stale_source {
        binding.resource.push_str(":stale");
    }
    let other = app
        .host
        .graph()
        .get_node_by_url(LINK_REST_B2_ADDRESS)
        .expect("B2 endpoint")
        .1
        .id;
    let wrong_pair_key = LinkKey::new(LinkStratum::Surface, first.to_string(), other.to_string());
    let candidates = [
        target(stale_source, key.clone(), first, second, PRIMARY),
        target(source.clone(), wrong_pair_key, first, second, PRIMARY),
        target(
            source.clone(),
            key.clone(),
            first,
            second,
            "fixture:p7:unknown-id",
        ),
        target(source.clone(), key.clone(), first, second, generated_id),
    ];
    let before = serde_json::to_value(app.host.graph().to_snapshot()).unwrap();
    let journal_len = app.host.graph_session().journal().entries().len();
    for candidate in &candidates {
        assert!(!can_withdraw(&app, candidate));
        assert!(withdraw(&mut app, candidate, 800).is_err());
    }
    assert_eq!(
        serde_json::to_value(app.host.graph().to_snapshot()).unwrap(),
        before
    );
    assert_eq!(
        app.host.graph_session().journal().entries().len(),
        journal_len
    );

    let mut another_owner = source.clone();
    if let LinkSource::Available { binding, .. } = &mut another_owner {
        binding.resource.push_str(":other-owner");
    }
    let promoted = freeze(&mut app, &key, &source, Uuid::from_u128(0x7010)).unwrap();
    let other_reading = inspect(app.host.graph(), promoted, &another_owner).unwrap();
    assert_eq!(other_reading.drift.status, "unavailable");
    assert!(other_reading.drift.reason.is_some());
}

#[test]
fn drift_distinguishes_added_changed_missing_and_receiver_coverage() {
    let (_backend, mut app, key, first, second) = fixture();
    let source = app.host.link_source();
    let owner = freeze(&mut app, &key, &source, Uuid::from_u128(0x7020)).unwrap();
    let first_key = app.host.graph().get_node_key_by_id(first).unwrap();
    let second_key = app.host.graph().get_node_key_by_id(second).unwrap();
    app.host
        .mutate_product_graph(|graph| {
            let (_, original) = graph.find_held_assertion(PRIMARY).unwrap();
            let mut edited = original.clone();
            edited.label = Some("changed since promotion".into());
            edited.basis.note = Some("changed metadata".into());
            graph
                .try_assert_persisted_relation_statement(first_key, second_key, edited)
                .expect("same exact ID may receive an intentional metadata update");

            let (_, sibling) = graph.find_held_assertion(INDEPENDENT).unwrap();
            let mut added = sibling.clone();
            added.statement_id = "fixture:p7:drift-added".into();
            added.label = Some("added after promotion".into());
            added.basis.note = Some("new independent reason".into());
            added.basis.evidence = vec!["urn:graphshell:p7:drift-added-evidence".into()];
            graph
                .try_assert_persisted_relation_statement(first_key, second_key, added)
                .expect("add an exact-ID assertion after promotion");

            let mut edges = graph.persisted_edges_between(first_key, second_key);
            for edge in &mut edges {
                if let Some(assertions) = edge.assertions.as_mut() {
                    assertions.retain(|claim| claim.statement_id != INDEPENDENT);
                }
            }
            apply_graph_delta(
                graph,
                GraphDelta::ReplaySetEdgesByIds {
                    from_id: first,
                    to_id: second,
                    edges,
                },
            );
        })
        .unwrap();

    let drift = inspect(app.host.graph(), owner, &source).unwrap().drift;
    assert_eq!(drift.status, "changed");
    assert_eq!(drift.changed, vec![PRIMARY.to_string()]);
    assert_eq!(drift.added, vec!["fixture:p7:drift-added".to_string()]);
    assert_eq!(drift.missing, vec![INDEPENDENT.to_string()]);

    let mut receiver_snapshot = KernelGraph::new().to_snapshot();
    let promoted_record = app.host.graph().resource_record(owner).unwrap();
    receiver_snapshot.resources = vec![promoted_record];
    let receiver = KernelGraph::try_from_snapshot(&receiver_snapshot)
        .expect("receiver admits the standalone promoted Resource facet");
    assert_eq!(receiver.resource_nodes().count(), 1);
    let opened = receiver.open_frozen_link(owner).unwrap();
    assert!(opened.coverage.limits.iter().any(|limit| {
        limit.layer == mere::kernel::graph::CoverageLayer::Possession
            && limit.reason == "referenced surfaces are unavailable"
            && limit.surfaces.len() == 2
    }));
    assert_eq!(
        inspect(&receiver, owner, &source).unwrap().drift.status,
        "unavailable",
        "missing endpoints are not reported as no current assertions"
    );
}
