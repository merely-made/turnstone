// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn toolbar_tool_toggle_retains_identity_state_and_restores_the_same_split() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/reading".into()));
    app.update(Action::SetWorkspaceMap(false));
    let reader = app.active_pane.unwrap();
    let toggle =
        Action::ToggleToolPane(crate::panes::PaneKindId::new(crate::panes::kind::INSPECTOR));
    app.update(toggle.clone());
    let inspector = app.active_pane.unwrap();
    assert_ne!(reader, inspector);
    assert_eq!(app.frisket.outer_workspace().unwrap().active_ids().len(), 2);
    app.update(toggle.clone());
    assert_eq!(app.active_pane, Some(reader));
    assert!(
        app.frisket.iter_leaves().any(|(id, _, _)| id == inspector),
        "hiding retains the pane"
    );
    assert_eq!(
        app.frisket.outer_workspace().unwrap().active_ids(),
        vec![workbench::TileId(reader.0)]
    );
    app.update(toggle);
    assert_eq!(app.active_pane, Some(inspector));
    assert_eq!(app.frisket.outer_workspace().unwrap().active_ids().len(), 2);
    app.update(Action::ToggleToolPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    app.update(Action::SetWorkspaceMap(true));
    let map = app.active_pane.unwrap();
    app.update(Action::ToggleToolPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    assert!(app.map_navigation_active(), "showing a tool preserves Map");
    assert!(
        app.frisket
            .outer_workspace()
            .unwrap()
            .active_ids()
            .contains(&workbench::TileId(map.0))
    );
}

#[test]
fn workbench_open_preserves_adjacent_tools_and_their_split_share() {
    let mut app = App::test_stub();
    let graph = app.default_graph_pane();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    let inspector = app.active_pane.unwrap();
    let before = app.frisket.split_at(&[]).unwrap();
    app.active_pane = Some(graph);
    app.update(Action::OpenAddress("https://example.com/reading".into()));
    app.update(Action::OpenInWorkbench);
    let reader = app.active_pane.unwrap();
    assert_eq!(
        app.frisket.split_at(&[]).unwrap(),
        before,
        "the adjacent tool keeps its existing share"
    );
    let frame = app.frisket.outer_workspace().unwrap();
    assert_eq!(
        frame.active_ids().len(),
        2,
        "Read occupies its map's main area beside the tool"
    );
    assert!(frame.active_ids().contains(&workbench::TileId(inspector.0)));
    assert!(frame.active_ids().contains(&workbench::TileId(reader.0)));
    assert!(!frame.active_ids().contains(&workbench::TileId(graph.0)));
    assert!(matches!(
        frame.workspace.tiled(),
        workbench::TileTree::Split { .. }
    ));
}

/// A Workbench page can differ from the canvas selection. Browser controls
/// act on that page without changing or operating on its graph sibling.
#[test]
fn workbench_browser_commands_preserve_the_independent_graph_selection() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/a".into()));
    let a = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.update(Action::OpenAddress("https://example.com/b".into()));
    let b = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.content.note_live(a, None);
    app.content.note_live(b, None);
    app.update(Action::WorkbenchActivate(a));
    assert_eq!(app.graph_runtimes.focused_member(), Some(b));
    assert_eq!(
        app.browser_command_target(),
        Some((a, "https://example.com/a".into()))
    );

    app.graph_pane_frame(app.default_graph_pane(), 800, 600)
        .unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    assert_eq!(
        app.browser_command_target(),
        Some((a, "https://example.com/a".into())),
        "a graph repaint must not retarget the following Inspector"
    );

    let closed = app.update(Action::ToggleNodeContent);
    assert!(closed.contains(&Effect::CloseContent { node: a }));
    assert!(!closed.contains(&Effect::CloseContent { node: b }));
    assert!(matches!(
        app.content.get(b),
        Some(crate::content::NodeContent::Live)
    ));
    let opened = app.update(Action::ToggleNodeContent);
    assert!(opened.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node, url } if *node == a && url == "https://example.com/a")));
    app.content.note_live(a, None);
    let reloaded = app.update(Action::Reload);
    assert!(reloaded.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node, url } if *node == a && url == "https://example.com/a")));
    assert!(!reloaded.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node, .. } | Effect::CloseContent { node } if *node == b)));
    assert_eq!(app.graph_runtimes.focused_member(), Some(b));
}

