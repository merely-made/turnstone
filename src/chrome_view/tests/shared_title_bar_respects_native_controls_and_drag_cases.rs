// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn shared_title_bar_respects_native_controls_and_drag_regions() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".into()));
    let mut config = app.shell_chrome_config().clone();
    config.appearance.ui_zoom = 1.0;
    config.titlebar_insets.insert(
        0,
        cambium_rootstock::TitlebarInsets {
            left: 156.0,
            right: 0.0,
            height: 56.0,
        },
    );
    config.titlebar_insets.insert(
        2,
        cambium_rootstock::TitlebarInsets {
            left: 78.0,
            right: 0.0,
            height: 28.0,
        },
    );
    app.set_shell_chrome_config(config);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0), (2, 640.0, 480.0)]);
    for (slot, width, height, leading, expected_height) in
        [(0, 1024, 600, 156.0, 56.0), (2, 640, 480, 78.0, 40.0)]
    {
        let (ornament, title) = {
            let dom = chrome.dom.borrow();
            let root = chrome.runner.window_root(chrome.projections[slot]).unwrap();
            let sheet = crate::ui::chrome_sheet(&chrome.appearance);
            let node = |class| dom.all_with_class(root, class)[0];
            if slot != 0 {
                assert!(
                    dom.outer_html(node("turnstone-title-name"))
                        .contains("Turnstone")
                );
            }
            let rect = |class| {
                crate::ui::subtree_node_rect(&dom, root, node(class), &sheet, width, height)
                    .unwrap()
            };
            (
                rect("turnstone-stone"),
                rect(if slot == 0 {
                    "turnstone-stone"
                } else {
                    "turnstone-title-name"
                }),
            )
        };
        assert!(ornament.0 >= leading, "ornament must clear native captions");
        assert!(title.0 >= leading);
        let point = (title.0 + title.2 / 2.0, title.1 + title.3 / 2.0);
        assert_eq!(
            chrome.app_region_at(slot, point.0, point.1, width, height),
            cambium_rootstock::AppRegion::Drag
        );
        assert!(
            chrome
                .click(slot, point.0, point.1, width, height)
                .is_empty(),
            "identity is inert"
        );
        let area = workspace_area_for_window(app.shell_chrome_config(), slot, width, height);
        assert_eq!(area.y, expected_height);
    }
    // The adjacent navigation row uses ordinary controls, outside the
    // shared title bar's inherited drag region.
    assert_eq!(
        chrome.app_region_at(0, 20.0, 80.0, 1024, 600),
        cambium_rootstock::AppRegion::NoDrag
    );
    // Authored descendants can explicitly opt out of the inherited drag
    // region without changing the window bridge or its hit geometry.
    let mut config = app.shell_chrome_config().clone();
    config.appearance.theme_presentation =
        Some(tabard::ThemePresentation::AuthoredStylesheet(vec![
            ".turnstone-stone { --app-region: no-drag; }".into(),
        ]));
    app.set_shell_chrome_config(config);
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let point = {
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let title = dom.all_with_class(root, "turnstone-stone")[0];
        let rect = crate::ui::subtree_node_rect(
            &dom,
            root,
            title,
            &crate::ui::chrome_sheet(&chrome.appearance),
            1024,
            600,
        )
        .unwrap();
        (rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0)
    };
    assert_eq!(
        chrome.app_region_at(0, point.0, point.1, 1024, 600),
        cambium_rootstock::AppRegion::NoDrag
    );
}

