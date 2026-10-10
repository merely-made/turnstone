// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;
use layout_dom_api::LayoutDom;

/// The pane draws the sections it was synced with: headers and key/value
/// rows land in the DOM under the panel's classes.
#[test]
fn synced_sections_reach_the_dom() {
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.sections = vec![InspectorSection {
            title: "Node".to_string(),
            rows: vec![("URL".to_string(), "https://example.test/".to_string())],
        }];
        state.clip_target = Some("file:///notes/field.knot".into());
        state.clip_source_available = true;
        state.clip_status = "ready".into();
        state.viewport_w = 400.0;
        state.viewport_h = 600.0;
    });
    let dom = pane.dom.borrow();
    let rows = dom.all_with_class(dom.document(), "detail-row");
    assert_eq!(rows.len(), 1);
    let values = dom.all_with_class(dom.document(), "detail-value");
    let text: String = values
        .iter()
        .flat_map(|&n| dom.dom_children(n))
        .filter_map(|c| dom.text(c).map(str::to_string))
        .collect();
    assert!(text.contains("https://example.test/"));
    assert!(text.contains("Knot clip: ready"));
}

#[test]
fn clip_button_bubbles_a_typed_inspector_intent() {
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.clip_target = Some("file:///notes/field.knot".into());
        state.clip_source_available = true;
        state.viewport_w = 400.0;
        state.viewport_h = 600.0;
    });
    let (x, y) = {
        let dom = pane.dom.borrow();
        let button = dom
            .dom_children(pane.runner.root())
            .find(|&node| {
                dom.element_name(node)
                    .is_some_and(|name| name.local.as_ref() == "button")
            })
            .expect("the inspector action is a real button");
        let (x, y, w, h) =
            crate::ui::node_rect(&dom, button, crate::ui::CAMBIUM_SHEET, 400, 600).unwrap();
        (x + w / 2.0, y + h / 2.0)
    };
    assert_eq!(
        pane.click(x, y, 400, 600),
        vec![InspectorIntent::ClipToKnot]
    );
}

#[test]
fn probe_resolved_clip_button_reaches_the_pane_at_receipt_size() {
    let mut pane = InspectorPane::new();
    pane.runner.update(|state| {
        state.sections = vec![InspectorSection {
            title: "Node".to_string(),
            rows: (0..14)
                .map(|index| (format!("Field {index}"), format!("Value {index}")))
                .collect(),
        }];
        state.clip_target = Some("clip_target_receipt.knot".into());
        state.clip_source_available = true;
        state.clip_status = "ready".into();
        state.viewport_w = 509.0;
        state.viewport_h = 576.0;
    });
    let _ = pane.scene(509, 576);
    let (x, y) = pane
        .selector_point(
            &taproot::Selector::role("button")
                .containing("Clip document")
                .on_surface("inspector"),
        )
        .expect("the configured clip button must resolve uniquely")
        .expect("the configured clip button must be painted inside its pane");
    assert!(
        (0.0..509.0).contains(&x) && (0.0..576.0).contains(&y),
        "Probe resolved the clip button outside its pane: ({x}, {y})"
    );
    assert_eq!(
        pane.click(x, y, 509, 576),
        vec![InspectorIntent::ClipToKnot]
    );
}
/// A click on a radio option reports the viewer intent — and the sidecar
/// mirror round-trips (sync shows what the app persisted).
#[test]
fn picking_a_viewer_reports_the_intent_and_mirrors_back() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/x".to_string()));
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    // Resolve the livery option's centre off the laid-out DOM.
    let (x, y) = {
        let dom = pane.dom.borrow();
        let radio = dom
            .all_with_class(dom.document(), "radio")
            .into_iter()
            .find(|&n| {
                dom.dom_children(n)
                    .any(|c| dom.text(c).is_some_and(|t| t == "( ) Genet"))
            })
            .expect("the livery option is drawn");
        let (rx, ry, rw, rh) =
            crate::ui::node_rect(&dom, radio, crate::ui::CAMBIUM_SHEET, 400, 600).unwrap();
        (rx + rw / 2.0, ry + rh / 2.0)
    };
    let intents = pane.click(x, y, 400, 600);
    assert!(
        matches!(&intents[..], [InspectorIntent::SetViewer { member: selected, viewer: Some(v) }] if v == "genet.livery" && Some(*selected) == app.graph_runtimes.focused_member()),
        "picking livery reports the pinned viewer"
    );
    // The app applies it; a re-sync mirrors the persisted truth back.
    let member = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::SetViewerOverride {
        member,
        viewer: Some("genet.livery".to_string()),
    });
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    assert_eq!(
        pane.runner.state().radio.selected,
        1,
        "the sidecar mirrors back"
    );
    assert!(
        pane.click(x, y, 400, 600).is_empty(),
        "re-picking the same is no intent"
    );
}