/// Two graph panes may name different graph/Forme sources while followers
/// resolve the last graph-pane context in their own space. Camera/selection
/// and Workbench state stay behind those pane and Forme boundaries.
#[test]
fn graph_views_workbenches_and_followers_are_pane_scoped() {
    let mut app = App::test_stub();
    let graph_a = app.graph_runtimes.active_graph();
    let member_a = {
        let canvas = app.graph_runtimes.canvas_mut(graph_a).unwrap();
        let key = canvas.visit("mere://graph-a");
        canvas.graph().get_node(key).unwrap().id
    };

    let graph_b = crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(0xb));
    let mut canvas_b = mere::canvas::Canvas::new();
    let member_b = {
        let key = canvas_b.visit("mere://graph-b");
        canvas_b.graph().get_node(key).unwrap().id
    };
    app.graph_runtimes
        .activate_or_insert(graph_b, None, canvas_b);
    assert!(app.graph_runtimes.activate(graph_a));

    let first = crate::panes::PaneId(0);
    let second = crate::panes::PaneId(1);
    let follower = crate::panes::PaneId(2);
    app.frisket.root = crate::panes::PaneNode::Split {
        axis: crate::panes::SplitAxis::Horizontal,
        ratio: 0.5,
        first: Box::new(crate::panes::PaneNode::Leaf {
            pane_id: first,
            content: PaneContent::Orrery,
            graph_id: graph_a,
        }),
        second: Box::new(crate::panes::PaneNode::Split {
            axis: crate::panes::SplitAxis::Vertical,
            ratio: 0.5,
            first: Box::new(crate::panes::PaneNode::Leaf {
                pane_id: second,
                content: PaneContent::Orrery,
                graph_id: graph_b,
            }),
            second: Box::new(crate::panes::PaneNode::Leaf {
                pane_id: follower,
                content: PaneContent::Roster,
                graph_id: graph_b,
            }),
        }),
    };
    app.next_pane_id = 3;
    app.index_pane_spaces();

    assert!(
        app.with_graph_pane(first, |canvas| canvas.select_member(member_a))
            .unwrap()
    );
    assert_eq!(app.graph_pane_focused_member(first), Some(member_a));
    assert_eq!(
        app.follower_context(follower).unwrap().member,
        Some(member_a)
    );

    assert!(
        app.with_graph_pane(second, |canvas| canvas.select_member(member_b))
            .unwrap()
    );
    assert_eq!(app.graph_pane_focused_member(second), Some(member_b));
    assert_eq!(
        app.follower_context(follower).unwrap().member,
        Some(member_b)
    );

    app.workbench_for_pane_mut(first)
        .unwrap()
        .open_tile(member_a);
    app.workbench_for_pane_mut(second)
        .unwrap()
        .open_tile(member_b);
    assert!(app.workbench_for_pane(first).unwrap().has_tile(member_a));
    assert!(!app.workbench_for_pane(first).unwrap().has_tile(member_b));
    assert!(app.workbench_for_pane(second).unwrap().has_tile(member_b));
}

