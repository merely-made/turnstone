// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

#[test]
fn reopening_a_retained_clip_with_a_new_range_respawns_its_reader() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let manifest = eidetic::ManifestId::of_blob(b"retained test clip");
    let url = crate::retained_clip::address(manifest);
    let node = visit(&mut app, &url);
    let admission = app
        .research_clip_admissions()
        .into_iter()
        .find(|clip| clip.node == node)
        .expect("clip belongs to current graph");
    let mut target = crate::research::ClipTarget {
        clip: admission,
        text_hash: eidetic::Hash::of("one 🐦 two".as_bytes()).to_string(),
        selector: Some(Selector { start: 0, end: 3 }),
    };
    app.open_research_clip(target.clone());
    target.selector = Some(Selector { start: 4, end: 5 });
    let effects = app.open_research_clip(target);
    assert!(
        effects.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node: opened, url: address } if *opened == node && address == &url)),
        "a changed selected passage must reach a fresh reader"
    );
}

#[test]
fn retained_clip_reader_refuses_a_malformed_text_hash() {
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let manifest = eidetic::ManifestId::of_blob(b"clip with malformed citation hash");
    let node = visit(&mut app, &crate::retained_clip::address(manifest));
    let clip = app
        .research_clip_admissions()
        .into_iter()
        .find(|clip| clip.node == node)
        .expect("clip belongs to current graph");
    let effects = app.open_research_clip(crate::research::ClipTarget {
        clip,
        text_hash: "not-a-hash".into(),
        selector: None,
    });
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SpawnContent { .. })),
        "a malformed citation must not open an unvalidated whole clip"
    );
}

