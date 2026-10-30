// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn field_preview_is_attached_to_its_measured_graph_aperture_and_keeps_graph_state() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://field.test/document".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ReadGraphMember(member));
    app.update(Action::SetWorkspaceMap(true));
    let pane = app.active_pane.unwrap();
    app.graph_pane_frame(pane, 800, 420).unwrap();
    app.with_graph_pane(pane, |c| {
        c.set_camera(mere::canvas::CameraView {
            offset: (120., 90.),
            zoom: 0.5,
        })
    });
    let before = app
        .with_graph_pane(pane, |c| (c.viewport(), c.selected_members()))
        .unwrap();
    let surfaces = [crate::surface::Surface {
        id: crate::surface::SurfaceId::graph(pane),
        kind: crate::surface::SurfaceKind::Graph(pane),
        rect: crate::surface::Rect::new(160., 100., 800., 420.),
    }];
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024., 700.)]);
    chrome.sync_forme_fields(&app, 0, &surfaces);
    let entry = &chrome.runner.state().forme_fields[&0][0];
    let [x, y, width] = entry.field.placement().unwrap();
    assert!(x >= entry.field.aperture[0] && y >= entry.field.aperture[1]);
    assert!(x + width <= entry.field.aperture[2]);
    let _ = chrome.scene(0, 1024, 700);
    let scale = app.shell_chrome_config().layout_scale(0);
    let (x, y) = toolbar_point(
        &chrome,
        "Preview field: Reading layout",
        (1024. / scale) as u32,
        (700. / scale) as u32,
    );
    assert!(
        surfaces[0].rect.contains(x * scale, y * scale),
        "the painted control must stay in its graph aperture"
    );
    let intents = chrome.click(0, x * scale, y * scale, 1024, 700);
    let [ChromeIntent::FocusCard(crate::focus_card::Command::Peek(target))] = intents.as_slice()
    else {
        panic!("{intents:?}")
    };
    app.update(Action::FocusCard(crate::focus_card::Command::Peek(
        target.clone(),
    )));
    assert_eq!(
        app.with_graph_pane(pane, |c| (c.viewport(), c.selected_members()))
            .unwrap(),
        before
    );
    chrome.sync_forme_fields(&app, 0, &[]);
    assert!(chrome.runner.state().forme_fields[&0].is_empty());
}

#[test]
fn chrome_is_laid_out_in_layout_pixels_per_window_and_hit_in_physical_ones() {
    let mut app = App::test_stub();
    let mut config = app.shell_chrome_config().clone();
    config.layout_scales.insert(0, 2.0);
    config.layout_scales.insert(1, 1.0);
    app.set_shell_chrome_config(config.clone());
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1600., 1200.), (1, 800., 600.)]);
    let primary = workspace_area_for_window(&config, 0, 1600, 1200);
    let lens = workspace_area_for_window(&config, 1, 800, 600);
    assert!(
        (primary.y - 2.0 * lens.y).abs() < 0.01,
        "the plan reserves the same chrome in physical pixels"
    );
    let windows = &chrome.runner.state().windows;
    assert_eq!(
        (windows[0].scale, windows[0].w, windows[0].h),
        (2.0, 800.0, 600.0)
    );
    assert_eq!(
        (windows[1].scale, windows[1].w, windows[1].h),
        (1.0, 800.0, 600.0)
    );
    assert_eq!(
        config.appearance.ui_zoom, 1.1,
        "native density is not a persisted zoom edit"
    );

    // The scene is in layout pixels (rasterized at 2x); the address field's
    // physical centre, found through the retained layout, activates it.
    let scene = chrome.scene(0, 1600, 1200);
    assert_eq!((scene.viewport_width, scene.viewport_height), (800, 600));
    let id = chrome.projections[0];
    let root = chrome.runner.window_root(id).unwrap();
    let (x, y) = chrome.layouts[0]
        .selector_point(
            &chrome.dom.borrow(),
            root,
            &taproot::Selector::class("scope-location-edit"),
        )
        .expect("the address field is on screen");
    assert!(
        y < primary.y,
        "the control sits in the chrome strip above the physical workspace"
    );
    let intents = chrome.click(0, x, y, 1600, 1200);
    assert!(intents.iter().any(|intent| matches!(
        intent,
        ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::EditLocation)
    )));
    let (x, y) = (x / 2.0, y / 2.0);
    assert!(
        !chrome
            .click(0, x, y, 1600, 1200)
            .iter()
            .any(|intent| matches!(
                intent,
                ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::EditLocation)
            )),
        "the same point read as physical misses the field"
    );
}

