// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn browser_strip_targets_the_workbench_member_without_moving_graph_selection() {
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress("https://example.test/a".into()));
    let a = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.update(Action::OpenAddress("https://example.test/b".into()));
    let b = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    app.update(Action::WorkbenchActivate(a));
    // Settle the fixture fetches so the strip displays the page URL,
    // rather than the higher-priority in-flight fetch status.
    for member in [a, b] {
        let request = app.content.active_fetch(member).unwrap();
        assert!(app.content.settle_fetch(member, request));
    }
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let intents = browser_button_intents(&mut chrome);
    assert!(intents.contains(&ChromeIntent::KeepNode(a)));
    assert!(!intents.contains(&ChromeIntent::KeepNode(b)));
    let dom = chrome.dom.borrow();
    let status = dom.all_with_class(dom.document(), "whereami");
    assert_eq!(status.len(), 1);
    assert!(dom.outer_html(status[0]).contains("https://example.test/a"));
    assert_eq!(app.graph_runtimes.focused_member(), Some(b));
}

#[test]
fn keep_button_targets_the_member_then_projects_kept_state() {
    let mut app = browser_history_app(true);
    let member = app.graph_runtimes.focused_member().expect("focused node");
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));

    app.update(Action::KeepNode { member });
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert!(app.node_is_kept(member));
    assert!(!browser_button_intents(&mut chrome).contains(&ChromeIntent::KeepNode(member)));
    let dom = chrome.dom.borrow();
    assert_eq!(
        dom.all_with_class(dom.document(), "shell-control")
            .into_iter()
            .filter(|&node| {
                dom.attribute(node, &"".into(), &"aria-label".into())
                    .is_some_and(|value| value.to_string() == "Keep document")
                    && dom
                        .attribute(node, &"".into(), &"aria-disabled".into())
                        .is_some_and(|value| value.to_string() == "true")
            })
            .count(),
        1,
        "the one-way control remains visible as disabled Kept state"
    );
}

#[test]
fn hidden_shellbar_is_removed_from_the_chrome_and_a11y_projections() {
    let mut app = open_omnibar_app();
    let mut config = app.shell_chrome_config().clone();
    config.shellbar.visible = false;
    app.set_shell_chrome_config(config);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let dom = chrome.dom.borrow();
    assert!(dom.all_with_class(dom.document(), "whereami").is_empty());
    drop(dom);
    assert!(
        !crate::a11y::a11y_lines(&app)
            .iter()
            .any(|line| line.starts_with("label: ")),
        "the hidden shellbar cannot leave an inaccessible caption behind"
    );
}

#[test]
fn live_appearance_is_applied_to_the_retained_chrome_surface() {
    let mut app = open_omnibar_app();
    let mut config = app.shell_chrome_config().clone();
    config.appearance.theme_id = Some("theme:night".into());
    config.appearance.theme_mode = crate::shell_services::ThemeMode::Light;
    config.appearance.ui_zoom = 1.5;
    app.set_shell_chrome_config(config);

    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    assert_eq!(chrome.appearance, app.shell_chrome_config().appearance);
    assert!(
        crate::ui::chrome_sheet(&chrome.appearance).contains("24.00px"),
        "the retained chrome is laid out at the live zoom"
    );
    assert!(
        !chrome.scene(0, 1024, 600).ops.is_empty(),
        "the live appearance stylesheet drives the rendered chrome surface"
    );
}

/// The forest topology: primary + a lens chrome are sibling window-roots
/// of ONE document, and each window's scene lays out only ITS subtree —
/// the omnibar card exists under the primary root and not the lens's.
#[test]
fn chrome_windows_are_forest_siblings_with_scoped_layout() {
    let app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0), (1, 800.0, 500.0)]);
    let dom = chrome.dom.borrow();
    let doc = dom.document();
    let roots: Vec<_> = dom.dom_children(doc).collect();
    assert_eq!(roots.len(), 2, "two window-roots under one document");
    // The omnibar card renders under the PRIMARY root only.
    let cards = dom.all_with_class(doc, "omni");
    assert_eq!(cards.len(), 1, "one omnibar card in the whole forest");
    let chips = dom.all_with_class(doc, "whereami");
    assert_eq!(chips.len(), 2, "both windows carry the caption chip");
    drop(dom);
    // Scoped scenes: the primary's has content; the lens's renders too
    // (its chip), from its own root at its own size.
    let primary = chrome.scene(0, 1024, 600);
    let lens = chrome.scene(1, 800, 500);
    assert!(!primary.ops.is_empty(), "primary chrome paints");
    assert!(!lens.ops.is_empty(), "lens chrome paints its chip");
}

