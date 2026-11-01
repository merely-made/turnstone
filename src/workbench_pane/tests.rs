// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::action::Action;
use layout_dom_api::LayoutDomMut;
#[test]
fn closing_a_background_tab_uses_its_own_target_and_never_starts_a_drag() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    app.update(Action::WorkbenchActivate(b));
    let owner = app
        .frisket
        .iter_leaves()
        .find(|(_, c, _)| matches!(c, crate::panes::PaneContent::Workbench))
        .unwrap()
        .0;
    let mut pane = WorkbenchPane::new();
    pane.sync(&app, owner, 800., 600.);
    pane.scene(800, 600, 1.0);
    let selector = taproot::Selector::role("button")
        .on_surface("workbench")
        .containing("Close mere://alpha");
    let (x, y) = pane.selector_point(&selector).unwrap().unwrap();
    assert_eq!(pane.tab_at(x, y, 800, 600), None);
    assert!(pane.click(x, y, 800, 600).is_empty());
    let actions = pane.take_read_actions();
    assert!(
        matches!(actions.as_slice(),[Action::Reading(crate::reading_view::ReadAction::Close(crate::reading_view::ReadTarget {pane,member}))] if *pane==owner && *member==a)
    );
    for action in actions {
        app.update(action);
    }
    assert!(!app.workbench_for_pane(owner).unwrap().has_tile(a));
    assert!(app.workbench_for_pane(owner).unwrap().has_tile(b));
    assert_eq!(app.browser_command_target().map(|(m, _)| m), Some(b));
}

fn pane_over(app: &App, w: f32, h: f32) -> WorkbenchPane {
    let mut pane = WorkbenchPane::new();
    pane.sync(app, app.default_graph_pane(), w, h);
    pane.scene(w as u32, h as u32, 1.0);
    pane
}

fn app_with_two_tiles() -> (App, Uuid, Uuid) {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    let a = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.update(Action::OpenAddress("mere://beta".to_string()));
    let b = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    (app, a, b)
}

#[test]
fn native_read_toolbar_routes_controls_outside_its_document_hole() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://reading".into()));
    app.update(Action::OpenInWorkbench);
    let member = app.graph_runtimes.focused_member().unwrap();
    app.apply_update(crate::action::Update::ContentSpawned {
        node: member,
        facts: Some(crate::content::ContentFacts {
            engine: "genet.reader".into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    });
    let pane_id = app
        .frisket
        .iter_leaves()
        .find(|(_, content, _)| matches!(content, crate::panes::PaneContent::Workbench))
        .unwrap()
        .0;
    let mut chrome = app.shell_chrome_config().clone();
    chrome.layout_scales.insert(0, 2.0);
    app.set_shell_chrome_config(chrome);
    let mut pane = WorkbenchPane::new();
    pane.sync(&app, pane_id, 1200., 800.);
    pane.scene(1200, 800, 2.0);
    let selector = taproot::Selector::role("button")
        .on_surface("workbench")
        .containing("Larger reading text");
    assert!(
        pane.selector_point(&selector).unwrap().is_none(),
        "appearance controls start folded"
    );
    let aa = taproot::Selector::role("button")
        .on_surface("workbench")
        .containing("Reading appearance");
    let (x, y) = pane.selector_point(&aa).unwrap().unwrap();
    pane.click(x, y, 1200, 800);
    for action in pane.take_read_actions() {
        app.update(action);
    }
    pane.sync(&app, pane_id, 1200., 800.);
    pane.scene(1200, 800, 2.0);
    let point = pane.selector_point(&selector).unwrap().unwrap();
    let cell = &pane.tiling().cells[0];
    assert!(cell.toolbar_rect().contains(point.0, point.1));
    assert!(!cell.body().contains(point.0, point.1));
    let target = cell
        .active_member()
        .map(|member| crate::reading_view::ReadTarget {
            pane: pane_id,
            member,
        })
        .unwrap();
    pane.click(point.0, point.1, 1200, 800);
    let actions = pane.take_read_actions();
    assert_eq!(actions.len(), 1);
    for action in actions {
        app.update(action);
    }
    assert_eq!(app.reading.preferences(target).text_size, 20);
    assert_eq!(
        app.shell_chrome_config().appearance.ui_zoom,
        1.1,
        "native density is not a persisted zoom edit"
    );
}

#[test]
fn all_document_tab_edges_keep_painted_hits_and_native_apertures_aligned() {
    use crate::chrome_layout::TabEdge;
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    for edge in [TabEdge::Top, TabEdge::Bottom, TabEdge::Left, TabEdge::Right] {
        let mut config = app.shell_chrome_config().clone();
        config.layout.document_tabs = edge;
        app.set_shell_chrome_config(config);
        let pane = pane_over(&app, 800.0, 600.0);
        let cell = &pane.tiling().cells[0];
        let dom = pane.dom.borrow();
        let node = dom.all_with_class(dom.document(), "wb-body")[0];
        let (x, y, w, h) = pane.layout.visible_rect(&dom, node).unwrap();
        let expected = cell.body();
        for (actual, want) in [x, y, w, h]
            .into_iter()
            .zip([expected.x, expected.y, expected.w, expected.h])
        {
            assert!((actual - want).abs() < 0.1, "{edge:?}: {actual} != {want}");
        }
        drop(dom);
        for (name, member) in [("alpha", a), ("beta", b)] {
            let point = pane
                .selector_point(
                    &taproot::Selector::class("tab")
                        .containing(name)
                        .on_surface("workbench"),
                )
                .unwrap()
                .unwrap();
            assert!(cell.tab_rect().contains(point.0, point.1));
            assert_eq!(pane.tab_at(point.0, point.1, 800, 600), Some(member));
        }
    }
}

#[test]
fn long_page_titles_cannot_claim_the_siblings_tab() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    for (member, name) in [(a, "Scry A"), (b, "Scry B")] {
        app.update(Action::ContentTitleChanged {
            member,
            title: format!("{name} {}", "long page state ".repeat(64)),
        });
    }
    let pane = pane_over(&app, 800.0, 600.0);
    for (member, name) in [(a, "Scry A"), (b, "Scry B")] {
        let point = pane
            .selector_point(
                &taproot::Selector::class("tab")
                    .containing(name)
                    .on_surface("workbench"),
            )
            .unwrap()
            .expect("the visible tab has a target");
        let cell = pane
            .tiling()
            .cells
            .iter()
            .find(|cell| cell.members.contains(&member))
            .unwrap();
        assert!(
            cell.rect.contains(point.0, point.1),
            "tab hit stays in its owning cell"
        );
        assert_eq!(pane.tab_at(point.0, point.1, 800, 600), Some(member));
    }
    assert!(
        pane.selector_point(&taproot::Selector::class("tab").on_surface("workbench"))
            .is_err()
    );
}