#[test]
fn toolbar_is_default_and_panes_switches_activity_navigation_without_duplicate_controls() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(!app.shell_chrome_config().layout.activity_rail);
    assert_eq!(
        workspace_area(app.shell_chrome_config(), true, 1024, 600).x,
        0.0
    );
    assert!(
        chrome
            .dom
            .borrow()
            .all_with_class(chrome.dom.borrow().document(), "turnstone-activity-rail")
            .is_empty()
    );
    let (_, y) = toolbar_point(&chrome, "Down:", 1024, 600);
    assert!(y < workspace_area(app.shell_chrome_config(), true, 1024, 600).y);
    let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    let (x, y) = toolbar_point(&chrome, "Activity rail", 1024, 600);
    let intents = chrome.click(0, x, y, 1024, 600);
    let [ChromeIntent::Layout(layout)] = intents.as_slice() else {
        panic!("layout toggle must dispatch through the existing action spine");
    };
    assert!(layout.activity_rail);
    app.update(Action::SetChromeLayout(*layout));
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let (x, y) = toolbar_point(&chrome, "Read", 1024, 600);
    let area = workspace_area(app.shell_chrome_config(), true, 1024, 600);
    assert!(x < area.x && y > area.y);
    let dom = chrome.dom.borrow();
    let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
    assert_eq!(
        dom.all_with_class(root, "shell-control")
            .into_iter()
            .filter(|node| { dom.outer_html(*node).contains("aria-label=\"Read\"") })
            .count(),
        1
    );
}

#[test]
fn rail_tabs_project_each_tile_and_keep_native_content_outside_the_rail() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/first".into()));
    let first = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ReadGraphMember(first));
    let reader = app.active_pane.unwrap();
    let map = app.default_graph_pane();
    let second = app
        .with_graph_pane(map, |canvas| {
            canvas.open_member_as_new_node(Some(first), "https://example.test/second")
        })
        .unwrap();
    app.update(Action::ReadGraphMember(second));
    app.update(Action::WorkbenchSplitOut {
        dragged: second,
        axis: crate::action::WbAxis::Row,
        after: true,
    });
    let before = (
        app.workbench_for_pane(reader)
            .unwrap()
            .slot_views()
            .map(|slot| (slot.members.to_vec(), slot.weight))
            .collect::<Vec<_>>(),
        app.workbench_for_pane(reader).unwrap().weights(),
    );
    let mut config = app.shell_chrome_config().clone();
    config.layout.document_rail = true;
    config.layout.activity_rail = true;
    app.set_shell_chrome_config(config);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert_eq!(chrome.runner.state().reading_stacks.len(), 2);
    assert_eq!(
        chrome
            .runner
            .state()
            .reading_stacks
            .iter()
            .flatten()
            .filter(|(_, _, active)| *active)
            .count(),
        2
    );
    let title = chrome.runner.state().reading_stacks[0][0].1.clone();
    let (x, y) = toolbar_point(&chrome, &title, 1024, 600);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::ReadMember(first)]
    );
    app.update(Action::ReadGraphMember(first));
    assert_eq!(
        (
            app.workbench_for_pane(reader)
                .unwrap()
                .slot_views()
                .map(|slot| (slot.members.to_vec(), slot.weight))
                .collect::<Vec<_>>(),
            app.workbench_for_pane(reader).unwrap().weights()
        ),
        before
    );
    assert_eq!(app.browser_command_target().unwrap().0, first);
    let area = workspace_area(app.shell_chrome_config(), true, 1024, 600);
    assert_eq!(
        area.x,
        (cambium::ShellMetrics::ACTIVITY_RAIL_WIDTH + cambium::ShellMetrics::DOCUMENT_NAV_WIDTH)
            * app.shell_chrome_config().appearance.zoom()
    );
    assert_eq!(area.x + area.w, 1024.0);
}