/// A suggestion-row click bubbles its ORIGINAL index — the new capability
/// the retained view adds over the hand chrome.
#[test]
fn clicking_a_suggestion_row_bubbles_its_index() {
    let app = open_omnibar_app();
    assert!(
        !app.omnibar.suggestions.is_empty(),
        "the '>r' query filters the registry to something"
    );
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    // Resolve the first selectable row's centre off the laid-out chrome
    // (the primary root's OWN subtree layout, the per-window pass the
    // shell runs).
    let (x, y) = {
        let dom = chrome.dom.borrow();
        let root = chrome
            .runner
            .window_root(chrome.projections[0])
            .expect("the primary window-root exists");
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        let row = dom
            .all_with_class(dom.document(), "omni-row-sel")
            .into_iter()
            .next()
            .expect("a selected row is drawn");
        let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 600)
            .expect("row has a rect");
        (rx + rw / 2.0, ry + rh / 2.0)
    };
    let intents = chrome.click(0, x, y, 1024, 600);
    assert_eq!(intents, vec![ChromeIntent::CommitRow(0)]);
}

/// A command row's label commits it and its own control keeps or drops
/// it, each with the row's original index.
#[test]
fn a_command_rows_keep_control_is_its_own_target() {
    let app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let centre_of = |chrome: &ChromeSurfaces, class: &str| {
        let dom = chrome.dom.borrow();
        let root = chrome
            .runner
            .window_root(chrome.projections[0])
            .expect("the primary window-root exists");
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        let node = dom
            .all_with_class(dom.document(), class)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("a {class} is drawn"));
        let (x, y, w, h) = crate::ui::subtree_node_rect(&dom, root, node, &sheet, 1024, 600)
            .expect("it has a rect");
        (x + w / 2.0, y + h / 2.0)
    };
    let (kx, ky) = centre_of(&chrome, "omni-keep");
    let (lx, ly) = centre_of(&chrome, "omni-row-label");
    assert!(kx > lx, "the control sits after the label");
    assert_eq!(
        chrome.click(0, kx, ky, 1024, 600),
        vec![ChromeIntent::ToggleKeep(0)]
    );
    assert_eq!(
        chrome.click(0, lx, ly, 1024, 600),
        vec![ChromeIntent::CommitRow(0)]
    );
}

#[test]
fn install_review_wraps_intact_and_keeps_card_and_click_geometry() {
    let (app, watch_url) = long_install_review_app();
    let review = app
        .omnibar
        .suggestions
        .iter()
        .find(|suggestion| is_install_review(suggestion))
        .expect("the pending install review is visible");
    let expected = row_text(review);
    assert!(
        crate::ui::chrome_row_width(&expected) > crate::ui::ROW_TEXT_BUDGET,
        "the fixture must actually overflow an ordinary one-line row"
    );
    assert!(
        expected.contains(&watch_url),
        "the displayed review is intact"
    );
    assert!(
        crate::a11y::a11y_lines(&app)
            .iter()
            .any(|line| line == &format!("button: {expected}")),
        "accessibility carries the same complete review"
    );

    let pending = app.pending_install.as_ref().expect("the install is staged");
    let review_text = crate::denizen::review_line(pending);
    let row_height = assert_install_review_layout(&app, &review_text);
    assert!(
        row_height > 44.0,
        "emergency breaking takes the URL beyond the two natural URL lines: {row_height}px"
    );
}

/// The caret split mirrors the omnibar state: before/caret/after and the
/// preedit ride the retained DOM (the receipt-proven IME honesty, kept).
#[test]
fn the_caret_split_mirrors_omnibar_state() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    for c in "abd".chars() {
        app.update(Action::OmnibarChar(c));
    }
    app.update(Action::OmnibarCaret(crate::action::CaretMove::Left));
    app.omnibar.preedit = Some("c".to_string());
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    let dom = chrome.dom.borrow();
    let input = dom
        .all_with_class(dom.document(), "omni-input")
        .into_iter()
        .next()
        .expect("the input line is drawn");
    let texts: Vec<String> = dom
        .dom_children(input)
        .flat_map(|c| {
            let mut out = Vec::new();
            if let Some(t) = dom.text(c) {
                out.push(t.to_string());
            }
            out.extend(
                dom.dom_children(c)
                    .filter_map(|g| dom.text(g).map(str::to_string)),
            );
            out
        })
        .collect();
    assert_eq!(
        texts,
        ["ab", "c", "|", "d"],
        "committed prefix, IME preedit, font-safe caret, committed suffix"
    );
}

