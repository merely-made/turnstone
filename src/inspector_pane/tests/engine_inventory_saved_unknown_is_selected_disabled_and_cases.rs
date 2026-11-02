// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn engine_inventory_saved_unknown_is_selected_disabled_and_refuses_click() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    app.browser.entry(member).viewer_override = Some("unknown.lane".into());
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        None,
        "unconfigured",
    );
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let selected = pane.runner.state().radio.selected;
    assert_ne!(selected, 0);
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let _ = pane.scene(400, 2000, 1.0);
    // Match identity and both accessibility states against the same painted
    // row. Estimated probe geometry can land on a different engine option.
    let points: Vec<_> = [
        ("data-engine-id", "unknown.lane"),
        ("aria-checked", "true"),
        ("aria-disabled", "true"),
    ]
    .iter()
    .map(|(name, value)| {
        pane.selector_point(
            &taproot::Selector::class("radio")
                .with_attr(*name, *value)
                .containing("unknown.lane"),
        )
        .expect("saved unknown row is unambiguous")
        .expect("saved unknown selection stays visible with an explicit refusal")
    })
    .collect();
    assert!(points.iter().all(|point| *point == points[0]));
    let (x, y) = points[0];
    assert!(pane.click(x, y, 400, 2000).is_empty());
    assert_eq!(pane.runner.state().radio.selected, selected);
    assert_eq!(
        app.browser.get(member).unwrap().viewer_override.as_deref(),
        Some("unknown.lane")
    );
    assert!(
        crate::observe::snapshot(&app)
            .viewer_choices
            .iter()
            .any(|choice| choice.viewer.as_deref() == Some("unknown.lane") && !choice.selectable)
    );
}