/// The browser-state sidecar (rung 6): content-on mirrors live truth at
/// refresh, prunes vanished nodes, and round-trips through the store.
#[test]
fn browser_states_refresh_and_round_trip() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/a".to_string()));
    let a = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(Update::ContentSpawned {
        node: a,
        facts: None,
    });
    app.update(Action::OpenAddress("https://example.com/b".to_string()));
    app.refresh_browser_states();
    assert!(app.browser.get(a).is_some_and(|b| b.content_on));
    assert!(
        app.browser
            .get(app.graph_runtimes.focused_member().unwrap())
            .is_none(),
        "a node without content stays out of the sidecar"
    );
    // Round trip through the converged store: web.* facets in facets.json.
    let dir = std::env::temp_dir().join(format!("turnstone-bn-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut facets = pandect::NodeFacetStore::new();
    pandect::write_web_states(&mut facets, &app.browser);
    crate::session::save_node_facets(&dir, &facets);
    let reloaded = crate::session::load_node_facets(&dir).unwrap_or_default();
    let restored = pandect::read_web_states(&reloaded);
    assert!(restored.get(a).is_some_and(|b| b.content_on));
    // Content off -> the refresh clears the flag.
    app.content.note_closed(a);
    app.refresh_browser_states();
    assert!(
        !app.browser.get(a).is_some_and(|b| b.content_on),
        "closed content clears the flag on the next refresh"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// SC step 4 pass B: a node's content lane, not its engine id, decides who
/// takes Reload and Stop. Every surface engine (Scry and Servo as well as
/// Weld) gets them on its own web plane; a document reloads through the App's
/// load.
#[test]
fn every_surface_engine_takes_reload_and_stop_on_its_web_plane() {
    for engine in [
        inker::routing::ENGINE_WELD_CHROMIUM,
        inker::routing::ENGINE_SCRYING_WEB,
        inker::routing::ENGINE_GRAFT_SERVO,
    ] {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.com/page".to_string()));
        let node = app.graph_runtimes.focused_member().unwrap();
        // The address's own page fetch is the App's, and Stop cancels it first.
        assert!(
            app.update(Action::Stop)
                .iter()
                .any(|effect| matches!(effect, Effect::CancelPage { .. }))
        );
        app.content.note_live(
            node,
            Some(crate::content::ContentFacts {
                engine: engine.to_string(),
                lane: crate::content::ContentLane::Surface,
                structure: None,
                lineage: None,
                capabilities: Default::default(),
            }),
        );
        let reload = app.update(Action::Reload);
        assert!(
            reload.contains(&Effect::ControlContent {
                node,
                control: crate::action::ContentControl::Reload,
            }),
            "{engine}: {reload:?}"
        );
        let stop = app.update(Action::Stop);
        assert!(
            stop.contains(&Effect::ControlContent {
                node,
                control: crate::action::ContentControl::Stop,
            }),
            "{engine}: a surface mid-load stops on its web plane"
        );
    }

    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/page".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: inker::routing::ENGINE_GENET_LIVERY.to_string(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    );
    assert!(
        !app.update(Action::Reload)
            .iter()
            .any(|effect| matches!(effect, Effect::ControlContent { .. })),
        "a document reloads through the App's load"
    );
}

/// The requested page scale rides the same sidecar to the same store, and a
/// reset leaves nothing behind for it to carry.
#[test]
fn requested_page_scale_round_trips_through_the_store() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/zoom".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: "weld.chromium".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: crate::content::DocumentCapabilityFacts {
                page_zoom: crate::content::CapabilityStatus::Partial {
                    detail: "applied, but the effective level is not read back".into(),
                },
                ..Default::default()
            },
        }),
    );
    app.update(Action::PageZoomIn { member: node });
    app.update(Action::PageZoomIn { member: node });

    let dir = std::env::temp_dir().join(format!("turnstone-zoom-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let round_trip = |browser: &pandect::browser_node_state::BrowserNodeStates| {
        let mut facets = pandect::NodeFacetStore::new();
        pandect::write_web_states(&mut facets, browser);
        crate::session::save_node_facets(&dir, &facets);
        pandect::read_web_states(&crate::session::load_node_facets(&dir).unwrap_or_default())
    };
    assert_eq!(
        round_trip(&app.browser)
            .get(node)
            .and_then(|state| state.page_scale),
        Some(1.25),
        "the REQUESTED scale is what persists"
    );

    app.update(Action::PageZoomReset { member: node });
    assert_eq!(
        round_trip(&app.browser)
            .get(node)
            .and_then(|state| state.page_scale),
        None,
        "reset clears rather than storing the default"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The workbench sidecar round-trips through the persistence port,
/// pruned to present members (platen's canonical pair underneath).
#[test]
fn workbench_persists_and_restores_pruned() {
    let dir = std::env::temp_dir().join(format!("turnstone-wb-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    let mut wb = mere::platen::Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(a);
    wb.open_tile(b);
    crate::session::save_workbench(&dir, &wb);
    // Both present: both tiles come back.
    let present: std::collections::HashSet<_> = [a, b].into_iter().collect();
    let restored = crate::session::load_workbench(&dir, &present);
    assert_eq!(restored.tile_count(), 2);
    // b's node vanished between sessions: its tile is reconciled away.
    let present: std::collections::HashSet<_> = [a].into_iter().collect();
    let restored = crate::session::load_workbench(&dir, &present);
    assert_eq!(restored.tile_count(), 1);
    assert!(restored.has_tile(a));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Committing a find-lane node row selects without fetching.
#[test]
fn committing_a_node_row_selects_without_fetch_effects() {
    let mut app = App::test_stub();
    app.graph_runtimes.visit("https://example.com/meerkats");
    app.update(Action::OmnibarOpen { command: false });
    for c in "meer".chars() {
        app.update(Action::OmnibarChar(c));
    }
    let effects = app.update(Action::OmnibarCommit);
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { .. })),
        "selecting an existing node must not refetch: {effects:?}"
    );
    assert!(!app.omnibar.open);
}

#[test]
fn omnibar_transcript_freezes_its_open_context_and_repeats_it() {
    let mut app = App::test_stub();
    let original_session = app.session_id;
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarInsert(
        "https://example.com/field-notes".into(),
    ));
    app.update(Action::OmnibarCommit);

    let first = app
        .shell_transcript()
        .entries()
        .next()
        .expect("a committed address enters the shell transcript")
        .clone();
    assert_eq!(first.target.context.session, Some(original_session));
    assert!(matches!(
        first.outcome,
        crate::shell_services::ShellOutcome::Completed { .. }
    ));

    // A changed host session does not retarget an already-composed command.
    app.session_id = crate::panes::SessionId::new();
    app.update(Action::RepeatShellEntry(first.id));
    let repeated = app
        .shell_transcript()
        .entries()
        .last()
        .expect("repeat makes a separately correlated attempt");
    assert_ne!(repeated.id, first.id);
    assert_eq!(repeated.target.context.session, Some(original_session));

    app.update(Action::OpenShellEntryTarget(first.id));
    assert_eq!(
        app.take_requested_shell_context(),
        Some(first.target),
        "A2 receives the original target rather than a canvas-derived replacement"
    );
}

/// The recall lane's ask: typing a needle lowers one RecallQuery, and the
/// answer becomes rows below the go row (search wiring W2).
#[test]
fn typing_asks_for_recall_and_the_answer_becomes_rows() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('r'));
    let effects = app.update(Action::OmnibarChar('s'));

    let asked: Vec<&String> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::RecallQuery { query } => Some(query),
            _ => None,
        })
        .collect();
    assert_eq!(asked, vec!["rs"], "one ask, carrying the current needle");

    app.apply_update(Update::RecallHits {
        query: "rs".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://rust-lang.org/".to_string(),
            title: Some("Rust".to_string()),
            at_ms: 42,
        }],
    });
    let rows = &app.omnibar.suggestions;
    assert!(
        rows.iter().any(|s| matches!(
            s,
            crate::ui::Suggestion::Recall { url, .. } if url == "https://rust-lang.org/"
        )),
        "the recalled page is offered: {rows:?}"
    );
}