/// The retention contract: a chrome that nothing has changed re-paints
/// without re-cascading. This is the unit-level guard for the command
/// palette open-lag receipt, which measured the unretained path at 36 ms
/// of a 44 ms frame.
#[test]
fn an_unchanged_chrome_paints_without_cascading_again() {
    let app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);

    assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
    assert_eq!(chrome.layout_rebuilds(0), 1, "the first frame cascades");

    // Idle frames: sync still runs (the shell calls it every frame), but
    // it writes no mutations, so nothing is invalidated.
    for _ in 0..5 {
        chrome.sync(&app, &[(0, 1024.0, 600.0)]);
        assert!(!chrome.scene(0, 1024, 600).ops.is_empty());
    }
    assert_eq!(
        chrome.layout_rebuilds(0),
        1,
        "five idle frames must not re-cascade"
    );

    // A resize is a per-root fact the layout detects for itself.
    assert!(!chrome.scene(0, 800, 600).ops.is_empty());
    assert_eq!(chrome.layout_rebuilds(0), 2, "a resize re-cascades");
}

#[test]
fn chrome_selectors_refuse_unpainted_menu_geometry_even_after_a_shared_drain() {
    let app = App::test_stub();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    let selector = taproot::Selector::role("button").with_attr("aria-label", "Panes");
    let (x, y) = chrome.selector_point(&selector).unwrap();
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::MenuChanged]
    );
    assert_eq!(chrome.runner.state().menu, Some(ChromeMenu::Panes));
    assert!(
        chrome.selector_point(&selector).is_none(),
        "a menu transition must be painted before resolving another click"
    );
    let mut consumed = Vec::new();
    chrome.dom.borrow_mut().drain_mutations(&mut consumed);
    assert!(!consumed.is_empty());
    assert!(
        chrome.selector_point(&selector).is_none(),
        "consuming the mutation batch does not make old fragments current"
    );
    chrome.scene(0, 1024, 600);
    let (x, y) = chrome.selector_point(&selector).unwrap();
    assert_eq!(chrome.layout_rebuilds(0), 2);
    assert_eq!(
        chrome.click(0, x, y, 1024, 600),
        vec![ChromeIntent::MenuChanged]
    );
    assert_eq!(chrome.runner.state().menu, None);
}

/// A state change reaches the layout. The retention must not be so eager
/// that a changed chrome keeps painting the old cascade.
#[test]
fn a_changed_chrome_cascades_again() {
    let mut app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    assert_eq!(chrome.layout_rebuilds(0), 1);

    app.update(Action::OmnibarChar('e'));
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    assert_eq!(
        chrome.layout_rebuilds(0),
        2,
        "an edited omnibar re-cascades"
    );
}

/// The forest hazard, stated as a test: N window-roots share ONE DOM and
/// therefore ONE mutation queue. A per-root drain would let whichever root
/// rendered first swallow the batch, leaving every other window painting a
/// tree that no longer exists — stale, not merely slow. The drain lives in
/// `absorb_dom_mutations` for exactly this reason, and this asserts that a
/// single change reaches BOTH roots.
#[test]
fn one_mutation_invalidates_every_window_root() {
    let mut app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    let sizes = [(0, 1024.0, 600.0), (1, 800.0, 500.0)];

    chrome.sync(&app, &sizes);
    chrome.scene(0, 1024, 600);
    chrome.scene(1, 800, 500);
    assert_eq!(chrome.layout_rebuilds(0), 1);
    assert_eq!(chrome.layout_rebuilds(1), 1);

    // The primary renders FIRST, which is precisely the ordering that
    // would hide a per-root drain bug.
    app.update(Action::OmnibarChar('e'));
    chrome.sync(&app, &sizes);
    chrome.scene(0, 1024, 600);
    chrome.scene(1, 800, 500);
    assert_eq!(chrome.layout_rebuilds(0), 2, "the primary re-cascaded");
    assert_eq!(
        chrome.layout_rebuilds(1),
        2,
        "the lens re-cascaded too, rather than losing the batch to the primary"
    );
}

/// A pointer press hit-tests the layout the frame painted, and pays no
/// cascade to do it. Before retention this path rebuilt the whole subtree
/// on every press, including presses that miss chrome entirely.
#[test]
fn a_press_hit_tests_without_cascading_again() {
    let app = open_omnibar_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    assert_eq!(chrome.layout_rebuilds(0), 1);

    // A press well away from any chrome control: it still hit-tests, and
    // it must not provoke a cascade to answer "nothing here".
    let intents = chrome.click(0, 5.0, 590.0, 1024, 600);
    assert!(
        intents.is_empty(),
        "a press on empty chrome commits nothing"
    );
    assert_eq!(
        chrome.layout_rebuilds(0),
        1,
        "hit testing shares the frame's layout instead of building its own"
    );
}
