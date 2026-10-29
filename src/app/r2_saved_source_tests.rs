// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Private corpus admission and opening-generation controls.

use super::App;
use crate::action::{Action, Effect, Update};
use crate::saved_sources::{Admission, Content, Entry, Key, Selector, Target};
use crate::{session, source_capture};
use chartulary::FacetId;
use mere::canvas::Canvas;
use std::path::Path;

fn app_at(root: &Path) -> App {
    let mut app = App::test_stub_at(root.to_path_buf());
    app.sessions = pandect::ManifestStore::with_root(session::sessions_root(root));
    let id = app
        .update(Action::NewSession)
        .into_iter()
        .find_map(|effect| match effect {
            Effect::SwitchSession { id } => Some(id),
            _ => None,
        })
        .expect("a new test session is persisted before adoption");
    app.adopt_session(id);
    app
}

fn visit(app: &mut App, url: &str) -> uuid::Uuid {
    let key = app.graph_runtimes.visit(url);
    app.graph_runtimes.graph().get_node(key).unwrap().id
}

fn attach(app: &mut App, graph: crate::panes::GraphId, node: uuid::Uuid, manifest: &str) {
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph).unwrap().canvas,
        |facets| source_capture::append_reference(facets, node, manifest.to_owned()),
    )
    .unwrap()
    .unwrap();
}

fn remove_refs(app: &mut App, graph: crate::panes::GraphId, node: uuid::Uuid) {
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph).unwrap().canvas,
        |facets| {
            facets.remove(
            &node,
            &FacetId::new(source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
            )
        },
    )
    .unwrap();
}

fn target(key: Key, text: &str) -> (Target, Content) {
    let content = Content {
        title: "Captured page title".into(),
        origin: "https://origin-gone.example/source".into(),
        captured_at_ms: 1_700_000_000_000,
        text: text.to_owned(),
        text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
        raw_manifest: "blake3:raw".into(),
    };
    let selected = text.find("passage").unwrap();
    let start = text[..selected].chars().count();
    let target = Target {
        key,
        text_hash: content.text_hash.clone(),
        selector: Some(Selector {
            start,
            end: start + "passage".chars().count(),
        }),
    };
    (target, content)
}

#[test]
fn admissions_follow_live_session_graph_facets_and_preserve_occurrences() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let root_graph = app.graph_runtimes.active_graph();
    let root_node = visit(&mut app, "https://saved-source.test/root");
    attach(&mut app, root_graph, root_node, "blake3:root-capture");
    attach(&mut app, root_graph, root_node, "blake3:root-capture");

    let same_session_graph = crate::panes::GraphId::new();
    app.graph_runtimes
        .activate_or_insert(same_session_graph, Some(app.session_id), Canvas::new());
    // This auxiliary in-memory graph shares the SessionId for admission
    // filtering, but does not own the session's one durable graph writer.
    let second_graph_node = visit(&mut app, "https://saved-source.test/second-graph");
    attach(
        &mut app,
        same_session_graph,
        second_graph_node,
        "blake3:second-graph-capture",
    );

    let foreign_session = crate::panes::SessionId::new();
    let foreign_graph = crate::panes::GraphId::new();
    app.graph_runtimes
        .activate_or_insert(foreign_graph, Some(foreign_session), Canvas::new());
    let foreign_node = visit(&mut app, "https://saved-source.test/foreign");
    attach(
        &mut app,
        foreign_graph,
        foreign_node,
        "blake3:foreign-capture",
    );

    app.graph_runtimes.activate(root_graph);
    let admissions = app.saved_source_admissions();
    let root_entries: Vec<_> = admissions
        .iter()
        .filter(|entry| entry.key.graph == root_graph && entry.key.node == root_node)
        .collect();
    assert_eq!(
        root_entries.len(),
        2,
        "duplicate references retain occurrences"
    );
    assert_eq!(root_entries[0].occurrence, 0);
    assert_eq!(root_entries[1].occurrence, 1);
    assert!(admissions.iter().any(|entry| {
        entry.key.graph == same_session_graph && entry.key.node == second_graph_node
    }));
    assert!(
        !admissions
            .iter()
            .any(|entry| entry.key.session == foreign_session)
    );
    assert!(!admissions.iter().any(|entry| {
        entry.key.graph == root_graph && entry.key.annotation_manifest == "blake3:unreferenced"
    }));
}

