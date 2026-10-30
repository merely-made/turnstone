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