#[test]
fn navigation_and_context_reserve_rows_in_the_actual_frame() {
    for (width, height, zoom, edge) in [
        (1024, 600, 1.0, ChromeEdge::Left),
        (420, 400, 1.0, ChromeEdge::Right),
        (800, 600, 2.0, ChromeEdge::Bottom),
        (1024, 600, 1.5, ChromeEdge::Top),
    ] {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("mere://alpha".into()));
        let mut config = app.shell_chrome_config().clone();
        config.appearance.ui_zoom = zoom;
        config.shellbar.placement = ChromePlacement::Docked(edge);
        app.set_shell_chrome_config(config);
        let config = app.shell_chrome_config();
        let area = workspace_area(config, true, width, height);
        let mut chrome = ChromeSurfaces::new();
        chrome.sync(&app, &[(0, width as f32, height as f32)]);
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        let rect = |class| {
            let node = dom.all_with_class(dom.document(), class)[0];
            crate::ui::subtree_node_rect(&dom, root, node, &sheet, width, height).unwrap()
        };
        let (nx, ny, nw, nh) = rect("browser-strip");
        let (_, cy, _, ch) = rect("whereami");
        assert!(nx >= 0.0 && nx + nw <= width as f32 + 0.1);
        assert!(
            ny + nh <= area.y + 0.1,
            "navigation must end before the frame"
        );
        if edge == ChromeEdge::Top {
            assert!(cy >= ny + nh - 0.1);
            assert!(cy + ch <= area.y + 0.1);
        } else {
            assert!(cy >= area.y + area.h - 0.1);
            assert!((cy + ch - height as f32).abs() < 0.1);
        }
        let mut frame = crate::outer_frame::OuterFrame::default();
        frame.set_workspace_area(area);
        let bodies = frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (width, height),
                &config.appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap();
        assert!(!bodies.is_empty());
        for (_, body) in bodies {
            assert!(body.y >= area.y);
            assert!(body.y + body.h <= area.y + area.h + 0.1);
        }
        assert_eq!(
            frame.header_height(),
            0.0,
            "the main Read/Map switch replaces the singleton frame header"
        );
        assert_eq!(
            frame
                .selector_point(&taproot::Selector::class("frisket-tab"))
                .unwrap(),
            None
        );
        let (_, body) = frame
            .prepare(
                &app.frisket,
                app.active_pane,
                None,
                (width, height),
                &config.appearance,
                app.session_id.as_uuid().as_u128(),
            )
            .unwrap()[0];
        assert_eq!(
            body, area,
            "the body uses exactly the space reserved below navigation"
        );
    }
}

#[test]
fn hidden_browser_chrome_keeps_only_the_title_row() {
    let mut config = crate::shell_services::ShellChromeConfig::default();
    config.shellbar.visible = false;
    assert_eq!(
        workspace_area(&config, true, 1024, 600),
        crate::surface::Rect::new(
            0.0,
            titlebar_height(config.appearance.zoom(), Default::default()),
            1024.0,
            600.0 - titlebar_height(config.appearance.zoom(), Default::default())
        )
    );
    config.shellbar.visible = true;
    let lens = workspace_area(&config, false, 800, 500);
    assert_eq!(
        lens.y,
        titlebar_height(config.appearance.zoom(), Default::default()),
        "lenses reserve only the title row above content"
    );
    assert!(lens.h < 500.0, "lenses keep context outside their content");
    let tiny = workspace_area(&config, true, 100, 20);
    assert!(tiny.h >= 0.0 && tiny.y <= 20.0);
}

#[test]
fn admitted_document_title_reaches_tabs_and_the_active_context() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress(format!(
        "mere://clip/{}",
        "ab".repeat(32)
    )));
    let member = app.graph_runtimes.focused_member().unwrap();
    let effects = app.apply_update(Update::ContentSpawned {
        node: member,
        facts: Some(crate::content::ContentFacts {
            engine: "retained-clip".into(),
            lane: crate::content::ContentLane::Document,
            structure: Some(crate::content::StructureFacts {
                title: Some("Knot selection".into()),
                headings: 1,
                links: 0,
                outline: vec![],
            }),
            lineage: None,
            capabilities: Default::default(),
        }),
    });
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, crate::action::Effect::SaveSession))
    );
    app.update(Action::OpenInWorkbench);
    let pane = app.active_pane.unwrap();
    let mut workbench = crate::workbench_pane::WorkbenchPane::new();
    workbench.sync(&app, pane, 800.0, 500.0);
    let dom = workbench.dom_ref();
    assert!(dom.outer_html(dom.document()).contains("Knot selection"));
    drop(dom);
    app.update(Action::OpenAddress("mere://other".into()));
    app.update(Action::WorkbenchActivate(member));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let dom = chrome.dom.borrow();
    let context = dom.all_with_class(dom.document(), "cambium-scope-location")[0];
    assert!(dom.outer_html(context).contains("Knot selection"));
}

#[test]
fn browser_strip_routes_per_node_back_forward_and_reload() {
    let app = browser_history_app(true);
    assert!(app.focused_can_back() && app.focused_can_forward());
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert_eq!(
        browser_button_intents(&mut chrome),
        vec![
            ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::Navigate(
                cambium::scope_navigation::ScopeDirection::Back
            )),
            ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::Navigate(
                cambium::scope_navigation::ScopeDirection::Forward
            )),
            ChromeIntent::Reload,
            ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
        ]
    );
}