#[test]
fn engine_inventory_human_label_keeps_engine_id_scenario_target() {
    let mut app = App::test_stub();
    app.engine_inventory
        .iter_mut()
        .find(|engine| engine.id == "scrying.web")
        .unwrap()
        .availability = crate::content::EngineAvailability::Unavailable {
        reason: "disabled fixture lane".into(),
    };
    app.update(Action::OpenAddress("https://example.test/".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        None,
        "unconfigured",
    );
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let _ = pane.scene(400, 2000, 1.0);
    let (x, y) = pane
        .selector_point(
            &taproot::Selector::class("radio")
                .with_attr("data-engine-id", "genet.reader")
                .containing("Reader"),
        )
        .expect("Reader row is unambiguous")
        .expect("the compatibility engine id resolves the painted plain label");
    assert_eq!(
        pane.click(x, y, 400, 2000),
        vec![InspectorIntent::SetViewer {
            member,
            viewer: Some("genet.reader".into()),
        }]
    );
    // Unavailable engines stay out of the compact picker unless they are the
    // saved choice. Unknown saved choices have a separate disabled-row test.
    let _ = pane.scene(400, 2000, 1.0);
    assert!(
        pane.selector_point(
            &taproot::Selector::class("radio").with_attr("data-engine-id", "scrying.web")
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn narrow_inspector_probe_selects_registered_scry_for_followed_b_after_a() {
    for reuse_after_on_demand_a in [false, true] {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress(
            "http://127.0.0.1:43123/page-a.html".into(),
        ));
        let a = app.graph_runtimes.focused_member().unwrap();
        let source = app.default_graph_pane();
        app.publish_member_context(source, Some(a));
        app.update(Action::SummonPane(crate::panes::PaneKindId::new(
            crate::panes::kind::INSPECTOR,
        )));
        let inspector = app.active_pane.unwrap();
        let mut pane = InspectorPane::new();
        pane.runner.update(|state| {
            state.viewer_expanded = true;
            state.research_expanded = true;
        });
        if reuse_after_on_demand_a {
            app.engine_inventory
                .iter_mut()
                .find(|engine| engine.id == "scrying.web")
                .unwrap()
                .availability = crate::content::EngineAvailability::OnDemand {
                detail: "Windows Scry is built; construction occurs on selection".into(),
            };
            app.browser.entry(a).viewer_override = Some("scrying.web".into());
            pane.sync(&app, inspector, 256.0, 570.0, None, None, "unconfigured");
            pane.runner.update(|state| {
                state.viewer_expanded = true;
                state.research_expanded = true;
            });
            let _ = pane.scene(256, 570, 1.0);
        }
        app.update(Action::OpenAddress(
            "http://127.0.0.1:43123/page-b.html".into(),
        ));
        let b = app.graph_runtimes.focused_member().unwrap();
        assert_ne!(a, b);
        app.publish_member_context(source, Some(b));
        app.engine_inventory
            .iter_mut()
            .find(|engine| engine.id == "scrying.web")
            .unwrap()
            .availability = crate::content::EngineAvailability::Registered;
        app.content.note_live(
            b,
            Some(crate::content::ContentFacts {
                engine: "genet.livery".into(),
                lane: crate::content::ContentLane::Document,
                structure: None,
                lineage: None,
                capabilities: crate::content::DocumentCapabilityFacts::default(),
            }),
        );
        pane.sync(&app, inspector, 251.0, 570.0, None, None, "unconfigured");
        pane.runner.update(|state| {
            state.viewer_expanded = true;
            state.research_expanded = true;
        });
        assert_eq!(pane.runner.state().member, Some(b));
        pane.runner.update(|state| {
            state.viewer_expanded = true;
            state.research_expanded = true;
        });
        let _ = pane.scene(251, 570, 1.0);
        let selector = taproot::Selector::class("radio")
            .with_attr("data-engine-id", "scrying.web")
            .on_surface("inspector");
        let estimated_point = {
            let dom = pane.dom_ref();
            taproot::resolve_estimated_fixture(
                &[taproot::ProbeSurface {
                    name: "inspector",
                    dom: &dom,
                    rect: [0.0, 0.0, 251.0, 570.0],
                    sheet: crate::ui::CAMBIUM_SHEET,
                }],
                &selector,
            )
            .expect("the registered System webview row is probe-visible")
            .point
        };
        // The document controls above Viewer can wrap below this narrow
        // viewport. Scroll the actual retained row into view before probing
        // its visible point, as the shell does for a user scrolling the pane.
        let (row_x, row_y, row_w, row_h) = {
            let dom = pane.dom_ref();
            let rows = pane.layout.matching_nodes(&dom, &selector);
            assert_eq!(rows.len(), 1, "the registered viewer has one retained row");
            pane.layout.painted_rect(&dom, rows[0]).unwrap()
        };
        pane.scroll_by(
            (row_x + row_w - 251.0).max(0.0),
            (row_y + row_h - 570.0).max(0.0),
        );
        pane.runner.update(|state| {
            state.viewer_expanded = true;
            state.research_expanded = true;
        });
        let _ = pane.scene(251, 570, 1.0);
        let (x, y) = pane
            .selector_point(&selector)
            .unwrap()
            .expect("the painted System webview row is visible");
        eprintln!(
            "narrow Inspector estimated={estimated_point:?}, painted=({x}, {y}), reused={reuse_after_on_demand_a}"
        );
        assert!(
            pane.selector_point(&taproot::Selector::role("radio").on_surface("inspector"))
                .is_err(),
            "a broad selector must refuse ambiguous rows"
        );
        assert!(
            (0.0..251.0).contains(&x) && (0.0..570.0).contains(&y),
            "Scry row outside narrow pane: ({x}, {y}), reused={reuse_after_on_demand_a}"
        );
        assert_eq!(
            pane.click(x, y, 251, 570),
            vec![InspectorIntent::SetViewer {
                member: b,
                viewer: Some("scrying.web".into()),
            }],
            "probe click must write B, reused={reuse_after_on_demand_a}, point=({x}, {y})"
        );
        assert_eq!(
            app.browser
                .get(a)
                .and_then(|state| state.viewer_override.as_deref()),
            reuse_after_on_demand_a.then_some("scrying.web")
        );
        // A short pane clips the row until a real scroll paints it in view.
        // Scenario geometry must follow that scroll, rather than the DOM's
        // unchanged estimated coordinates.
        let (scroll_x, scroll_y) = pane.scroll.offset();
        pane.scroll_by(-scroll_x, -scroll_y);
        pane.sync(&app, inspector, 251.0, 100.0, None, None, "unconfigured");
        pane.runner.update(|state| {
            state.viewer_expanded = true;
            state.research_expanded = true;
        });
        let _ = pane.scene(251, 100, 1.0);
        assert_eq!(pane.selector_point(&selector), Ok(None));
        for _ in 0..16 {
            if pane.selector_point(&selector).unwrap().is_some() {
                break;
            }
            pane.scroll_by(0.0, 80.0);
            pane.runner.update(|state| {
                state.viewer_expanded = true;
                state.research_expanded = true;
            });
            let _ = pane.scene(251, 100, 1.0);
        }
        let (scrolled_x, scrolled_y) = pane
            .selector_point(&selector)
            .unwrap()
            .expect("Scry becomes targetable only once painted inside the scrolled viewport");
        assert!((0.0..251.0).contains(&scrolled_x) && (0.0..100.0).contains(&scrolled_y));
        assert_eq!(
            pane.click(scrolled_x, scrolled_y, 251, 100),
            vec![InspectorIntent::SetViewer {
                member: b,
                viewer: Some("scrying.web".into()),
            }]
        );
    }
}

#[test]
fn engine_inventory_resync_discards_focus_request_when_rows_change() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/".into()));
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        None,
        "unconfigured",
    );
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    pane.runner
        .update(|state| state.radio.focus_request = Some(state.viewer_options.len() - 1));
    let previous_options = pane.runner.state().viewer_options.clone();
    app.engine_inventory
        .iter_mut()
        .find(|engine| engine.id == "weld.chromium")
        .unwrap()
        .availability = crate::content::EngineAvailability::OnDemand {
        detail: "runtime was configured after the earlier inventory".into(),
    };
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        None,
        "unconfigured",
    );
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    assert_eq!(pane.runner.state().radio.focus_request, None);
    assert_ne!(pane.runner.state().viewer_options, previous_options);
}