#[test]
fn singleton_cells_hide_strips_and_keep_one_header_close_per_reading() {
    let (app, _, _) = app_with_two_tiles();
    let pane = pane_over(&app, 800., 600.);
    assert_eq!(pane.tiling().cells.len(), 2);
    let dom = pane.dom.borrow();
    let root = dom.document();
    assert!(dom.all_with_class(root, "tab").is_empty());
    assert_eq!(dom.all_with_class(root, "read-header-trailing").len(), 2);
    for cell in &pane.tiling().cells {
        assert_eq!(cell.tab_rect().w, 0.);
        assert_eq!(cell.toolbar_rect().y, cell.rect.y);
    }
}
#[test]
fn a_singleton_keeps_a_header_grab_region_clear_of_appearance_and_close_controls() {
    let (app, a, _) = app_with_two_tiles();
    let pane = pane_over(&app, 800., 600.);
    let cell = &pane.tiling().cells[0];
    let toolbar = cell.toolbar_rect();
    assert_eq!(
        pane.tab_at(toolbar.x + toolbar.w * 0.5, toolbar.y + 8., 800, 600),
        Some(a)
    );
    let close = taproot::Selector::role("button")
        .on_surface("workbench")
        .containing("Close reading: mere://alpha");
    let (x, y) = pane.selector_point(&close).unwrap().unwrap();
    assert_eq!(pane.tab_at(x, y, 800, 600), None);
}
#[test]
fn tabs_after_a_hidden_singleton_still_resolve_to_their_actual_members() {
    let (mut app, _, b) = app_with_two_tiles();
    app.update(Action::OpenAddress("mere://gamma".into()));
    let c = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.update(Action::WorkbenchStackOnto {
        dragged: c,
        target: b,
    });
    let pane = pane_over(&app, 800., 600.);
    for (name, member) in [("beta", b), ("gamma", c)] {
        let (x, y) = pane
            .selector_point(
                &taproot::Selector::class("tab")
                    .on_surface("workbench")
                    .containing(name),
            )
            .unwrap()
            .unwrap();
        assert_eq!(pane.tab_at(x, y, 800, 600), Some(member));
    }
}
#[test]
fn a_multi_tab_cell_has_tab_closes_and_no_duplicate_header_close() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    let pane = pane_over(&app, 800., 600.);
    let dom = pane.dom.borrow();
    let root = dom.document();
    assert_eq!(dom.all_with_class(root, "tab-close").len(), 2);
    assert!(dom.all_with_class(root, "read-header-trailing").is_empty());
}