#[test]
fn model_free_search_action_opens_the_exact_sealed_clip_reader() {
    use eidetic_search::query::{
        Coverage, ProducerKind, QueryMode, ResearchSnapshot, ResearchSource,
    };
    let root = tempfile::tempdir().unwrap();
    let mut app = app_at(root.path());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    let epoch = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1));
    let source = "PRIVATE café 🐦 harbor PRIVATE";
    let text = "café 🐦 harbor";
    let start = source.find(text).unwrap();
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/source",
        source,
        &crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start,
            end: start + text.len(),
        },
    )
    .unwrap();
    let context = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: None,
            entity: "producer absent".into(),
            revision: None,
        },
        capture_id: "fixture".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(1),
    };
    let manifest = crate::retained_clip::retain_native(
        root.path(),
        crate::retained_clip::NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &context,
        crate::retained_clip::limits().unwrap(),
        &*custody.sealer,
    )
    .unwrap();
    let url = crate::retained_clip::address(manifest);
    let node = visit(&mut app, &url);
    let clip = app
        .research_clip_admissions()
        .into_iter()
        .find(|clip| clip.node == node)
        .unwrap();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: {
            let epoch = epoch.clone();
            std::sync::Arc::new(move || Some(epoch.load(std::sync::atomic::Ordering::SeqCst)))
        },
    }));
    let effects = app.update(Action::SearchSavedSources);
    let base = effects
        .into_iter()
        .find_map(|fx| match fx {
            Effect::ResearchSearch(request) => Some(request),
            _ => None,
        })
        .unwrap();
    let hash = eidetic::Hash::of(text.as_bytes()).to_string();
    let whole = crate::research::Target::Clip(crate::research::ClipTarget {
        clip,
        text_hash: hash.clone(),
        selector: None,
    });
    let source_id = crate::research::source_id(&whole).unwrap();
    let snapshot = ResearchSnapshot {
        generation: "action fixture".into(),
        scope: vec![source_id.clone()],
        sources: vec![ResearchSource {
            retained: eidetic::models::recall::RetainedSourceText {
                source: source_id,
                version: hash,
                title: Some("Knot selection".into()),
                text: text.into(),
            },
            producer: ProducerKind::Selection,
            address: None,
            date: None,
            tags: vec![],
        }],
        coverage: Coverage::Complete,
    };
    let mut last_open = None;
    for (phrase, expected) in [("café", "café"), ("harbor", "harbor")] {
        let mut request = base.clone();
        request.id = uuid::Uuid::new_v4();
        request.query.mode = QueryMode::Phrase(phrase.into());
        request.settings.snippet_context = 0;
        let result =
            eidetic_search::query::search(&snapshot, &request.query, &request.settings).unwrap();
        let hit = &result.hits[0];
        let passage = hit.passage.as_ref().unwrap();
        let mut target = crate::research::target(&hit.source, &hit.version).unwrap();
        target.set_selector(Selector {
            start: passage.selector.start,
            end: passage.selector.end,
        });
        app.research_search.request = Some(request.clone());
        app.research_search_finished(request.clone(), Ok(result));
        last_open = Some((target.clone(), request.id));
        let effects = app.update(Action::ResearchSearch(
            crate::action::ResearchSearchAction::Open {
                target,
                request: request.id,
            },
        ));
        assert!(
            effects.iter().any(
                |fx| matches!(fx, Effect::SpawnContent { node: actual, .. } if *actual == node)
            )
        );
        let document = app
            .research_clip_document(node, &url, manifest)
            .unwrap()
            .unwrap();
        assert!(document.blocks.iter().any(|block| matches!(block, inker::Block::Quote { blocks } if matches!(blocks.as_slice(), [inker::Block::Paragraph { spans }] if matches!(spans.as_slice(), [inker::InlineSpan::Text(value)] if value == expected)))));
    }
    let reader = app.research_clip_readers.get_mut(&node).unwrap();
    reader.target.selector = Some(Selector { start: 0, end: 999 });
    assert!(app.research_clip_document(node, &url, manifest).is_err());
    let reader = app.research_clip_readers.get_mut(&node).unwrap();
    reader.target.selector = None;
    assert!(
        !app.research_clip_document(node, &url, manifest)
            .unwrap()
            .unwrap()
            .blocks
            .iter()
            .any(|block| matches!(block, inker::Block::Quote { .. }))
    );
    app.research_clip_readers
        .get_mut(&node)
        .unwrap()
        .target
        .text_hash = eidetic::Hash::of(b"wrong").to_string();
    assert!(app.research_clip_document(node, &url, manifest).is_err());

    // The search open has been accepted, but its queued reader spawn has not
    // yet resolved the document. Custody advances without replacing identity.
    let identity = app.identity.clone().unwrap();
    let (target, request) = last_open.unwrap();
    let crate::research::Target::Clip(clip_target) = target.clone() else {
        panic!("the search result should be a retained clip");
    };
    let effects = app.update(Action::ResearchSearch(
        crate::action::ResearchSearchAction::Open { target, request },
    ));
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node: opened, .. } if *opened == node)));
    epoch.store(2, std::sync::atomic::Ordering::SeqCst);
    assert!(std::sync::Arc::ptr_eq(
        app.identity.as_ref().unwrap(),
        &identity
    ));
    assert!(app.research_clip_document(node, &url, manifest).is_err());
    let effects = app.invalidate_research_clip_readers();
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::CloseContent { node: closed } if *closed == node)));
    assert!(!app.research_clip_readers.contains_key(&node));

    assert!(
        app.open_research_clip(clip_target.clone())
            .iter()
            .any(|effect| matches!(effect,
        Effect::SpawnContent { node: opened, .. } if *opened == node))
    );
    app.research.access(None);
    assert!(app.research_clip_document(node, &url, manifest).is_err());
    assert!(
        app.invalidate_research_clip_readers()
            .iter()
            .any(|effect| matches!(effect,
        Effect::CloseContent { node: closed } if *closed == node))
    );
    assert!(
        !app.open_research_clip(clip_target)
            .iter()
            .any(|effect| matches!(effect,
        Effect::SpawnContent { node: opened, .. } if *opened == node))
    );
}
