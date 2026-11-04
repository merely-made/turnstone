// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use layout_dom_api::LayoutDom;

use super::*;
use crate::action::{Action, Update};

#[path = "tests/research_wheel_cases.rs"]
mod research_wheel_cases;

fn toolbar_point(chrome: &ChromeSurfaces, name: &str, w: u32, h: u32) -> (f32, f32) {
    let dom = chrome.dom.borrow();
    let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
    let node = dom
        .all_with_class(root, "shell-control")
        .into_iter()
        .chain(dom.all_with_class(root, "browser-status"))
        .find(|node| {
            dom.outer_html(*node).starts_with("<button")
                && dom.outer_html(*node).contains(&if name.ends_with(':') {
                    format!("aria-label=\"{name}")
                } else {
                    format!("aria-label=\"{name}\"")
                })
        })
        .expect("named toolbar control");
    let (x, y, w, h) = crate::ui::subtree_node_rect(
        &dom,
        root,
        node,
        &crate::ui::chrome_sheet(&chrome.appearance),
        w,
        h,
    )
    .unwrap();
    (x + w / 2.0, y + h / 2.0)
}

fn open_omnibar_app() -> App {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("mere://alpha".to_string()));
    app.update(Action::OmnibarOpen { command: true });
    app.update(Action::OmnibarChar('r'));
    app
}

fn long_install_review_app() -> (App, String) {
    let watch_url = format!(
        "https://watch.example/{}",
        "averyverylongunbrokenwatchtarget".repeat(6)
    );
    let mut app = App::test_stub();
    app.viewport = (1024.0, 320.0);
    app.pending_install = Some(crate::denizen::PendingInstall {
        path: std::path::PathBuf::from("watcher.lua"),
        label: "watcher".into(),
        body: crate::denizen::PackBody::Scenario("mere.snapshot()".into()),
        subject: servitor::Subject::new([7; 32]),
        rings: crate::denizen::default_rings(),
        watch_url: Some(watch_url.clone()),
        deadband: None,
    });
    app.update(Action::OmnibarOpen { command: true });
    (app, watch_url)
}

fn browser_history_app(settled: bool) -> App {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/a".into()));
    let node = app.graph_runtimes.focused_member().expect("focused node");
    let request = app.content.active_fetch(node).expect("initial fetch");
    assert!(app.content.settle_fetch(node, request));
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "gemini://capsule.test/b".into(),
    });
    app.update(Action::ContentNavigationCommitted {
        member: node,
        url: "gemini://capsule.test/c".into(),
    });
    app.update(Action::NavBack);
    if settled {
        let request = app.content.active_fetch(node).expect("Back fetch");
        assert!(app.content.settle_fetch(node, request));
    }
    app
}

fn browser_button_intents(chrome: &mut ChromeSurfaces) -> Vec<ChromeIntent> {
    let rects = {
        let dom = chrome.dom.borrow();
        let root = chrome
            .runner
            .window_root(chrome.projections[0])
            .expect("the primary window-root exists");
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        dom.all_with_class(dom.document(), "shell-control")
            .into_iter()
            .filter(|button| {
                let html = dom.outer_html(*button);
                [
                    "Back",
                    "Forward",
                    "Reload",
                    "Stop",
                    "Keep document",
                    "Allow once",
                    "Always allow",
                    "Deny",
                ]
                .iter()
                .any(|name| html.contains(&format!("aria-label=\"{name}")))
            })
            .map(|button| {
                crate::ui::subtree_node_rect(&dom, root, button, &sheet, 1024, 600)
                    .expect("browser button has a rect")
            })
            .collect::<Vec<_>>()
    };
    rects
        .into_iter()
        .flat_map(|(x, y, w, h)| chrome.click(0, x + w / 2.0, y + h / 2.0, 1024, 600))
        .collect()
}

fn findable_app() -> App {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/find".into()));
    let node = app.graph_runtimes.focused_member().expect("focused node");
    app.content.note_live(
        node,
        Some(crate::content::ContentFacts {
            engine: "test.surface".into(),
            lane: crate::content::ContentLane::Surface,
            structure: None,
            lineage: None,
            capabilities: crate::content::DocumentCapabilityFacts {
                find_in_page: crate::content::CapabilityStatus::Supported,
                ..Default::default()
            },
        }),
    );
    app.update(Action::OpenDocumentFind);
    app
}

/// One key through the chrome's find field, lowered and applied the way
/// the shell does.
fn find_press(
    chrome: &mut ChromeSurfaces,
    app: &mut App,
    key: cambium::Key,
    shift: bool,
) -> Vec<Action> {
    chrome.sync(app, &[(0, 1024.0, 600.0)]);
    let mut event = cambium::KeyEvent::new(key);
    event.mods.shift = shift;
    let actions: Vec<Action> = chrome
        .find_key(event)
        .into_iter()
        .filter_map(crate::document_find::find_action)
        .collect();
    for action in &actions {
        app.update(action.clone());
    }
    actions
}