#[test]
fn quiet_saved_source_load_keeps_the_omnibar_closed_for_workshop_choices() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    assert!(!app.omnibar.open);

    let effect = app.begin_saved_source_load();

    let (generation, request, session, admissions) = match effect {
        Effect::LoadSavedSources {
            generation,
            request,
            session,
            admissions,
        } => (generation, request, session, admissions),
        _ => panic!("quiet saved source load did not produce LoadSavedSources"),
    };
    assert!(!app.omnibar.open);
    assert_eq!(session, app.session_id);
    assert_eq!(generation, app.source_capture_generation);
    assert_eq!(app.saved_sources.request, Some(request));
    assert_eq!(app.saved_sources.admissions, admissions);
    assert_eq!(app.saved_sources.status, "Loading");
}

#[test]
fn asynchronous_corpus_answers_are_rejected_after_generation_or_facets_change() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let node = visit(&mut app, "https://saved-source.test/current");
    attach(&mut app, graph, node, "blake3:attached");

    let effects = app.update(Action::SearchSavedSources);
    let (generation, request, session, admissions) = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::LoadSavedSources {
                generation,
                request,
                session,
                admissions,
            } => Some((generation, request, session, admissions)),
            _ => None,
        })
        .expect("search dispatches exact current admissions");
    let old_admission = admissions[0].clone();

    app.source_capture_generation = uuid::Uuid::new_v4();
    let old_generation_effects = app.apply_update(Update::SavedSourcesLoaded {
        generation,
        request,
        session,
        admissions: admissions.clone(),
        result: Ok(Vec::new()),
    });
    assert!(old_generation_effects.is_empty());
    assert_eq!(app.saved_sources.request, Some(request));
    assert!(app.saved_sources.entries.is_empty());

    // Start a new current-generation request, then remove the reference before
    // its reply. The late corpus cannot restore the detached source.
    let effects = app.update(Action::SearchSavedSources);
    let (generation, request, session, admissions) = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::LoadSavedSources {
                generation,
                request,
                session,
                admissions,
            } => Some((generation, request, session, admissions)),
            _ => None,
        })
        .unwrap();
    remove_refs(&mut app, graph, node);
    let detached_facet_effects = app.apply_update(Update::SavedSourcesLoaded {
        generation,
        request,
        session,
        admissions,
        result: Ok(vec![Entry {
            admission: old_admission,
            content: None,
            error: Some("late stale row".into()),
        }]),
    });
    assert!(!detached_facet_effects.iter().any(|effect| matches!(
        effect,
        Effect::ReadSavedSource { .. }
            | Effect::SpawnContent { .. }
            | Effect::FetchPage { .. }
            | Effect::FetchFeed { .. }
            | Effect::FetchFavicon { .. }
    )));
    assert!(detached_facet_effects.contains(&Effect::Redraw));
    assert_eq!(app.saved_sources.request, None);
    assert!(app.saved_sources.entries.is_empty());
    assert!(app.saved_sources.status.contains("references changed"));
}

