// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn host_session_refusal_blocks_save_fork_background_and_survives_graph_replacement() {
    let root = tempfile::tempdir().unwrap();
    let app = App::test_stub_at(root.path().to_path_buf());
    let directory = app.session_dir();
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join(pandect::session_graph_store::GRAPH_FILE);
    std::fs::write(&file, b"{invalid session graph").unwrap();
    let id = app.session_id;
    drop(app);
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let old_count = app.sessions.len();
    let adopted = app.adopt_session(id);
    assert!(
        adopted
            .iter()
            .any(|effect| matches!(effect, Effect::CancelResearchSearch))
    );
    assert!(
        adopted
            .iter()
            .any(|effect| matches!(effect, Effect::Redraw))
    );
    assert!(
        adopted
            .iter()
            .all(|effect| matches!(effect, Effect::CancelResearchSearch | Effect::Redraw))
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
        // The donor already has a directory for its held writer lease.
        .filter(|directory| *directory != app.session_dir())
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
            &cambium::MenuSession::open(Some(super::super::palette::CONTEXT_COMMANDS)),
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
            &cambium::MenuSession::open(Some(super::super::palette::CONTEXT_COMMANDS)),
            10,
        )
        .iter()
        .any(|(label, _)| label == "Retry session")
    );
}
