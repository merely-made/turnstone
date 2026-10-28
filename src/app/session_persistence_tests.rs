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
fn refused_session_lease_clears_the_departed_sessions_live_state() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let first = app.session_id;
    let first_graph = crate::panes::GraphId::from_uuid(*first.as_uuid());
    app.sessions
        .insert(pandect::GraphSessionManifest::new(first, first_graph));
    app.adopt_session(first);

    let departed_node = uuid::Uuid::new_v4();
    app.graph_runtimes
        .get_mut(first_graph)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            add_node(
                graph,
                Some(departed_node),
                "turnstone://departed-session".into(),
                Default::default(),
            );
            true
        });
    app.content.note_requested(departed_node);
    app.place = crate::place::PlaceState::Joining { generation: 44 };
    app.resident_run_error = Some("departed session's unresolved receipt".into());
    app.research.set_available(true);
    app.research.project(crate::research::Context {
        session: first,
        generation: uuid::Uuid::new_v4(),
        admissions: Vec::new(),
        clips: Vec::new(),
    });
    assert_eq!(app.research.context().unwrap().session, first);

    let blocked = crate::panes::SessionId::new();
    let blocked_graph = crate::panes::GraphId::from_uuid(*blocked.as_uuid());
    app.sessions
        .insert(pandect::GraphSessionManifest::new(blocked, blocked_graph));
    let blocked_dir = session::session_dir(root.path(), blocked);
    let _other_writer = crate::session_persistence::SessionWriteLease::acquire(&blocked_dir)
        .expect("another owner holds the target session lease");

    let effects = app.adopt_session(blocked);

    assert!(effects.iter().any(|effect| matches!(effect, Effect::Redraw)));
    assert!(app.session_load_refused());
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Failed { ref error }
            if error.contains("session writer lease refused")
    ));
    assert_eq!(app.content.get(departed_node), None);
    assert!(app
        .resident_run_error
        .as_deref()
        .is_some_and(|error| error.contains("session writer lease refused")));
    assert!(app.research.is_available());
    assert!(
        app.research.context().is_err(),
        "a refused session cannot leave the departed session's research context admitted"
    );
    assert!(app.graph_runtimes.graph().get_node_by_id(departed_node).is_none());
}

