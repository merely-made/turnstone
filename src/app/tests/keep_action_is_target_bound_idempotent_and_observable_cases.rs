// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn keep_action_is_target_bound_idempotent_and_observable() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://keep/first".to_string()));
    let first = app.graph_runtimes.focused_member().expect("first node");
    let keep_first = app
        .available_actions()
        .into_iter()
        .find(|(label, _)| label == "Keep node")
        .map(|(_, action)| action)
        .expect("an unkept focused node offers Keep");

    app.update(Action::OpenAddress("mere://keep/second".to_string()));
    let second = app.graph_runtimes.focused_member().expect("second node");
    assert_ne!(first, second);
    let _ = app.take_events();

    let effects = app.update(keep_first);
    assert!(app.node_is_kept(first));
    assert!(!app.node_is_kept(second));
    assert_eq!(app.graph_runtimes.focused_member(), Some(second));
    assert_eq!(effects, vec![Effect::SaveSession, Effect::Redraw]);
    assert_eq!(app.take_events(), vec![AppEvent::NodeKept(first)]);

    assert_eq!(
        app.update(Action::KeepNode { member: first }),
        vec![Effect::Redraw],
        "repeating Keep is an idempotent no-op"
    );
    assert!(app.take_events().is_empty());

    app.update(Action::KeepNode { member: second });
    let snap = crate::observe::snapshot(&app);
    assert!(snap.focused.as_ref().is_some_and(|node| node.kept));
    assert!(
        !app.available_actions()
            .iter()
            .any(|(label, _)| label == "Keep node"),
        "the contextual palette row leaves once its action is complete"
    );
}

#[test]
fn publishing_pane_is_a_summonable_window_control() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::PUBLISHING,
    )));

    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, content, _)| content.kind_id().as_str() == crate::panes::kind::PUBLISHING)
    );
    assert!(
        crate::action::palette_actions()
            .iter()
            .any(|(label, action)| {
                *label == "Open Publishing pane"
                    && matches!(action, Action::SummonPane(id) if id.as_str() == crate::panes::kind::PUBLISHING)
            })
    );
}

#[test]
fn shared_knot_pane_is_a_summonable_reader_control() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::SHARED_KNOT,
    )));

    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, content, _)| content.kind_id().as_str() == crate::panes::kind::SHARED_KNOT)
    );
    assert!(
        crate::action::palette_actions()
            .iter()
            .any(|(label, action)| {
                *label == "Open Shared document pane"
                    && matches!(action, Action::SummonPane(id) if id.as_str() == crate::panes::kind::SHARED_KNOT)
            })
    );
}

/// Composition ORDER is the config's order, so reordering is the same leaf
/// edit as add/remove: it moves within the stack, clamps at the ends
/// rather than wrapping, and the palette only offers a move that would do
/// something.
#[test]
fn moving_a_composed_section_reorders_that_leaf_and_clamps() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::GLOSS,
    )));
    let pane = app.active_pane.expect("the summoned gloss is active");
    let sections = |app: &App| match app.pane_content(pane) {
        Some(PaneContent::Gloss(cfg)) => cfg.sections.clone(),
        _ => panic!("the active pane is a Gloss"),
    };
    let mv = |app: &mut App, section: &str, delta: i32| {
        app.update(Action::MovePaneSection {
            pane,
            section: section.to_string(),
            delta,
        })
    };

    // Start with a configured bare minimap for the one/two-section checks.
    app.update(Action::TogglePaneSection {
        pane,
        section: "downloads".into(),
    });
    // With ONE section there is nothing to reorder, so no move row.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    assert!(
        !app.session_actions()
            .iter()
            .any(|(label, _)| label.starts_with("Gloss: move section")),
        "a lone section offers no move"
    );

    // Compose a second: it stacks BELOW, in config order.
    app.update(Action::TogglePaneSection {
        pane,
        section: "nodes".to_string(),
    });
    assert_eq!(sections(&app), vec!["removed", "nodes"]);

    // Moving it up swaps the stack, and persists with the layout.
    let fx = mv(&mut app, "nodes", -1);
    assert_eq!(sections(&app), vec!["nodes", "removed"]);
    assert!(
        fx.iter().any(|e| matches!(e, Effect::SaveSession)),
        "a reorder persists like any leaf edit: {fx:?}"
    );

    // At the top, up is a no-op: clamped, NOT wrapped to the bottom. It
    // reports no move, so the receipt cannot mistake it for one.
    let fx = mv(&mut app, "nodes", -1);
    assert_eq!(
        sections(&app),
        vec!["nodes", "removed"],
        "clamped at the top"
    );
    assert!(
        !fx.iter().any(|e| matches!(e, Effect::SaveSession)),
        "a no-op move saves nothing: {fx:?}"
    );
    // And the palette does not offer it.
    assert!(
        !app.session_actions()
            .iter()
            .any(|(label, _)| label == "Gloss: move section up — Nodes"),
        "no up-row on the first section"
    );

    // An id this pane has not composed moves nothing.
    let fx = mv(&mut app, "recent", 1);
    assert_eq!(sections(&app), vec!["nodes", "removed"]);
    assert!(!fx.iter().any(|e| matches!(e, Effect::SaveSession)));
}