#[test]
fn active_fetch_switches_reload_to_stop_and_projects_link_preview() {
    let mut app = browser_history_app(false);
    app.set_link_preview(Some("gemini://capsule.test/prospective".into()));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    {
        let dom = chrome.dom.borrow();
        assert_eq!(
            dom.all_with_class(dom.document(), "browser-loading").len(),
            1
        );
        assert_eq!(dom.all_with_class(dom.document(), "link-preview").len(), 1);
    }
    assert_eq!(
        browser_button_intents(&mut chrome),
        vec![
            ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::Navigate(
                cambium::scope_navigation::ScopeDirection::Back
            )),
            ChromeIntent::Scope(crate::scope_navigation::ScopeCommand::Navigate(
                cambium::scope_navigation::ScopeDirection::Forward
            )),
            ChromeIntent::Stop,
            ChromeIntent::KeepNode(app.graph_runtimes.focused_member().expect("focused node")),
        ]
    );
}

#[test]
fn document_find_projects_controlled_input_status_and_controls() {
    let mut app = findable_app();
    app.document_find.input = TextInput::new("needle");
    app.document_find.model = crate::action::DocumentFindModel {
        count: 1,
        matches: vec![crate::action::DocumentFindMatch {
            label: "Needle heading".into(),
            role: Some("heading".into()),
        }],
        current: Some(0),
        complete: true,
    };

    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let dom = chrome.dom.borrow();
    assert_eq!(dom.all_with_class(dom.document(), "document-find").len(), 1);
    assert_eq!(dom.all_with_class(dom.document(), "read-query").len(), 1);
    assert_eq!(dom.all_with_class(dom.document(), "read-result").len(), 1);
    let find = dom.all_with_class(dom.document(), "document-find")[0];
    let enabled = dom.all_with_class(find, "shell-control").len();
    let disabled = dom.all_with_class(find, "browser-button-disabled").len();
    assert_eq!(enabled + disabled, 3, "find owns previous, next, and close");
}

#[test]
fn find_field_edits_at_the_caret_through_the_shared_key_map() {
    use cambium::{Key, NamedKey};
    let mut app = findable_app();
    let mut chrome = ChromeSurfaces::new();
    for text in ["a", "b", "d"] {
        find_press(&mut chrome, &mut app, Key::Character(text.into()), false);
    }
    find_press(
        &mut chrome,
        &mut app,
        Key::Named(NamedKey::ArrowLeft),
        false,
    );
    find_press(&mut chrome, &mut app, Key::Character("c".into()), false);
    assert_eq!(app.document_find.query(), "abcd", "mid-string insert");
    assert_eq!(app.document_find.input.caret(), 3);
    find_press(&mut chrome, &mut app, Key::Named(NamedKey::Home), false);
    assert_eq!(app.document_find.input.caret(), 0);
    find_press(
        &mut chrome,
        &mut app,
        Key::Named(NamedKey::ArrowRight),
        true,
    );
    find_press(
        &mut chrome,
        &mut app,
        Key::Named(NamedKey::ArrowRight),
        true,
    );
    assert_eq!(app.document_find.input.selected_text(), "ab");
    find_press(
        &mut chrome,
        &mut app,
        Key::Named(NamedKey::Backspace),
        false,
    );
    assert_eq!(
        app.document_find.query(),
        "cd",
        "selection deletes as one edit"
    );
    find_press(&mut chrome, &mut app, Key::Named(NamedKey::End), false);
    find_press(
        &mut chrome,
        &mut app,
        Key::Named(NamedKey::Backspace),
        false,
    );
    assert_eq!(app.document_find.query(), "c");
    // The projected field carries the app's caret, not the end of the text.
    find_press(&mut chrome, &mut app, Key::Named(NamedKey::Home), false);
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert_eq!(chrome.runner.state().find.input.caret(), 0);
}