#[test]
fn toolbar_is_one_native_row_with_controls_clear_of_caption_buttons_at_all_widths() {
    for width in [320, 420, 800, 1024, 1400] {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/reading".into()));
        let mut config = app.shell_chrome_config().clone();
        config.titlebar_insets.insert(
            0,
            cambium_rootstock::TitlebarInsets {
                left: 156.0,
                right: 0.0,
                height: 56.0,
            },
        );
        app.set_shell_chrome_config(config);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, width as f32, 600.0)]);
        let point = toolbar_point(&chrome, "Edit location:", width, 600);
        assert!(point.0 >= 156.0 && point.0 < width as f32);
        assert!(point.1 <= 56.0);
        assert_eq!(
            chrome.app_region_at(0, point.0, point.1, width, 600),
            cambium_rootstock::AppRegion::NoDrag
        );
        let (px, py) = toolbar_point(&chrome, "Panes", width, 600);
        assert!(px < width as f32 && py <= 56.0);
        assert_eq!(
            workspace_area(app.shell_chrome_config(), true, width, 600).y,
            56.0
        );
    }
}

#[test]
fn panes_territory_toggle_projects_the_current_maps_state() {
    let mut app = browser_history_app(true);
    app.update(Action::SetWorkspaceMap(false));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(!chrome.runner.state().territory);
    let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    let (x, y) = toolbar_point(&chrome, "Territory", 1024, 600);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::Territory]
    );
    app.update(Action::ToggleTerritory);
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(chrome.runner.state().territory);
}

#[test]
fn panes_menu_dispatches_real_tools_and_keyboard_uses_the_same_retained_buttons() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/a".into()));
    app.update(Action::SetWorkspaceMap(false));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::MenuChanged]
    );
    assert!(chrome.has_menu());
    let first = chrome.runner.state().tools[0].0;
    chrome.menu_key("down");
    chrome.menu_key("down");
    assert_eq!(
        chrome.menu_key("enter").unwrap(),
        vec![ChromeIntent::Tool(first)]
    );
    assert!(!chrome.has_menu());
    let (x, y) = toolbar_point(&chrome, "Panes", 1024, 600);
    chrome.click(0, x, y, 1024, 600);
    assert!(chrome.dismiss_menu());
    assert!(!chrome.has_menu());
}