/// Composition is a property of a PANE, not of the Gloss: the Overmap
/// composes the same sections, through the same renderer, with the same
/// leaf config and the same actions. Its palette rows name it, derived from
/// the pane's own tag rather than a second table.
#[test]
fn the_overmap_composes_sections_too() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::OVERMAP,
    )));
    let pane = app.active_pane.expect("the summoned overmap is active");

    // A fresh Overmap composes nothing (its swatch fills the pane).
    assert_eq!(
        app.pane_content(pane).and_then(|c| c.composition()),
        Some(&crate::panes::PaneComposition::default())
    );
    // The palette offers ITS rows, named for IT.
    assert!(
        app.session_actions()
            .iter()
            .any(|(label, _)| label == "Overmap: add section — Removed"),
        "the pane-scoped rows name the pane: {:?}",
        app.session_actions()
    );

    // The same action composes it, onto ITS OWN leaf.
    app.update(Action::TogglePaneSection {
        pane,
        section: "removed".to_string(),
    });
    match app.pane_content(pane) {
        Some(PaneContent::Overmap(cfg)) => assert_eq!(cfg.sections, vec!["removed"]),
        other => panic!("expected a composed overmap, got {other:?}"),
    }
}

/// The rung7_lens_ops receipt's exact op sequence, app-level: tear out
/// the roster, summon the trail beside it (in the lens), reweight, close
/// the ACTIVE pane. The close must remove the TRAIL (the summon made it
/// active), never the roster.
#[test]
fn lens_ops_close_removes_the_summoned_pane() {
    let mut app = App::test_stub();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::ROSTER,
    )));
    app.update(Action::TearOutActivePane);
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::TRAIL,
    )));
    app.update(Action::SetActivePaneDivider(0.7));
    app.update(Action::CloseActivePane);
    let lens = app.lenses[0].as_ref().unwrap();
    let tags: Vec<&str> = lens.iter_leaves().map(|(_, c, _)| c.tag()).collect();
    assert!(
        tags.contains(&"roster") && !tags.contains(&"trail"),
        "close removes the summoned trail, not the roster: {tags:?}"
    );
}