#[test]
fn selector_resolution_waits_for_current_paint() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    let mut pane = WorkbenchPane::new();
    pane.sync(&app, app.default_graph_pane(), 800.0, 600.0);
    let selector = taproot::Selector::class("tab").containing("alpha");
    let rect = [0.0, 0.0, 800.0, 600.0];
    assert_eq!(
        pane.resolve(&selector, rect),
        None,
        "no painted snapshot yet"
    );
    pane.scene(800, 600, 1.0);
    assert!(pane.resolve(&selector, rect).is_some());

    let tab = {
        let dom = pane.dom.borrow();
        taproot::matching_classes(&dom, &selector)[0]
    };
    pane.dom
        .borrow_mut()
        .set_attribute(tab, cambium::attr_qual("data-paint-revision"), "changed");
    assert_eq!(pane.resolve(&selector, rect), None, "old geometry is stale");
    pane.scene(800, 600, 1.0);
    assert!(pane.resolve(&selector, rect).is_some());
}

/// The press resolution: a point inside a drawn tab maps back to its
/// member — the tab-drag gesture's anchor.
#[test]
fn a_tab_point_resolves_to_its_member() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    let pane = pane_over(&app, 800.0, 600.0);
    let (ax, ay) = pane
        .resolve(
            &taproot::Selector::class("tab").containing("alpha"),
            [0.0, 0.0, 800.0, 600.0],
        )
        .unwrap();
    assert_eq!(pane.tab_at(ax, ay, 800, 600), Some(a));
    let (bx, by) = pane
        .resolve(
            &taproot::Selector::class("tab").containing("beta"),
            [0.0, 0.0, 800.0, 600.0],
        )
        .unwrap();
    assert_eq!(pane.tab_at(bx, by, 800, 600), Some(b));
    assert_eq!(
        pane.tab_at(400.0, 300.0, 800, 600),
        None,
        "a body point is no tab"
    );
}

/// A click on an inactive tab of a stacked cell reports the activation
/// for the shell to lower — the strip's own selection is the signal.
#[test]
fn clicking_an_inactive_tab_reports_activation() {
    let (mut app, a, b) = app_with_two_tiles();
    app.update(Action::WorkbenchStackOnto {
        dragged: b,
        target: a,
    });
    // b landed active; a's tab is the inactive one.
    let mut pane = pane_over(&app, 800.0, 600.0);
    assert_eq!(pane.tiling().cells.len(), 1);
    let (x, y) = pane
        .resolve(
            &taproot::Selector::class("tab").containing("alpha"),
            [0.0, 0.0, 800.0, 600.0],
        )
        .expect("alpha's tab is drawn in the stack");
    let acts = pane.click(x, y, 800, 600);
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].0, a, "clicking alpha's tab activates alpha");
}

#[test]
fn long_failure_hint_wraps_inside_the_document_body() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://failure".into()));
    app.update(Action::OpenInWorkbench);
    let member = app.graph_runtimes.focused_member().unwrap();
    let error = format!(
        "profile identity is pending: {}",
        "long-unavailable-source ".repeat(6)
    );
    app.content.note_failed(member, error.clone());
    let pane = pane_over(&app, 400.0, 400.0);
    let dom = pane.dom.borrow();
    let body = dom.all_with_class(dom.document(), "wb-body")[0];
    assert!(dom.outer_html(body).contains(&error));
    let text = dom.all_with_class(body, "wb-hint")[0];
    let (_, _, body_w, body_h) = pane.layout.visible_rect(&dom, body).unwrap();
    let (_, _, text_w, text_h) = pane
        .layout
        .visible_rect(&dom, text)
        .expect("hint text is laid out");
    assert!(
        text_w <= body_w,
        "hint exceeds body width: {text_w} > {body_w}"
    );
    assert!(
        text_h > 20.0 && text_h <= body_h,
        "hint should occupy multiple visible lines: {text_h}"
    );
}