#[test]
fn kept_card_yields_to_the_menu_and_returns_beside_a_new_peek_at_native_scale() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/source".into()));
    let pane = app.default_graph_pane();
    let source = app.graph_pane_focused_member(pane).unwrap();
    let other = app
        .with_graph_pane(pane, |canvas| {
            canvas.open_member_as_new_node(Some(source), "https://example.test/related")
        })
        .unwrap();
    app.update(Action::ReadGraphMember(source));
    let graph = app.graph_for_pane(pane).unwrap();
    let target = app
        .focus_card_target(graph, other, crate::focus_card::Origin::Node)
        .unwrap();
    app.update(Action::FocusCard(crate::focus_card::Command::Peek(
        target.clone(),
    )));
    app.update(Action::FocusCard(crate::focus_card::Command::Keep(target)));
    let mut config = app.shell_chrome_config().clone();
    config.layout_scales.insert(0, 2.0);
    app.set_shell_chrome_config(config);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 2200., 1520.)]);
    chrome.scene(0, 2200, 1520);
    let target = |chrome: &ChromeSurfaces, name: &str| {
        chrome
            .selector_point(&taproot::Selector::role("button").with_attr("aria-label", name))
            .expect("one painted control")
    };
    let (x, y) = target(&chrome, "Panes");
    chrome.click(0, x, y, 2200, 1520);
    chrome.scene(0, 2200, 1520);
    assert_eq!(
        app.focus_card_views().len(),
        1,
        "the kept binding remains retained"
    );
    assert!(
        !chrome.focus_cards_visible(),
        "hidden cards also yield keyboard ownership"
    );
    assert!(
        chrome
            .dom
            .borrow()
            .all_with_class(
                chrome.runner.window_root(chrome.projections[0]).unwrap(),
                "cambium-focus-card"
            )
            .is_empty(),
        "the menu owns the overlay column"
    );
    let (x, y) = target(&chrome, "Peek selection");
    let intents = chrome.click(0, x, y, 2200, 1520);
    let [ChromeIntent::FocusCard(crate::focus_card::Command::Peek(next))] = intents.as_slice()
    else {
        panic!("the card must not intercept the menu: {intents:?}")
    };
    assert_eq!(next.member, Some(source));
    app.update(Action::FocusCard(crate::focus_card::Command::Peek(
        next.clone(),
    )));
    chrome.sync(&app, &[(0, 2200., 1520.)]);
    chrome.scene(0, 2200, 1520);
    let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
    assert_eq!(
        chrome
            .dom
            .borrow()
            .all_with_class(root, "cambium-focus-card")
            .len(),
        2
    );
    assert_eq!(app.focused_read_target().unwrap().member, source);
}

#[test]
fn panes_peek_has_one_painted_target_and_closes_the_menu_before_showing_the_card() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/peek".into()));
    let member = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::ReadGraphMember(member));
    app.update(Action::SetWorkspaceMap(true));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024., 700.)]);
    chrome.scene(0, 1024, 700);
    let (x, y) = toolbar_point(&chrome, "Panes", 1024, 700);
    chrome.click(0, x, y, 1024, 700);
    chrome.scene(0, 1024, 700);
    let selector = taproot::Selector::role("button").with_attr("aria-label", "Peek selection");
    let (x, y) = chrome
        .selector_point(&selector)
        .expect("one painted menu target");
    let intents = chrome.click(0, x, y, 1024, 700);
    let [ChromeIntent::FocusCard(crate::focus_card::Command::Peek(target))] = intents.as_slice()
    else {
        panic!("expected one peek command: {intents:?}");
    };
    assert_eq!(target.member, Some(member));
    assert!(!chrome.has_menu());
    app.update(Action::FocusCard(crate::focus_card::Command::Peek(
        target.clone(),
    )));
    chrome.sync(&app, &[(0, 1024., 700.)]);
    chrome.scene(0, 1024, 700);
    let label = format!("Read: {}", app.focus_card_views()[0].info.title);
    let read = taproot::Selector::role("button").with_attr("aria-label", &label);
    assert!(
        chrome.selector_point(&read).is_some(),
        "the card's control is painted and reachable"
    );
    assert!(
        app.map_navigation_active(),
        "peek leaves the map projection in place"
    );
}

#[test]
fn relationship_menu_opens_from_panes_without_an_ambiguous_toolbar_target() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("https://example.test/root".into()));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 360.0, 600.0)]);
    let (x, y) = toolbar_point(&chrome, "Panes", 360, 600);
    chrome.click(0, x, y, 360, 600);
    let (x, y) = toolbar_point(&chrome, "Browse document connections", 360, 600);
    assert_eq!(
        chrome.click(0, x, y, 360, 600),
        vec![ChromeIntent::MenuChanged]
    );
    assert_eq!(chrome.runner.state().menu, Some(ChromeMenu::Connections));
}
