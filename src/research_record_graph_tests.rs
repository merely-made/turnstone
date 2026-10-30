// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use mere::kernel::graph::Graph;

#[test]
fn model_free_graph_binding_survives_save_and_exact_withdrawal() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("graph.json");
    let mut graph = Graph::new();
    let (journal, hook) = mere::kernel::graph::journal_capture_hook();
    graph.set_recorder(Some(hook.clone()));
    let node = mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(uuid::Uuid::new_v4()),
        "https://example.test/entity".into(),
        Default::default(),
    );
    let entity = graph.shown_resource_id(node).unwrap();
    let one = eidetic::ManifestId::of_blob(b"one");
    let two = eidetic::ManifestId::of_blob(b"two");
    let before = journal.lock().unwrap().len();
    let first = attach(&mut graph, entity, one).unwrap();
    assert!(journal.lock().unwrap().len() > before);
    assert_eq!(attach(&mut graph, entity, one).unwrap(), first);
    let second = attach(&mut graph, entity, two).unwrap();
    assert_ne!(first.statement_id, second.statement_id);
    crate::session_graph_file::save(&path, &graph).unwrap();
    let mut graph = crate::session_graph_file::load(&path).unwrap().unwrap();
    graph.set_recorder(Some(hook));
    let held = bindings(&graph, false).unwrap();
    assert_eq!(held.len(), 2);
    assert!(held.contains(&first));
    assert!(held.contains(&second));
    let before_withdraw = journal.lock().unwrap().len();
    assert!(withdraw_owned(&mut graph, &first.statement_id).unwrap());
    assert!(journal.lock().unwrap().len() > before_withdraw);
    crate::session_graph_file::save(&path, &graph).unwrap();
    let graph = crate::session_graph_file::load(&path).unwrap().unwrap();
    assert_eq!(bindings(&graph, false).unwrap(), vec![second.clone()]);
    assert_eq!(bindings(&graph, true).unwrap().len(), 2);
    assert!(
        graph
            .find_assertion(&first.statement_id)
            .unwrap()
            .1
            .withdrawn
            .is_some()
    );
    assert!(graph.find_held_assertion(&second.statement_id).is_some());
}

#[test]
fn model_free_recovery_restores_exact_statement_without_replacing_existing_assertions() {
    let mut original = Graph::new();
    let node = mere::kernel::graph::apply::add_node(
        &mut original,
        Some(uuid::Uuid::new_v4()),
        "https://example.test/recovery".into(),
        Default::default(),
    );
    let entity = original.shown_resource_id(node).unwrap();
    let before = original.clone();
    let first = attach(
        &mut original,
        entity,
        eidetic::ManifestId::of_blob(b"original"),
    )
    .unwrap();
    let assertion = assertion_for(&original, &first).unwrap();
    let mut reopened = before;
    let other = attach(
        &mut reopened,
        entity,
        eidetic::ManifestId::of_blob(b"other"),
    )
    .unwrap();
    restore(&mut reopened, &first, &assertion, false).unwrap();
    let current = bindings(&reopened, false).unwrap();
    assert_eq!(current.len(), 2);
    assert!(current.contains(&first));
    assert!(current.contains(&other));
    assert_eq!(assertion_for(&reopened, &first).unwrap(), assertion);
    let mut conflicting = assertion.clone();
    conflicting["predicate"] = serde_json::json!("urn:wrong");
    assert!(restore(&mut reopened, &first, &conflicting, false).is_err());
}

#[test]
fn model_free_graph_enumeration_refuses_limits_and_reserved_surface_tombstones() {
    let mut graph = Graph::new();
    let node = mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(uuid::Uuid::new_v4()),
        "https://example.test/limited".into(),
        Default::default(),
    );
    let entity = graph.shown_resource_id(node).unwrap();
    attach(&mut graph, entity, eidetic::ManifestId::of_blob(b"held")).unwrap();
    let mut settings = eidetic::records::RecordSettings::default();
    settings.max_work = 0;
    assert!(bindings_limited(&graph, false, &mut Budget::new(&settings)).is_err());

    let mut graph = Graph::new();
    let node = mere::kernel::graph::apply::add_node(
        &mut graph,
        Some(uuid::Uuid::new_v4()),
        "https://example.test/surface".into(),
        Default::default(),
    );
    graph
        .declare_predicate(PREDICATE, mere::kernel::graph::GraphStratum::Surface)
        .unwrap();
    let spec = mere::kernel::graph::SemanticStatementSpec {
        predicate: PREDICATE.into(),
        recognized_sub_kind: None,
        label: None,
        basis: AssertionBasis {
            note: None,
            method: Some(AssertionMethod {
                iri: METHOD.into(),
                version: Some("1".into()),
            }),
            evidence: vec![format!(
                "{}{}",
                REFERENCE,
                eidetic::ManifestId::of_blob(b"wrong")
            )],
        },
        graph_scope: GraphScope::Default,
        provenance_iri: None,
        asserted_at_ms: None,
    };
    let (_, asserted) = graph
        .try_assert_semantic_statement(node, node, spec)
        .unwrap();
    assert!(graph.withdraw_semantic_assertion(&asserted.statement_id, None, None));
    assert!(
        bindings_limited(
            &graph,
            false,
            &mut Budget::new(&eidetic::records::RecordSettings::default())
        )
        .is_err()
    );
}