#[test]
fn tall_failure_hint_scrolls_within_its_body_and_clamps() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://failure".into()));
    app.update(Action::OpenInWorkbench);
    let member = app.graph_runtimes.focused_member().unwrap();
    app.content
        .note_failed(member, "unavailable source identity ".repeat(200));
    let mut pane = pane_over(&app, 400.0, 240.0);
    let (x, y) = {
        let dom = pane.dom.borrow();
        let body = dom.all_with_class(dom.document(), "wb-body")[0];
        let (x, y, w, h) = pane.layout.visible_rect(&dom, body).unwrap();
        (x + w / 2.0, y + h / 2.0)
    };
    assert!(pane.scroll_body_at(x, y, -100_000.0));
    pane.scene(400, 240, 1.0);
    let tail = {
        let mut dom = pane.dom.borrow_mut();
        let body = dom.all_with_class(dom.document(), "wb-body")[0];
        let (bx, by, bw, bh) = pane.layout.visible_rect(&dom, body).unwrap();
        let mut max_byte = 0;
        for row in 0..8 {
            for column in 0..8 {
                if let Some(position) = pane.layout.text_caret_at(
                    &mut dom,
                    body,
                    bx + 14.0 + column as f32 * (bw - 28.0) / 8.0,
                    by + 14.0 + row as f32 * (bh - 28.0) / 8.0,
                ) {
                    max_byte = max_byte.max(position.byte);
                }
            }
        }
        max_byte
    };
    assert!(
        tail > 4_000,
        "end of long error must be painted: byte {tail}"
    );
    let end = pane.layout.scroll_stamp();
    assert!(
        end.iter().any(|(_, _, y)| f32::from_bits(*y) > 0.0),
        "downward wheel must move the body: {end:?}"
    );
    assert!(pane.scroll_body_at(x, y, -100_000.0));
    assert_eq!(
        pane.layout.scroll_stamp(),
        end,
        "offset is clamped at the end"
    );
    assert!(pane.scroll_body_at(x, y, 100_000.0));
    assert_ne!(
        pane.layout.scroll_stamp(),
        end,
        "the beginning remains reachable"
    );
}

#[test]
fn mounted_failure_body_scrolls_in_shared_scaled_layout() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://failure".into()));
    app.update(Action::OpenInWorkbench);
    let member = app.graph_runtimes.focused_member().unwrap();
    app.content
        .note_failed(member, "unavailable source identity ".repeat(200));
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let mount = cambium::RetainedMount::new(dom.clone());
    let mut pane = WorkbenchPane::new_in(Some(mount.clone()));
    let sibling_mount = cambium::RetainedMount::new(dom.clone());
    let mut sibling = WorkbenchPane::new_in(Some(sibling_mount.clone()));
    let window = dom.borrow_mut().create_element(cambium::html_qual("div"));
    let host = dom.borrow_mut().create_element(cambium::html_qual("div"));
    {
        let mut dom = dom.borrow_mut();
        let document = dom.document();
        dom.append_child(document, window);
        dom.append_child(window, host);
        dom.set_attribute(
            host,
            cambium::attr_qual("style"),
            "position:relative;transform:translate(50px,30px);width:200px;height:120px;",
        );
        dom.append_child(host, mount.node());
        dom.append_child(host, sibling_mount.node());
    }
    sibling.sync(&app, app.default_graph_pane(), 400.0, 240.0);
    pane.sync(&app, app.default_graph_pane(), 400.0, 240.0);
    let shared = Rc::new(RefCell::new(crate::ui::RetainedLayout::new_at(Some(
        window,
    ))));
    shared.borrow_mut().set_scale(2.0);
    pane.bind_inline(&shared);
    sibling.bind_inline(&shared);
    shared
        .borrow_mut()
        .scene(&mut dom.borrow_mut(), workbench_sheet(), 800, 600);
    let (x, y, before) = {
        let dom = dom.borrow();
        let body = dom.all_with_class(host, "wb-body")[0];
        let (x, y, w, h) = pane.layout.visible_rect(&dom, body).unwrap();
        let host_rect = shared.borrow().painted_rect(&dom, mount.node()).unwrap();
        assert!(
            host_rect.0 > 0.0 && host_rect.1 > 0.0,
            "fixture has a nonzero mount offset: {host_rect:?}"
        );
        (x + w / 2.0, y + h / 2.0, shared.borrow().scroll_stamp())
    };
    assert!(pane.scroll_body_at(x, y, -100_000.0));
    let after = shared.borrow().scroll_stamp();
    assert_ne!(
        before, after,
        "mounted pane updates the shared scroll owner"
    );
    assert!(after.iter().any(|(_, _, y)| f32::from_bits(*y) > 0.0));
    app.content
        .note_failed(member, "a changed sibling error".into());
    sibling.sync(&app, app.default_graph_pane(), 400.0, 240.0);
    assert_eq!(
        shared.borrow().scroll_stamp(),
        after,
        "refreshing a sibling preserves this pane's scroll"
    );
    shared
        .borrow_mut()
        .scene(&mut dom.borrow_mut(), workbench_sheet(), 800, 600);
    let dom = dom.borrow();
    let body = dom.all_with_class(host, "wb-body")[0];
    assert!(
        pane.layout.visible_rect(&dom, body).is_some(),
        "body remains visible after scrolling"
    );
}