#[test]
fn exact_read_is_local_and_detaching_reference_withdraws_the_retained_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let source_graph = app.graph_runtimes.active_graph();
    let source_node = visit(&mut app, "https://saved-source.test/source");
    let manifest = "blake3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    attach(&mut app, source_graph, source_node, manifest);
    let admission = app.saved_source_admissions().remove(0);
    let key = admission.key.clone();
    let (target, content) = target(key.clone(), "A 🐦 selected passage remains offline.");
    let saved_address = crate::saved_sources::address(&target).unwrap();
    let reader_node = visit(&mut app, &saved_address);

    let effects = app.request_saved_source_read(reader_node, saved_address.clone(), target.clone());
    let (generation, request, dispatched_target, dispatched_admission) = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::ReadSavedSource {
                generation,
                request,
                target,
                admission,
                ..
            } => Some((*generation, *request, target.clone(), admission.clone())),
            _ => None,
        })
        .expect("saved source has an exact local read effect");
    assert_eq!(dispatched_target, target);
    assert_eq!(dispatched_admission, admission);
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFeed { .. } | Effect::FetchFavicon { .. }
    )));

    let result_effects = app.apply_update(Update::SavedSourceRead {
        generation,
        request,
        node: reader_node,
        url: saved_address,
        target: dispatched_target,
        result: Ok(content),
    });
    assert!(result_effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node, .. } if *node == reader_node
    )));
    assert!(app.saved_readers[&reader_node].content.is_some());

    remove_refs(&mut app, source_graph, source_node);
    let withdrawal = app.invalidate_saved_readers();
    assert!(withdrawal.iter().any(|effect| matches!(
        effect,
        Effect::CloseContent { node } if *node == reader_node
    )));
    assert!(app.saved_readers[&reader_node].content.is_none());
    assert!(
        app.saved_readers[&reader_node]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("no longer available"))
    );

    let wrong_session = Target {
        key: Key {
            session: crate::panes::SessionId::new(),
            ..key
        },
        text_hash: target.text_hash,
        selector: target.selector,
    };
    let rejected = app.request_saved_source_read(
        reader_node,
        "mere://saved-source/foreign".into(),
        wrong_session,
    );
    assert!(
        !rejected
            .iter()
            .any(|effect| matches!(effect, Effect::ReadSavedSource { .. }))
    );
}

#[test]
fn retargeted_or_old_generation_read_replies_cannot_install_saved_text() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let source_node = visit(&mut app, "https://saved-source.test/retarget-source");
    let annotation = eidetic::Hash::of(b"retarget-capture").to_string();
    attach(&mut app, graph, source_node, &annotation);
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, content) = target(key.clone(), "Before 🐦 after the retained passage.");
    let saved_url = crate::saved_sources::address(&target).unwrap();
    let reader_node = visit(&mut app, &saved_url);

    let first = app.request_saved_source_read(reader_node, saved_url.clone(), target.clone());
    let (request, generation) = first
        .iter()
        .find_map(|effect| match effect {
            Effect::ReadSavedSource {
                request,
                generation,
                ..
            } => Some((*request, *generation)),
            _ => None,
        })
        .unwrap();
    let away = "https://ordinary.test/away";
    app.commit_content_navigation(reader_node, away.into());
    remove_refs(&mut app, graph, source_node);
    let late = app.apply_update(Update::SavedSourceRead {
        generation,
        request,
        node: reader_node,
        url: saved_url.clone(),
        target: target.clone(),
        result: Ok(content.clone()),
    });
    assert!(
        !late
            .iter()
            .any(|effect| matches!(effect, Effect::SpawnContent { .. }))
    );
    assert!(
        !app.saved_readers.contains_key(&reader_node),
        "the superseded reader's pending request is discarded"
    );
    assert_eq!(
        app.graph_runtimes
            .graph_containing_member(reader_node)
            .and_then(|graph| app.graph_runtimes.canvas(graph))
            .and_then(|canvas| canvas.graph().get_node_by_id(reader_node))
            .map(|(_, node)| node.url()),
        Some(away)
    );

    // Restore the live attachment and dispatch a fresh request. Its response
    // still belongs to the prior opening generation and must be ignored.
    attach(&mut app, graph, source_node, &key.annotation_manifest);
    app.commit_content_navigation(reader_node, saved_url.clone());
    let second = app.request_saved_source_read(reader_node, saved_url.clone(), target.clone());
    let (request, old_generation) = second
        .iter()
        .find_map(|effect| match effect {
            Effect::ReadSavedSource {
                request,
                generation,
                ..
            } => Some((*request, *generation)),
            _ => None,
        })
        .unwrap();
    app.source_capture_generation = uuid::Uuid::new_v4();
    let stale_generation = app.apply_update(Update::SavedSourceRead {
        generation: old_generation,
        request,
        node: reader_node,
        url: saved_url,
        target,
        result: Ok(content),
    });
    assert!(stale_generation.is_empty());
    assert!(app.saved_readers[&reader_node].content.is_none());
}