/// The actual retained Confirm row, shared with the staged-install test.
pub(crate) fn assert_install_review_layout(app: &App, expected_review: &str) -> f32 {
    let review = app
        .omnibar
        .suggestions
        .iter()
        .find(|suggestion| is_install_review(suggestion))
        .expect("the pending install review is visible");
    let expected = row_text(review);
    assert_eq!(
        expected,
        format!("\u{203a} {expected_review}"),
        "the displayed ask is complete"
    );
    assert!(
        crate::a11y::a11y_lines(app)
            .iter()
            .any(|line| line == &format!("button: {expected}")),
        "accessibility carries the same complete review"
    );

    let mut chrome = ChromeSurfaces::new();
    chrome.sync(app, &[(0, 1024.0, 320.0)]);
    let (x, y, row_height) = {
        let dom = chrome.dom.borrow();
        let root = chrome
            .runner
            .window_root(chrome.projections[0])
            .expect("the primary window-root exists");
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        let row = dom
            .all_with_class(dom.document(), "omni-row-review")
            .into_iter()
            .next()
            .expect("the review has its wrapping class");
        let mut nodes = vec![row];
        let mut displayed = String::new();
        while let Some(node) = nodes.pop() {
            if let Some(text) = dom.text(node) {
                displayed.push_str(text);
            }
            nodes.extend(dom.dom_children(node).collect::<Vec<_>>().into_iter().rev());
        }
        assert_eq!(
            displayed, expected,
            "the retained DOM preserves the whole ask"
        );
        let (rx, ry, rw, rh) = crate::ui::subtree_node_rect(&dom, root, row, &sheet, 1024, 320)
            .expect("review has a rect");
        assert!(
            rw <= CARD_W,
            "the wrapped review stays within the card: {rw}px"
        );
        assert!(
            rh.is_finite() && rh > 0.0,
            "the review has visible height: {rh}px"
        );

        let card = dom
            .all_with_class(dom.document(), "omni")
            .into_iter()
            .next()
            .expect("the omnibar card exists");
        let (cx, cy, cw, ch) = crate::ui::subtree_node_rect(&dom, root, card, &sheet, 1024, 320)
            .expect("card has a rect");
        assert!(
            rx >= cx && ry >= cy && rx + rw <= cx + cw && ry + rh <= cy + ch,
            "the whole review remains inside the card: row=({rx}, {ry}, {rw}, {rh}) card=({cx}, {cy}, {cw}, {ch})"
        );
        assert!(
            cy + ch <= 320.0 - 16.0,
            "the row budget leaves the expanded card inside the viewport: top={cy}px card={ch}px review={rh}px rows={} extra={}px",
            app.omnibar.suggestions.len(),
            crate::ui::chrome_review_row_extra_height(&expected, &chrome.appearance)
        );
        (rx + rw / 2.0, ry + rh / 2.0, rh)
    };
    assert_eq!(
        chrome.click(0, x, y, 1024, 320),
        vec![ChromeIntent::CommitRow(0)],
        "the expanded row remains one clickable review"
    );
    row_height
}

#[path = "tests/browser_strip_targets_the_workbench_member_without_moving_cases.rs"]
mod browser_strip_targets_the_workbench_member_without_moving_cases;
#[path = "tests/field_preview_is_attached_to_its_measured_graph_cases.rs"]
mod field_preview_is_attached_to_its_measured_graph_cases;
#[path = "tests/shared_title_bar_respects_native_controls_and_drag_cases.rs"]
mod shared_title_bar_respects_native_controls_and_drag_cases;