#[test]
fn writer_lease_survives_same_session_readoption_and_switch_back() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let first = app.session_id;
    let first_graph = crate::panes::GraphId::from_uuid(*first.as_uuid());
    app.sessions
        .insert(pandect::GraphSessionManifest::new(first, first_graph));
    app.adopt_session(first);
    let member = uuid::Uuid::new_v4();
    app.graph_runtimes
        .get_mut(first_graph)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            add_node(
                graph,
                Some(member),
                "turnstone://lease-retention".into(),
                Default::default(),
            );
            true
        });
    app.adopt_session(first);
    assert!(
        app.graph_runtimes.graph().get_node_by_id(member).is_some(),
        "same-session adoption keeps unsaved live graph state"
    );

    let second = crate::panes::SessionId::new();
    let second_graph = crate::panes::GraphId::from_uuid(*second.as_uuid());
    app.sessions
        .insert(pandect::GraphSessionManifest::new(second, second_graph));
    app.adopt_session(second);
    app.adopt_session(first);
    assert!(
        app.graph_runtimes.graph().get_node_by_id(member).is_some(),
        "switch-back reuses the graph and its held writer lease"
    );
    assert!(app.persist_owned_graph().is_ok());
}

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
    app.graph_runtimes.adopt_graph(Graph::new(), SessionPersistence::Unbound);
    app.graph_runtimes.adopt_graph(graph, SessionPersistence::writable(app.session_dir(), None).unwrap());
    let effects = app.fork_session_from(member);
    let child = effects.iter().find_map(|effect| match effect { Effect::SwitchSession { id } => Some(*id), _ => None }).unwrap();
    let name = app.sessions.get(child).and_then(|m| m.display_name.clone()).unwrap();
    assert!(name.contains(" (picked from "), "a torn-out session is named as a picked-off part: {name}");
    let loaded = session::try_load_session_graph_locked(&session::session_dir(root.path(), child)).unwrap().unwrap().graph;
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
fn new_session_records_its_birth_and_reopens_same_site_relations_exactly() {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root.path()));
    let effects = app.update(Action::NewSession);
    let id = effects.iter().find_map(|effect| match effect {
        Effect::SwitchSession { id } => Some(*id),
        _ => None,
    }).expect("new session requests ordinary adoption");
    app.adopt_session(id);
    for url in [
        "mere://alpha.example/one", "mere://alpha.example/two",
        "mere://beta.example/one", "mere://beta.example/two",
        "mere://gamma.example/one", "mere://gamma.example/two",
    ] {
        app.graph_runtimes.visit(url);
    }
    let expected = app.graph_runtimes.graph().to_snapshot_at(0);
    assert!(expected.edges.is_empty(), "fresh siblings have no Surface containment");
    assert_eq!(expected.nodes.len(), 6);
    assert_eq!(expected.resources.len(), 6);
    assert_eq!(expected.resource_edges.len(), 5);
    let legacy = Graph::try_from_snapshot(&expected).unwrap().to_snapshot_at(0);
    assert_eq!(legacy.edges.len(), 3,
        "the unprofiled control must expose the reopen-only Domain derivation");

    app.persist_session_graph().unwrap();
    let directory = app.session_dir();
    let path = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let stored = crate::session_graph_file::load_profiled_snapshot(&path)
        .unwrap().unwrap();
    assert_eq!(stored.placement, Some(PlacementProfile::RecordedStrataV1));
    let bytes = std::fs::read(&path).unwrap();

    let mut reopened = App::test_stub_at(root.path().to_path_buf());
    reopened.sessions.load_from_disk(session::sessions_root(root.path())).unwrap();
    reopened.adopt_session(id);
    assert_eq!(
        serde_json::to_value(reopened.graph_runtimes.graph().to_snapshot_at(0)).unwrap(),
        serde_json::to_value(&expected).unwrap(),
        "fresh application adoption preserves every recorded graph column",
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes,
        "adoption does not rewrite the recorded graph");
    reopened.persist_session_graph().unwrap();
    let saved = crate::session_graph_file::load_profiled_snapshot(&path)
        .unwrap().unwrap();
    assert_eq!(saved.placement, Some(PlacementProfile::RecordedStrataV1));
    let mut actual = saved.snapshot;
    actual.timestamp_secs = 0;
    assert_eq!(serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap(),
        "ordinary re-save preserves all source facts and recorded placement");
}

#[test]
fn host_session_recorded_and_absent_ordinary_save_reopen_and_replacement() {
    for placement in [Some(PlacementProfile::RecordedStrataV1), None] {
        let mut app = App::test_stub();
        let directory = app.session_dir();
        app.graph_runtimes.adopt_graph(Graph::new(), SessionPersistence::Unbound);
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
            SessionPersistence::writable(directory.clone(), placement).unwrap(),
        );
        app.persist_session_graph().unwrap();
        let stored = crate::session_graph_file::load_profiled_snapshot(
            &directory.join(pandect::session_graph_store::GRAPH_FILE),
        )
        .unwrap()
        .unwrap();
        assert_eq!(stored.placement, placement);
        let loaded = session::try_load_session_graph_locked(&directory)
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
        let stored = crate::session_graph_file::load_profiled_snapshot(
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
    crate::session_graph_file::save_profiled(&file, &Graph::new(), None).unwrap();
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
    let blocked = pandect::node_facets_path(&directory).to_path_buf();
    let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
    app.graph_runtimes.visit("https://write-failure.test/page");
    assert!(app.persist_session_graph().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), original);
    blocked.restore();
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
    let blocked = file.to_path_buf();
    let blocked = crate::session_persistence::BlockedReplacement::new(&blocked);
    app.graph_runtimes
        .visit("https://graph-write-failure.test/page");
    assert!(app.persist_session_graph().is_err());
    assert_eq!(blocked.read_preserved(&file).unwrap(), original);
    blocked.restore();
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

#[test]
fn host_session_interrupted_unique_temporary_preserves_evidence_and_refuses_save() {
    let mut app = App::test_stub();
    let directory = app.session_dir();
    app.graph_runtimes
        .visit("https://interrupted-replacement.test/page");
    app.persist_session_graph().unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    let backup = file.with_extension("json.42.7.tmp");
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