/// Browser controls act on the focused node's own lineage. Back and Forward
/// keep its identity and refetch the revealed address; a new in-node
/// navigation truncates Forward; Reload refreshes the same member.
#[test]
fn back_forward_and_reload_flow_through_the_spine() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/a".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "https://example.com/b".into(),
    });
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "https://example.com/c".into(),
    });
    assert!(app.focused_can_back());

    let effects = app.update(Action::NavBack);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { node: actual, .. } if *actual == node)),
        "Back refetches the focused member: {effects:?}"
    );
    assert_eq!(app.graph_runtimes.focused_member(), Some(node));
    assert_eq!(
        app.graph_runtimes.focused_url(),
        Some("https://example.com/b")
    );
    assert!(app.focused_can_forward());

    app.update(Action::NavForward);
    assert_eq!(
        app.graph_runtimes.focused_url(),
        Some("https://example.com/c")
    );

    app.update(Action::NavBack);
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "https://example.com/d".into(),
    });
    assert!(!app.focused_can_forward(), "new lineage truncates Forward");

    app.apply_update(Update::ContentSpawned { node, facts: None });
    let effects = app.update(Action::Reload);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::FetchPage { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CloseContent { .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. }))
    );
    let described: Vec<String> = app
        .take_events()
        .iter()
        .map(crate::observe::AppEvent::describe)
        .collect();
    assert!(described.iter().any(|e| e.starts_with("nav-back ")));
    assert!(described.iter().any(|e| e.starts_with("nav-forward ")));
    assert!(described.iter().any(|e| e.starts_with("reloaded ")));
}

#[test]
fn stop_cancels_only_the_focused_request_and_rejects_its_late_answer() {
    let mut app = App::test_stub();
    let url = "gemini://capsule.test/slow";
    app.update(Action::OpenAddress(url.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let request = app.content.active_fetch(node).expect("active fetch");

    let effects = app.update(Action::Stop);
    assert!(effects.contains(&Effect::CancelPage { request, node }));
    assert!(matches!(
        app.content.fetch_phase(node),
        Some(crate::content::PageFetchPhase::Stopped { received_bytes: 0 })
    ));

    let stale = app.apply_update(Update::PageFetched {
        request,
        node,
        url: url.into(),
        result: Ok(crate::action::FetchedPage::text(
            Some("text/gemini".into()),
            "# Too late",
        )),
    });
    assert!(stale.is_empty());
    assert!(app.content.fetched(node, url).is_none());
}

#[test]
fn nomadnet_omnibar_stop_and_reload_keep_native_request_identity() {
    let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/turnstone.mu";
    assert_eq!(
        crate::ui::normalize_address(address).as_deref(),
        Some(address)
    );
    let mut app = App::test_stub();
    let opened = app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let request = app.content.active_fetch(node).expect("native fetch starts");
    assert!(opened.iter().any(|effect| matches!(effect,
        Effect::FetchPage { url, .. } if url == address)));
    let stopped = app.update(Action::Stop);
    assert!(stopped.contains(&Effect::CancelPage { request, node }));
    assert!(
        app.apply_update(Update::PageFetched {
            request,
            node,
            url: address.into(),
            result: Ok(crate::action::FetchedPage::text(None, "late")),
        })
        .is_empty()
    );
    assert!(app.content.fetched(node, address).is_none());
    let reloaded = app.update(Action::Reload);
    let next = app
        .content
        .active_fetch(node)
        .expect("native reload starts");
    assert_ne!(request, next);
    assert!(reloaded.iter().any(|effect| matches!(effect,
        Effect::FetchPage { url, request, .. } if url == address && *request == next)));
}

/// The workbench lane end to end at the App tier: opening the focused
/// node tiles it, summons the Workbench pane, and spawns its content;
/// stacking collapses cells; closing empties honestly.
#[test]
fn workbench_actions_flow_through_the_spine() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    let a = app.graph_runtimes.focused_member().unwrap();
    let effects = app.update(Action::OpenInWorkbench);
    assert!(app.active_workbench().unwrap().is_tiled());
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    assert!(
        app.frisket
            .iter_leaves()
            .any(|(_, c, _)| matches!(c, PaneContent::Workbench)),
        "opening a tile summons the Workbench pane"
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::SpawnContent { .. })),
        "a tile wants live content: {effects:?}"
    );
    // Re-opening the same node adds nothing.
    app.update(Action::OpenInWorkbench);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    // A second node tiles beside it; stacking collapses to one cell.
    app.update(Action::OpenAddress("mere://beta".to_string()));
    let b = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    assert_eq!(app.active_workbench().unwrap().slot_count(), 2);
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    assert_eq!(app.active_workbench().unwrap().slot_count(), 1);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 2);
    // Activate the buried tab; close the focused (beta) tile.
    app.update(Action::WorkbenchActivate(a));
    app.update(Action::CloseWorkbenchTile);
    assert_eq!(app.active_workbench().unwrap().tile_count(), 1);
    assert!(app.active_workbench().unwrap().has_tile(a));
}