#[test]
fn engine_inventory_survives_session_adoption() {
    let mut app = App::test_stub();
    let inventory = app.engine_inventory.clone();
    let effects = app.update(Action::NewSession);
    let id = effects
        .iter()
        .find_map(|effect| match effect {
            crate::action::Effect::SwitchSession { id } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.adopt_session(id);
    assert_eq!(app.engine_inventory, inventory);
}

#[test]
fn engine_inventory_valid_selection_recovers_failed_pin_but_keeps_at_rest_closed() {
    for viewer in [None, Some("genet.livery".to_string())] {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/failed".into()));
        let member = app.graph_runtimes.focused_member().unwrap();
        app.browser.entry(member).viewer_override = Some("unknown.lane".into());
        app.content.note_failed(member, "unknown engine".into());
        let effects = app.update(Action::SetViewerOverride { member, viewer });
        assert!(effects.iter().any(|effect| matches!(effect,
            crate::action::Effect::SpawnContent { node, .. } if *node == member)));
        assert!(matches!(
            app.content.get(member),
            Some(crate::content::NodeContent::Requested)
        ));
    }
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/closed".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    // Visiting an address does not itself open its document surface.
    assert!(app.content.get(member).is_none());
    let effects = app.update(Action::SetViewerOverride {
        member,
        viewer: Some("genet.livery".into()),
    });
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, crate::action::Effect::SpawnContent { .. }))
    );
    assert!(app.content.get(member).is_none());
}