#[test]
fn saved_reader_navigation_back_restores_exact_offline_selector_and_rejects_bad_targets() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let source_node = visit(&mut app, "https://saved-source.test/history-source");
    let annotation = eidetic::Hash::of(b"history-capture").to_string();
    attach(&mut app, graph, source_node, &annotation);
    let key = app.saved_source_admissions()[0].key.clone();
    let text = "Alpha 🐦 selected passage omega.";
    let (target, content) = target(key.clone(), text);
    let saved_url = crate::saved_sources::address(&target).unwrap();
    let reader_node = visit(&mut app, &saved_url);
    app.commit_content_navigation(reader_node, "https://ordinary.test/history-away".into());

    let back = app.update(Action::NavBack);
    assert!(!back.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFeed { .. } | Effect::FetchFavicon { .. }
    )));
    let current = app
        .graph_runtimes
        .graph_containing_member(reader_node)
        .and_then(|graph| app.graph_runtimes.canvas(graph))
        .and_then(|canvas| canvas.graph().get_node_by_id(reader_node))
        .map(|(_, node)| node.url().to_owned());
    assert_eq!(current.as_deref(), Some(saved_url.as_str()));
    let read = app.request_saved_source_read(reader_node, saved_url.clone(), target.clone());
    let (request, generation, actual_target) = read
        .iter()
        .find_map(|effect| match effect {
            Effect::ReadSavedSource {
                request,
                generation,
                target,
                ..
            } => Some((*request, *generation, target.clone())),
            _ => None,
        })
        .expect("history-restored internal URL resolves through the local source actor");
    assert_eq!(actual_target, target);
    assert!(!read.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFeed { .. } | Effect::FetchFavicon { .. }
    )));
    let installed = app.apply_update(Update::SavedSourceRead {
        generation,
        request,
        node: reader_node,
        url: saved_url,
        target: actual_target,
        result: Ok(content),
    });
    assert!(
        installed.iter().any(
            |effect| matches!(effect, Effect::SpawnContent { node, .. } if *node == reader_node)
        )
    );

    let malformed_hash = Target {
        text_hash: "not-a-hash".into(),
        ..target.clone()
    };
    let malformed_range = Target {
        selector: Some(Selector { start: 4, end: 4 }),
        ..target
    };
    for malformed in [malformed_hash, malformed_range] {
        let effects = app.update(Action::OpenSavedSource(malformed));
        assert!(!effects.iter().any(|effect| matches!(
            effect,
            Effect::FetchPage { .. }
                | Effect::FetchFeed { .. }
                | Effect::FetchFavicon { .. }
                | Effect::SpawnContent { .. }
                | Effect::ReadSavedSource { .. }
        )));
    }
    assert!(
        app.saved_source_admissions()
            .iter()
            .any(|admission| admission.key == key)
    );
}