#[test]
fn workbench_initial_reader_shares_orrery_stack_and_survives_restart() {
    let mut app = App::test_stub();
    let graph = app.default_graph_pane();
    app.update(Action::OpenAddress("https://example.com/reading".into()));
    app.update(Action::OpenInWorkbench);
    let reader = app.active_pane.unwrap();
    assert_ne!(reader, graph);
    let restored: crate::panes::FrisketLayout =
        serde_json::from_str(&serde_json::to_string(&app.frisket).unwrap()).unwrap();
    let frame = restored.outer_workspace().unwrap();
    let workbench::TileTree::Stack(stack) = frame.workspace.tiled() else {
        panic!("initial reading must not reserve half the window for Orrery")
    };
    assert_eq!(
        stack.tabs.iter().map(|t| t.id.0).collect::<Vec<_>>(),
        vec![graph.0, reader.0]
    );
    assert_eq!(stack.tabs[stack.active].id.0, reader.0);
    drop(frame);
    app.frisket = restored;
    app.update(Action::OuterWorkspace {
        space: crate::action::SpaceRef::Primary,
        event: workbench::TileEvent::Activated(workbench::TileId(graph.0)).into(),
    });
    assert_eq!(app.active_pane, Some(graph));
    assert_eq!(
        app.frisket.outer_workspace().unwrap().active_ids(),
        vec![workbench::TileId(graph.0)]
    );
}

#[test]
fn graph_navigation_initial_read_keeps_inspect_beside_the_main_projection() {
    let mut app = App::test_stub();
    let graph = app.default_graph_pane();
    app.update(Action::OpenAddress(
        "https://example.test/read-with-inspect".into(),
    ));
    let member = app.graph_pane_focused_member(graph).unwrap();
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    let inspect = app.active_pane.unwrap();
    app.update(Action::ReadGraphMember(member));
    let read = app.active_pane.unwrap();
    let frame = app.frisket.outer_workspace().unwrap();
    assert_eq!(
        frame.active_ids().len(),
        2,
        "Read and Inspect use two main surfaces"
    );
    assert!(frame.active_ids().contains(&workbench::TileId(read.0)));
    assert!(frame.active_ids().contains(&workbench::TileId(inspect.0)));
    assert!(
        !frame.active_ids().contains(&workbench::TileId(graph.0)),
        "the map is retained behind its Read projection"
    );
    let path = crate::panes::workspace_bridge::path_for(
        frame.workspace.tiled(),
        workbench::TileId(graph.0),
    )
    .unwrap();
    let workbench::TileTree::Stack(stack) =
        crate::panes::workspace_bridge::tree_at(frame.workspace.tiled(), &path).unwrap()
    else {
        panic!("main stack")
    };
    assert_eq!(stack.tabs.len(), 2);
}

#[test]
fn toolbar_read_map_round_trip_preserves_member_identity_and_reveals_selection() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::SetWorkspaceMap(false));
    let reader = app.active_pane.unwrap();
    assert_eq!(app.browser_command_target().unwrap().0, first);
    app.update(Action::OpenAddress("https://example.test/second".into()));
    let second = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::WorkbenchActivate(first));
    app.update(Action::SetWorkspaceMap(true));
    let map = app.active_pane.unwrap();
    assert_ne!(reader, map);
    assert_eq!(app.graph_pane_focused_member(map), Some(first));
    app.graph_pane_select_member(map, second);
    app.update(Action::SetWorkspaceMap(false));
    assert_eq!(app.active_pane, Some(reader));
    assert_eq!(app.browser_command_target().unwrap().0, second);
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
    app.update(Action::ReadGraphMember(first));
    assert_eq!(app.browser_command_target().unwrap().0, first);
    assert_eq!(app.graph_runtimes.graph().nodes().count(), 2);
}