#[test]
fn engine_inventory_mapping_preserves_unknown_saved_pin() {
    let app = App::test_stub();
    let options = crate::inspector_controls::viewer_options(&app, Some("unknown.lane"));
    assert_eq!(
        crate::inspector_controls::viewer_for_index(&options, 0),
        Some(None)
    );
    assert_eq!(
        crate::inspector_controls::viewer_for_index(&options, 1),
        Some(Some("genet.livery".into()))
    );
    assert_eq!(
        crate::inspector_controls::index_for_viewer(&options, None),
        0
    );
    assert_eq!(
        crate::inspector_controls::index_for_viewer(&options, Some("genet.livery")),
        1
    );
    let unknown = crate::inspector_controls::index_for_viewer(&options, Some("unknown.lane"));
    assert_ne!(unknown, 0, "unknown must not look like Auto");
    assert!(options[unknown].label.contains("unknown.lane"));
    assert!(options[unknown].label.contains("unavailable"));
    assert_eq!(
        crate::inspector_controls::viewer_for_index(&options, unknown),
        None
    );
}

#[test]
fn focused_document_capabilities_sit_beside_the_viewer_picker() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.com/x".to_string()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: "genet.livery".into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: crate::content::DocumentCapabilityFacts {
                find_in_page: crate::content::CapabilityStatus::Supported,
                navigation: crate::content::CapabilityStatus::Partial {
                    detail: "host-owned lineage".into(),
                },
                ..Default::default()
            },
        }),
    );
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    assert!(
        pane.runner
            .state()
            .capabilities
            .iter()
            .any(|line| line == "Find in page: supported")
    );
    assert!(
        pane.runner
            .state()
            .capabilities
            .iter()
            .any(|line| { line == "Navigation controls: partial: host-owned lineage" })
    );
}

/// The requested page zoom rides beside the capability, labelled as the
/// request it is: nothing reads the effective level back yet.
#[test]
fn the_requested_page_zoom_is_mirrored_beside_the_capability() {
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
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    assert!(
        pane.runner
            .state()
            .capabilities
            .iter()
            .any(|line| line == "Requested page zoom: 100%")
    );

    app.update(Action::PageZoomOut { member: node });
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    assert!(
        pane.runner
            .state()
            .capabilities
            .iter()
            .any(|line| line == "Requested page zoom: 90%"),
        "{:?}",
        pane.runner.state().capabilities
    );

    // A retained session answers with the level it settled on, and that
    // rides beside the request rather than replacing it.
    app.content.note_page_zoom(
        node,
        crate::content::PageZoomFacts {
            requested: 0.9,
            applied: 0.9,
            min: 0.25,
            max: 5.0,
        },
    );
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        600.0,
        None,
        false,
        "unconfigured",
    );
    assert!(
        pane.runner
            .state()
            .capabilities
            .iter()
            .any(|line| line == "Requested page zoom: 90% (applied 90%)"),
        "{:?}",
        pane.runner.state().capabilities
    );
}
#[test]
fn viewer_controls_write_the_followed_member_instead_of_the_runtime_cursor() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://followed".into()));
    let followed = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenAddress("mere://cursor".into()));
    let cursor = app.graph_runtimes.focused_member().unwrap();
    assert_ne!(followed, cursor);
    let source = app.default_graph_pane();
    app.publish_member_context(source, Some(followed));
    app.update(Action::SummonPane(crate::panes::PaneKindId::new(
        crate::panes::kind::INSPECTOR,
    )));
    let id = app.active_pane.unwrap();
    let mut pane = InspectorPane::new();
    pane.sync(&app, id, 400.0, 600.0, None, false, "unconfigured");
    assert_eq!(pane.runner.state().member, Some(followed));
    let _ = pane.scene(400, 600);
    let (x, y) = pane
        .selector_point(
            &taproot::Selector::class("radio")
                .with_attr("data-engine-id", "genet.reader")
                .on_surface("inspector"),
        )
        .expect("the requested viewer must resolve uniquely")
        .expect("the requested viewer must be painted inside its pane");
    assert!(
        (0.0..400.0).contains(&x) && (0.0..600.0).contains(&y),
        "Probe resolved the viewer outside its pane: ({x}, {y})"
    );
    let intents = pane.click(x, y, 400, 600);
    let [InspectorIntent::SetViewer { member, viewer }] = &intents[..] else {
        panic!("typed viewer write")
    };
    assert_eq!(*member, followed);
    let effects = app.update(Action::SetViewerOverride {
        member: *member,
        viewer: viewer.clone(),
    });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, crate::action::Effect::SaveSession))
    );
    assert_eq!(
        app.browser
            .get(followed)
            .unwrap()
            .viewer_override
            .as_deref(),
        Some("genet.reader")
    );
    assert!(
        app.browser
            .get(cursor)
            .and_then(|state| state.viewer_override.as_deref())
            .is_none()
    );
    pane.sync(&app, id, 400.0, 600.0, None, false, "unconfigured");
    assert_eq!(pane.runner.state().radio.selected, 2);
}