#[test]
fn find_field_keys_step_and_close_without_a_shell_key_map() {
    use crate::action::DocumentFindDirection::{Next, Previous};
    use cambium::{Key, NamedKey};
    let mut app = findable_app();
    let mut chrome = ChromeSurfaces::new();
    find_press(&mut chrome, &mut app, Key::Character("x".into()), false);
    assert_eq!(
        find_press(&mut chrome, &mut app, Key::Named(NamedKey::Enter), false),
        [Action::StepDocumentFind(Next)]
    );
    assert_eq!(
        find_press(&mut chrome, &mut app, Key::Named(NamedKey::Enter), true),
        [Action::StepDocumentFind(Previous)]
    );
    assert_eq!(
        find_press(&mut chrome, &mut app, Key::Named(NamedKey::Escape), false),
        [Action::CloseDocumentFind]
    );
    assert!(!app.document_find.open);
}

#[test]
fn find_field_keeps_keys_after_a_neighbouring_control_takes_focus() {
    use cambium::Key;
    let mut app = findable_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let id = chrome.projections[0];
    let button = {
        let dom = chrome.dom.borrow();
        let find = dom.all_with_class(dom.document(), "document-find")[0];
        dom.all_with_class(find, "shell-control")[0]
    };
    chrome.runner.set_focus(id, Some(button));
    find_press(&mut chrome, &mut app, Key::Character("z".into()), false);
    assert_eq!(app.document_find.query(), "z");
}

#[test]
fn permission_decision_projects_origin_choices_and_accessibility() {
    let mut app = App::test_stub();
    let node = uuid::Uuid::new_v4();
    let id = inker::UserAgentRequestId::new(41);
    app.apply_update(Update::PermissionRequested {
        node,
        id,
        origin: "https://maps.example/path".into(),
        descriptors: vec![inker::PermissionDescriptor::Geolocation],
    });

    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let intents = browser_button_intents(&mut chrome);
    assert!(intents.contains(&ChromeIntent::Permission {
        request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
        choice: crate::user_agent_decision::PermissionChoice::AllowOnce,
    }));
    assert!(intents.contains(&ChromeIntent::Permission {
        request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
        choice: crate::user_agent_decision::PermissionChoice::AlwaysAllow,
    }));
    assert!(intents.contains(&ChromeIntent::Permission {
        request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
        choice: crate::user_agent_decision::PermissionChoice::Deny,
    }));
    let dom = chrome.dom.borrow();
    assert_eq!(dom.all_with_class(dom.document(), "decision-card").len(), 1);
    assert!(dom.outer_html(dom.document()).contains("maps.example"));
    drop(dom);
    let a11y = crate::a11y::a11y_lines(&app);
    assert!(a11y.iter().any(|line| line == "button: Always allow"));
    assert!(
        a11y.iter()
            .any(|line| { line == "dialog: https://maps.example wants location" })
    );
}

#[test]
fn authentication_card_masks_password_and_a11y_withholds_credentials() {
    let mut app = App::test_stub();
    let node = uuid::Uuid::new_v4();
    app.apply_update(Update::AuthenticationRequested {
        node,
        id: inker::UserAgentRequestId::new(42),
        host: "secure.example".into(),
        port: 443,
        realm: Some("private".into()),
        scheme: "basic".into(),
        is_proxy: false,
    });
    app.update(Action::InsertAuthentication("private-user".into()));
    app.update(Action::FocusAuthenticationField(
        crate::user_agent_decision::AuthenticationField::Password,
    ));
    app.update(Action::InsertAuthentication("private-password".into()));

    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let dom = chrome.dom.borrow();
    let html = dom.outer_html(dom.document());
    assert!(html.contains("private-user"));
    assert!(!html.contains("private-password"));
    assert!(html.contains("\u{2022}"));
    drop(dom);
    let a11y = crate::a11y::a11y_lines(&app).join("\n");
    assert!(a11y.contains("textinput: Username"));
    assert!(a11y.contains("passwordinput: Password"));
    assert!(!a11y.contains("private-user"));
    assert!(!a11y.contains("private-password"));
}

#[test]
fn authentication_card_debug_withholds_credential_fields() {
    let card = DecisionCard::Authentication {
        request: crate::user_agent_decision::UserAgentRequestKey::new(
            uuid::Uuid::new_v4(),
            inker::UserAgentRequestId::new(43),
        ),
        prompt: "Sign in".into(),
        queued: 1,
        username: TextInput::new("private-user"),
        password: TextInput::new("private-password"),
        field: crate::user_agent_decision::AuthenticationField::Username,
        remember_for_process: false,
        submitting: false,
        error: None,
    };
    let debug = format!("{card:?}");
    assert!(!debug.contains("private-user"));
    assert!(!debug.contains("private-password"));
    assert!(debug.contains("[redacted]"));
}