#[test]
fn research_comparison_rows_wrap_fit_and_keep_their_click_indices() {
    use crate::app::research_search_state::View;
    let mut app = App::test_stub();
    app.viewport = (1024.0, 320.0);
    app.update(Action::SearchSavedSources);
    app.research_search.view = View::Compare;
    app.research_search.error = Some(format!(
        "Source unavailable: {}",
        "long-source-identity-".repeat(12)
    ));
    // Movement rebuilds the same research view through the ordinary input path.
    app.update(Action::OmnibarMove(0));
    let expected = app
        .omnibar
        .suggestions
        .iter()
        .map(row_text)
        .collect::<Vec<_>>();
    assert!(expected[0].contains("long-source-identity"));
    assert!(crate::ui::chrome_row_width(&expected[0]) > crate::ui::ROW_TEXT_BUDGET);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 320.0)]);
    let points = {
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let sheet = crate::ui::chrome_sheet(&chrome.appearance);
        let rows = dom.all_with_class(root, "omni-row-research");
        assert_eq!(rows.len(), expected.len());
        let card = dom.all_with_class(root, "omni")[0];
        let (_, cy, _, ch) =
            crate::ui::subtree_node_rect(&dom, root, card, &sheet, 1024, 320).unwrap();
        assert!(cy + ch <= 304.0, "card extends below viewport: {}", cy + ch);
        rows.iter()
            .enumerate()
            .map(|(index, row)| {
                let html = dom.outer_html(*row);
                assert!(html.contains(&expected[index]), "row text changed: {html}");
                let (x, y, w, h) =
                    crate::ui::subtree_node_rect(&dom, root, *row, &sheet, 1024, 320).unwrap();
                if index == 0 {
                    assert!(h > 30.0, "long source label did not wrap: {h}");
                }
                assert!(y + h <= 304.0);
                (x + w / 2.0, y + h / 2.0)
            })
            .collect::<Vec<_>>()
    };
    for (index, (x, y)) in points.into_iter().enumerate() {
        assert_eq!(
            chrome.click(0, x, y, 1024, 320),
            vec![ChromeIntent::CommitRow(index)]
        );
    }
    let previous_first = row_text(&app.omnibar.suggestions[0]);
    app.update(Action::OmnibarMove(1));
    assert_ne!(
        row_text(&app.omnibar.suggestions[0]),
        previous_first,
        "trimmed rows remain reachable"
    );
}

#[test]
fn very_long_research_row_scrolls_inside_card_and_keeps_its_action() {
    use crate::app::research_search_state::View;
    let mut app = App::test_stub();
    app.viewport = (1024.0, 320.0);
    app.update(Action::SearchSavedSources);
    app.research_search.view = View::Compare;
    app.research_search.error = Some(format!(
        "Exact source: {}",
        "very-long-source-identity-".repeat(120)
    ));
    app.update(Action::OmnibarMove(0));
    assert_eq!(app.omnibar.suggestions.len(), 1);
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 320.0)]);
    chrome.scene(0, 1024, 320);
    let (x, y, bottom) = {
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let card = dom.all_with_class(root, "omni-research-scroll")[0];
        let (x, y, w, h) = crate::ui::subtree_node_rect(
            &dom,
            root,
            card,
            &crate::ui::chrome_sheet(&chrome.appearance),
            1024,
            320,
        )
        .unwrap();
        assert!(y + h <= 304.0, "card must fit its viewport");
        (x + w / 2.0, y + h / 2.0, y + h)
    };
    assert_eq!(
        chrome.scroll_research(0, 0.0, 0.0, -100.0, 1024, 320),
        crate::ui::ElementScroll::Outside
    );
    assert_ne!(
        chrome.scroll_research(0, x, y, -100_000.0, 1024, 320),
        crate::ui::ElementScroll::Outside
    );
    chrome.scene(0, 1024, 320);
    assert_eq!(
        chrome.click(0, x, bottom - 10.0, 1024, 320),
        vec![ChromeIntent::CommitRow(0)]
    );
    assert_ne!(
        chrome.scroll_research(0, x, y, -100_000.0, 1024, 320),
        crate::ui::ElementScroll::Outside
    );
    assert_eq!(
        chrome.click(0, x, bottom - 10.0, 1024, 320),
        vec![ChromeIntent::CommitRow(0)],
        "scroll is clamped at the end"
    );
}

#[test]
fn narrow_research_card_fits_window_and_keeps_rows_clickable() {
    use crate::app::research_search_state::View;
    let mut app = App::test_stub();
    app.viewport = (400.0, 320.0);
    app.update(Action::SearchSavedSources);
    app.research_search.view = View::Compare;
    app.research_search.error = Some("long source identity ".repeat(100));
    app.update(Action::OmnibarMove(0));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 400.0, 320.0)]);
    chrome.scene(0, 400, 320);
    let (x, y, bottom) = {
        let dom = chrome.dom.borrow();
        let root = chrome.runner.window_root(chrome.projections[0]).unwrap();
        let card = dom.all_with_class(root, "omni-research-scroll")[0];
        let (x, y, w, h) = crate::ui::subtree_node_rect(
            &dom,
            root,
            card,
            &crate::ui::chrome_sheet(&chrome.appearance),
            400,
            320,
        )
        .unwrap();
        assert!(
            x >= 0.0 && x + w <= 400.0 && y + h <= 304.0,
            "card exceeds narrow window: {x}, {y}, {w}, {h}"
        );
        (x + w / 2.0, y + h / 2.0, y + h)
    };
    assert_ne!(
        chrome.scroll_research(0, x, y, -100_000.0, 400, 320),
        crate::ui::ElementScroll::Outside
    );
    chrome.scene(0, 400, 320);
    assert_eq!(
        chrome.click(0, x, bottom - 10.0, 400, 320),
        vec![ChromeIntent::CommitRow(0)]
    );
}
