// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Host controls for session placement, refusal, and publication ordering.
use super::App;
use crate::action::{Action, Effect};
use crate::session;
use crate::session_persistence::SessionPersistence;
use mere::kernel::graph::{Graph, apply::add_node};
use pandect::graph_placement::PlacementProfile;

#[test]
fn host_session_fork_preserves_resource_records_and_tag_assertions() {
    use mere::kernel::graph::{Author, resource_tags::{TagConcept, tag_concept_iri}};
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root.path()));
    let mut graph = Graph::new();
    let member = uuid::Uuid::new_v4();
    let key = add_node(&mut graph, Some(member), "https://fork-content.test/page".into(), Default::default());
    let resource = graph.shown_resource_id(key).unwrap();
    graph.write_as(Author::person("fork-tagger"), |graph| {
        let owner = graph.write_author().asserter_iri();
        graph.tag_resource_with_concept(resource, &tag_concept_iri(&owner, "Paper"),
            TagConcept { owner_iri: owner, label: "Paper".into() }).unwrap();
    });
    let original = graph.to_snapshot();
    app.graph_runtimes.adopt_graph(graph, SessionPersistence::writable(app.session_dir(), None));
    let effects = app.fork_session_from(member);
    let child = effects.iter().find_map(|effect| match effect { Effect::SwitchSession { id } => Some(*id), _ => None }).unwrap();
    let loaded = session::try_load_session_graph(&session::session_dir(root.path(), child)).unwrap().unwrap().graph;
    let (copied, node) = loaded.get_node_by_url("https://fork-content.test/page").unwrap();
    assert_ne!(node.id, member);
    assert!(crate::content_tags::content_tags(&loaded, copied).contains("Paper"));
    let saved = loaded.to_snapshot();
    for record in &original.resources {
        assert!(saved.resources.contains(record), "resource record and owner facets survive: {}", record.canonical_iri);
    }
    for edge in &original.resource_edges {
        let wire = serde_json::to_value(edge).unwrap();
        assert!(saved.resource_edges.iter().any(|saved| serde_json::to_value(saved).unwrap() == wire), "held assertions survive verbatim");
    }
}

#[test]
fn host_session_recorded_and_absent_ordinary_save_reopen_and_replacement() {
    for placement in [Some(PlacementProfile::RecordedStrataV1), None] {
        let mut app = App::test_stub();
        let directory = app.session_dir();
        let mut graph = Graph::new();
        for url in [
            "https://host-placement.test/dir/page",
            "https://host-placement.test/dir/",
        ] {
            add_node(&mut graph, None, url.into(), Default::default());
        }
        let mut snapshot = graph.to_snapshot();
        snapshot.edges.clear();
        snapshot.resource_edges.clear();
        let graph = Graph::try_from_recorded_snapshot(&snapshot).unwrap();
        app.graph_runtimes.adopt_graph(
            graph,
            SessionPersistence::writable(directory.clone(), placement),
        );
        app.persist_session_graph().unwrap();
        let stored = pandect::session_graph_store::load_profiled_snapshot(
            &directory.join(pandect::session_graph_store::GRAPH_FILE),
        )
        .unwrap()
        .unwrap();
        assert_eq!(stored.placement, placement);
        let loaded = session::try_load_session_graph(&directory)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.placement, placement);
        let reopened = loaded.graph.to_snapshot();
        if placement.is_some() {
            assert!(reopened.edges.is_empty() && reopened.resource_edges.is_empty());
        } else {
            assert!(!reopened.edges.is_empty() || !reopened.resource_edges.is_empty());
        }
        app.graph_runtimes.set_graph(Graph::new());
        app.persist_session_graph().unwrap();
        let stored = pandect::session_graph_store::load_profiled_snapshot(
            &directory.join(pandect::session_graph_store::GRAPH_FILE),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            stored.placement, None,
            "unqualified replacement revokes prior authority"
        );
    }
}