#[test]
fn opening_saved_source_uses_the_focused_panes_graph_not_the_compatibility_cursor() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let source_graph = app.graph_runtimes.active_graph();
    let source_node = visit(&mut app, "https://saved-source.test/pane-source");
    let annotation = eidetic::Hash::of(b"pane-capture").to_string();
    attach(&mut app, source_graph, source_node, &annotation);
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "A 🐦 pane-owned passage.");
    let saved_url = crate::saved_sources::address(&target).unwrap();

    // Pane 0 remains focused on the admitted source graph. Make a second
    // graph active only through the legacy runtime cursor and give it the
    // same local node-key shape, so a compatibility lookup selects the wrong
    // node if the open path forgets its pane owner.
    let cursor_graph = crate::panes::GraphId::new();
    let mut cursor_canvas = Canvas::new();
    let cursor_key = cursor_canvas.visit("https://cursor-only.test/decoy");
    let cursor_node = cursor_canvas.graph().get_node(cursor_key).unwrap().id;
    app.graph_runtimes
        .activate_or_insert(cursor_graph, None, cursor_canvas);
    assert!(app.graph_runtimes.activate(cursor_graph));
    assert_eq!(app.focused_graph_pane(), Some(crate::panes::PaneId(0)));
    assert_eq!(
        app.graph_for_pane(crate::panes::PaneId(0)),
        Some(source_graph)
    );
    assert_eq!(app.graph_runtimes.active_graph(), cursor_graph);

    let effects = app.update(Action::OpenSavedSource(target.clone()));
    let reader_node = app
        .graph_runtimes
        .canvas(source_graph)
        .and_then(|canvas| canvas.graph().get_node_by_url(&saved_url))
        .map(|(_, node)| node.id)
        .expect("the saved reader was visited in the focused pane's graph");
    assert_ne!(reader_node, cursor_node);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node, url } if *node == reader_node && url == &saved_url
    )));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node, .. } if *node == cursor_node
    )));
    assert!(
        app.browser
            .get(reader_node)
            .is_some_and(|state| state.content_on)
    );
    assert!(
        !app.browser
            .get(cursor_node)
            .is_some_and(|state| state.content_on)
    );

    // A pane actually owned by another session cannot host the private reader,
    // even while the compatibility cursor remains a different graph.
    let foreign_graph = crate::panes::GraphId::new();
    app.graph_runtimes.activate_or_insert(
        foreign_graph,
        Some(crate::panes::SessionId::new()),
        Canvas::new(),
    );
    assert!(app.graph_runtimes.activate(cursor_graph));
    app.frisket.root = crate::panes::PaneNode::Leaf {
        pane_id: crate::panes::PaneId(0),
        content: crate::panes::PaneContent::Orrery,
        graph_id: foreign_graph,
    };
    app.index_pane_spaces();
    assert_eq!(
        app.graph_for_pane(crate::panes::PaneId(0)),
        Some(foreign_graph)
    );
    assert_eq!(app.graph_runtimes.active_graph(), cursor_graph);
    let refused = app.update(Action::OpenSavedSource(target));
    assert!(!refused.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { .. }
            | Effect::ReadSavedSource { .. }
            | Effect::FetchPage { .. }
            | Effect::FetchFeed { .. }
            | Effect::FetchFavicon { .. }
    )));
}

#[test]
fn opening_saved_source_from_answers_reveals_and_focuses_the_graph_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let node = visit(&mut app, "https://gone.test/citation");
    attach(
        &mut app,
        graph,
        node,
        &eidetic::ManifestId::of_blob(b"A1").to_string(),
    );
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "An exact 🐦 saved passage.");
    let url = crate::saved_sources::address(&target).unwrap();
    app.update(Action::OpenAnswers);
    let answer_pane = app.active_pane.unwrap();
    app.focus = crate::surface::FocusTarget::Pane(answer_pane);
    app.maximized = Some(answer_pane);
    let effects = app.update(Action::OpenSavedSource(target));
    let pane = app.default_graph_pane();
    assert_eq!(app.active_pane, Some(pane));
    assert_eq!(app.focus, crate::surface::FocusTarget::Graph(pane));
    assert_eq!(app.maximized, Some(pane));
    let reader = app
        .graph_pane_focused_member(pane)
        .expect("the reader is focused in its owning graph");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SpawnContent { node, url: actual } if *node == reader && *actual == url)));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFavicon { .. }
    )));
}

#[test]
fn direct_saved_citation_address_reveals_and_focuses_the_graph_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let graph = app.graph_runtimes.active_graph();
    let node = visit(&mut app, "https://gone.test/citation");
    attach(
        &mut app,
        graph,
        node,
        &eidetic::ManifestId::of_blob(b"A1").to_string(),
    );
    let key = app.saved_source_admissions()[0].key.clone();
    let (target, _) = target(key, "An exact 🐦 saved passage.");
    let url = crate::saved_sources::address(&target).unwrap();
    app.update(Action::OpenAnswers);
    let answer_pane = app.active_pane.unwrap();
    app.focus = crate::surface::FocusTarget::Pane(answer_pane);
    app.maximized = Some(answer_pane);
    let effects = app.update(Action::OpenAddress(url.clone()));
    let pane = app.default_graph_pane();
    assert_eq!(app.active_pane, Some(pane));
    assert_eq!(app.focus, crate::surface::FocusTarget::Graph(pane));
    assert_eq!(app.maximized, Some(pane));
    let reader = app
        .graph_pane_focused_member(pane)
        .expect("the reader is focused in its owning graph");
    assert!(effects.iter().any(|effect| matches!(effect, Effect::SpawnContent { node, url: actual } if *node == reader && *actual == url)));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::FetchFavicon { .. }
    )));
}