struct AddedDocumentEngine;

impl inker::SessionEngine<netrender::Scene> for AddedDocumentEngine {
    fn engine_id(&self) -> &str {
        "example.document"
    }

    fn spawn(
        &self,
        _: &inker::SessionSpawnRequest,
    ) -> Result<Box<dyn inker::DocumentSession<netrender::Scene>>, inker::SessionError> {
        Err(inker::SessionError::SpawnFailed(
            "inventory fixture does not spawn".into(),
        ))
    }
}

#[test]
fn engine_inventory_new_registration_reaches_picker_and_observation() {
    let mut registry = crate::shell::standard_content_engines();
    registry.register(Box::new(AddedDocumentEngine));
    let mut app = App::test_stub();
    app.engine_inventory =
        crate::shell::project_engine_inventory(&registry, &inker::SurfaceEngineRegistry::new());
    let options = crate::inspector_controls::viewer_options(&app, None);
    let option = options
        .iter()
        .find(|option| option.viewer.as_deref() == Some("example.document"))
        .expect("a registration must not require editing a picker list");
    assert!(option.selectable);
    let observed = crate::observe::snapshot(&app);
    assert_eq!(observed.engine_inventory, app.engine_inventory);
    assert_eq!(observed.viewer_choices, options);
    assert!(
        observed
            .engine_inventory
            .iter()
            .any(|engine| engine.id == "scrying.web"
                && if cfg!(all(feature = "scry", windows)) {
                    matches!(
                        engine.availability,
                        crate::content::EngineAvailability::OnDemand { .. }
                    )
                } else {
                    matches!(
                        engine.availability,
                        crate::content::EngineAvailability::Unavailable { .. }
                    )
                })
    );
}

#[test]
fn engine_inventory_saved_unknown_is_selected_disabled_and_refuses_click() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    app.browser.entry(member).viewer_override = Some("unknown.lane".into());
    let mut pane = InspectorPane::new();
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        false,
        "unconfigured",
    );
    let selected = pane.runner.state().radio.selected;
    assert_ne!(selected, 0);
    let _ = pane.scene(400, 2000);
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
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        false,
        "unconfigured",
    );
    let _ = pane.scene(400, 2000);
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
    // The first click mutated the retained DOM. Repaint before targeting the
    // unavailable row so the test follows the current presented geometry.
    let _ = pane.scene(400, 2000);
    let (x, y) = pane
        .selector_point(
            &taproot::Selector::class("radio").with_attr("data-engine-id", "scrying.web"),
        )
        .expect("System webview row is unambiguous")
        .expect("unavailable System webview row remains painted");
    assert!(
        pane.click(x, y, 400, 2000).is_empty(),
        "an unavailable row cannot repeat an earlier unsynced selection"
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
        if reuse_after_on_demand_a {
            app.engine_inventory
                .iter_mut()
                .find(|engine| engine.id == "scrying.web")
                .unwrap()
                .availability = crate::content::EngineAvailability::OnDemand {
                detail: "Windows Scry is built; construction occurs on selection".into(),
            };
            app.browser.entry(a).viewer_override = Some("scrying.web".into());
            pane.sync(&app, inspector, 256.0, 570.0, None, false, "unconfigured");
            let _ = pane.scene(256, 570);
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
        pane.sync(&app, inspector, 251.0, 570.0, None, false, "unconfigured");
        assert_eq!(pane.runner.state().member, Some(b));
        let _ = pane.scene(251, 570);
        let selector = taproot::Selector::class("radio")
            .with_attr("data-engine-id", "scrying.web")
            .on_surface("inspector");
        let estimated_point = {
            let dom = pane.dom_ref();
            taproot::resolve(
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
        pane.sync(&app, inspector, 251.0, 100.0, None, false, "unconfigured");
        let _ = pane.scene(251, 100);
        assert_eq!(pane.selector_point(&selector), Ok(None));
        pane.scroll_by(0.0, 200.0);
        let _ = pane.scene(251, 100);
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
    pane.sync(
        &app,
        app.default_graph_pane(),
        400.0,
        2000.0,
        None,
        false,
        "unconfigured",
    );
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
        false,
        "unconfigured",
    );
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