#[test]
fn source_binding_disclosure_is_hit_testable_at_narrow_width_and_emits_exact_pin() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress(
        "https://example.test/a-long-document-address".into(),
    ));
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    let owner = app
        .frisket
        .iter_leaves()
        .find(|(_, content, _)| matches!(content, crate::panes::PaneContent::Inspector))
        .unwrap()
        .0;
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    pane.sync(&app, owner, 251.0, 600.0, None, None, "unconfigured");
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let _ = pane.scene(251, 600, 1.0);
    let (x, y) = pane
        .selector_point(&taproot::Selector::role("button").containing("Pane source:"))
        .unwrap()
        .unwrap();
    assert!(x >= 0.0 && x < 251.0 && y >= 0.0 && y < 100.0);
    assert!(pane.click(x, y, 251, 600).is_empty());
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let _ = pane.scene(251, 600, 1.0);
    let (x, y) = pane
        .selector_point(&taproot::Selector::role("button").containing("Pin this document"))
        .unwrap()
        .unwrap();
    let intents = pane.click(x, y, 251, 600);
    let [InspectorIntent::Binding(intent)] = intents.as_slice() else {
        panic!("exactly the source choice must dispatch");
    };
    assert_eq!(intent.pane, owner);
    assert!(matches!(
        intent.choice,
        crate::pane_binding::BindingChoice::Pin {
            member: Some(_),
            ..
        }
    ));
    app.update(Action::SetPaneBinding(*intent));
    pane.sync(&app, owner, 251.0, 600.0, None, None, "unconfigured");
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    assert_eq!(app.pane_binding_snapshot(owner).unwrap().mode, "Pinned");
}

#[test]
fn inspector_starts_with_a_compact_viewer_and_one_capability_table() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/quiet".into()));
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.,
        600.,
        None,
        None,
        "unconfigured",
    );
    let _ = pane.scene(400, 600, 1.0);
    assert!(!pane.runner.state().viewer_expanded);
    assert!(!pane.runner.state().research_expanded);
    assert!(
        pane.selector_point(&taproot::Selector::role("radio").on_surface("inspector"))
            .unwrap()
            .is_none()
    );
    let (x, y) = pane
        .selector_point(
            &taproot::Selector::role("button")
                .containing("Change viewer")
                .on_surface("inspector"),
        )
        .unwrap()
        .unwrap();
    pane.click(x, y, 400, 600);
    assert!(pane.runner.state().viewer_expanded);
    pane.runner.update(|state| {
        state.viewer_expanded = true;
        state.research_expanded = true;
    });
    let _ = pane.scene(400, 600, 1.0);
    assert!(
        pane.runner
            .state()
            .viewer_options
            .iter()
            .all(|option| option.selectable)
    );
}

#[test]
fn live_shared_report_reaches_the_inspector_and_dies_with_the_receiver() {
    use inker::webcue::*;
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://fixture.test/shared".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(node, None);
    let report = CapabilityReport::new(
        BackendIdentity {
            engine: "fixture.shared".into(),
            version: None,
            platform: "fixture-os".into(),
            build_features: vec![],
        },
        PresentationMode::HostDevice,
        Evidence::Constructed,
        SemanticProtocol::Unavailable,
        Feature::ALL.map(|feature| {
            (
                feature,
                if feature == Feature::Zoom {
                    Support::Partial("effective zoom read-back".into())
                } else {
                    Support::Unsupported("not wired by this binding".into())
                },
            )
        }),
    )
    .unwrap();
    app.content.note_cue_report(node, report);
    let rows = crate::inspector_controls::capabilities(&app, Some(node));
    assert_eq!(rows.len(), Feature::ALL.len() + 6);
    assert!(rows.contains(&"Platform: fixture-os".into()));
    assert!(rows.contains(&"Zoom: partial: effective zoom read-back".into()));
    assert!(rows.contains(&"Ime: unavailable: not wired by this binding".into()));
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.,
        600.,
        None,
        None,
        "unconfigured",
    );
    assert!(
        content_lines(&pane)
            .iter()
            .any(|row| row == "Engine: fixture.shared")
    );
    assert!(
        content_lines(&pane)
            .iter()
            .any(|row| row.starts_with("NativeKeyboard: unavailable:"))
    );
    app.content.note_requested(node);
    assert!(app.content.cue_report(node).is_none());
    assert!(
        !crate::inspector_controls::capabilities(&app, Some(node))
            .iter()
            .any(|row| row.contains("fixture.shared"))
    );
}