#[test]
fn host_session_refusal_blocks_save_fork_background_and_survives_graph_replacement() {
    let mut app = App::test_stub();
    let directory = app.session_dir();
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    std::fs::write(&file, b"{invalid session graph").unwrap();
    let old_count = app.sessions.len();
    let adopted = app.adopt_session(app.session_id);
    assert!(
        adopted
            .iter()
            .all(|effect| matches!(effect, Effect::Redraw))
    );
    assert!(app.session_load_refused());
    app.update(Action::OmnibarOpen { command: true });
    assert!(app.omnibar.open);
    assert!(
        app.available_actions()
            .iter()
            .any(|(label, _)| label == "Retry session")
    );
    assert!(
        app.available_actions()
            .iter()
            .all(|(_, action)| matches!(action, Action::SwitchSession(_) | Action::NewSession))
    );
    let retry = app.update(Action::SwitchSession(app.session_id));
    assert!(
        retry
            .iter()
            .any(|effect| matches!(effect, Effect::SwitchSession { id } if *id == app.session_id))
    );
    assert!(app.persist_session_graph().is_err());
    assert!(
        app.update(Action::SaveSession)
            .iter()
            .all(|effect| matches!(effect, Effect::Redraw))
    );
    assert!(app.tick(10).is_empty());
    let mut graph = Graph::new();
    let seed = uuid::Uuid::new_v4();
    add_node(
        &mut graph,
        Some(seed),
        "https://refusal.test/page".into(),
        Default::default(),
    );
    app.graph_runtimes.set_graph(graph);
    assert!(app.session_load_refused());
    assert!(app.fork_session_from(seed).is_empty());
    assert_eq!(app.sessions.len(), old_count);
    assert_eq!(std::fs::read(&file).unwrap(), b"{invalid session graph");
    pandect::session_graph_store::save_profiled(&file, &Graph::new(), None).unwrap();
    app.adopt_session(app.session_id);
    assert!(
        !app.session_load_refused(),
        "successful explicit retry unlocks persistence"
    );
    app.persist_session_graph().unwrap();
}

#[test]
fn host_session_facet_write_failure_does_not_replace_graph_or_migration_evidence() {
    let mut app = App::test_stub();
    let directory = app.session_dir();
    app.persist_session_graph().unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let original = std::fs::read(&file).unwrap();
    let blocked = pandect::node_facets_path(&directory).with_extension("json.tmp");
    std::fs::create_dir(&blocked).unwrap();
    app.graph_runtimes.visit("https://write-failure.test/page");
    assert!(app.persist_session_graph().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), original);
    std::fs::remove_dir(blocked).unwrap();
    app.persist_session_graph().unwrap();
    assert_ne!(std::fs::read(file).unwrap(), original);
}

#[test]
fn host_session_graph_write_failure_preserves_original_and_returns_failure() {
    let mut app = App::test_stub();
    let directory = app.session_dir();
    app.persist_session_graph().unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let original = std::fs::read(&file).unwrap();
    let blocked = file.with_extension("json.tmp");
    std::fs::create_dir(&blocked).unwrap();
    app.graph_runtimes
        .visit("https://graph-write-failure.test/page");
    assert!(app.persist_session_graph().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), original);
    std::fs::remove_dir(blocked).unwrap();
    app.persist_session_graph().unwrap();
    assert_ne!(std::fs::read(file).unwrap(), original);
}

#[test]
fn host_session_fork_manifest_failure_never_publishes_or_switches() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.graph_runtimes
        .visit("https://fork-publication.test/page");
    let seed = app.graph_runtimes.focused_member().unwrap();
    // The fixture's manifest store deliberately has no publication root.
    // Child graph/facets can persist, but manifest publication must fail.
    assert!(app.sessions.root().is_none());
    let count = app.sessions.len();
    assert!(app.fork_session_from(seed).is_empty());
    assert_eq!(
        app.sessions.len(),
        count,
        "failed child cannot publish on a later flush"
    );
    let children: Vec<_> = std::fs::read_dir(app.data_root.join("sessions"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(children.len(), 1);
    assert!(
        children[0]
            .join(pandect::session_graph_store::GRAPH_FILE)
            .is_file()
    );
    assert!(pandect::node_facets_path(&children[0]).is_file());
    assert!(
        !children[0]
            .join(pandect::manifest_store::MANIFEST_FILE)
            .exists()
    );
}

#[test]
fn host_session_interrupted_replacement_never_overwrites_its_backup() {
    let mut app = App::test_stub();
    let directory = app.session_dir();
    app.graph_runtimes
        .visit("https://interrupted-replacement.test/page");
    app.persist_session_graph().unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let backup = file.with_extension("json.previous");
    let original = std::fs::read(&file).unwrap();
    std::fs::rename(&file, &backup).unwrap();
    app.adopt_session(app.session_id);
    assert!(app.session_load_refused());
    assert!(app.persist_session_graph().is_err());
    assert!(!file.exists());
    assert_eq!(std::fs::read(&backup).unwrap(), original);
    assert!(
        app.available_actions()
            .iter()
            .any(|(label, _)| label == "Retry session")
    );
    assert!(
        app.command_lane(
            &cambium::MenuSession::open(Some(super::palette::CONTEXT_COMMANDS)),
            10,
        )
        .iter()
        .any(|(label, _)| label == "Retry session")
    );
}
