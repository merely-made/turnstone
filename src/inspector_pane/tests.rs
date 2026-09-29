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
    let (x, y) = {
        let dom = pane.dom.borrow();
        taproot::resolve(
            &[taproot::ProbeSurface {
                name: "inspector",
                dom: &dom,
                rect: [0.0, 0.0, 509.0, 576.0],
                sheet: crate::ui::CAMBIUM_SHEET,
            }],
            &taproot::Selector::role("button").containing("Clip document"),
        )
        .expect("Probe must resolve the configured clip button")
        .point
    };
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
                    .any(|c| dom.text(c).is_some_and(|t| t.contains("livery")))
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
fn index_mapping_round_trips() {
    assert_eq!(crate::inspector_controls::viewer_for_index(0), None);
    assert_eq!(
        crate::inspector_controls::viewer_for_index(1).as_deref(),
        Some("genet.livery")
    );
    assert_eq!(crate::inspector_controls::index_for_viewer(None), 0);
    assert_eq!(
        crate::inspector_controls::index_for_viewer(Some("genet.livery")),
        1
    );
    assert_eq!(
        crate::inspector_controls::index_for_viewer(Some("genet.web")),
        0,
        "the retired lane is not selectable"
    );
    assert_eq!(
        crate::inspector_controls::index_for_viewer(Some("unknown.lane")),
        0,
        "unknown shows Auto"
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
    let (x, y) = {
        let dom = pane.dom.borrow();
        taproot::resolve(
            &[taproot::ProbeSurface {
                name: "inspector",
                dom: &dom,
                rect: [0.0, 0.0, 400.0, 600.0],
                sheet: crate::ui::CAMBIUM_SHEET,
            }],
            &taproot::Selector::class("radio").containing("genet.reader"),
        )
        .unwrap()
        .point
    };
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
