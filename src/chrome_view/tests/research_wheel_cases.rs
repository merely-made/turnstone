// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use eidetic::models::recall::RetainedSourceText;
use eidetic_search::compare::{ComparisonRelationship, ComparisonSettings, compare};

fn comparison_app() -> App {
    let mut app = App::test_stub();
    app.viewport = (1024.0, 600.0);
    app.research.set_available(true);
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.access(Some(crate::research::ClipAccess {
        root: std::env::temp_dir(),
        sealer: custody.sealer,
        epoch: std::sync::Arc::new(|| Some(1)),
    }));
    app.sync_research_context();
    app.update(Action::SearchSavedSources);
    let target = |i: u8| {
        crate::research::Target::Capture(crate::saved_sources::Target {
            key: crate::saved_sources::Key {
                session: app.session_id,
                graph: crate::panes::GraphId::nil(),
                node: uuid::Uuid::from_u128(u128::from(i)),
                annotation_manifest: format!("sealed-v1:blake3:{}", format!("{i:02x}").repeat(32)),
            },
            text_hash: format!("blake3:{}", format!("{:02x}", i + 2).repeat(32)),
            selector: None,
        })
    };
    let left = target(1);
    let right = target(2);
    let source = |target: &crate::research::Target, text: &str| RetainedSourceText {
        source: crate::research::source_id(target).unwrap(),
        version: target.text_hash().into(),
        title: None,
        text: text.into(),
    };
    let left_text = source(&left, "old note");
    let right_text = source(&right, "new note");
    let settings = ComparisonSettings::default();
    let (context, revision) = app.research.context_revision().unwrap();
    let request = crate::research_compare::Request {
        id: uuid::Uuid::new_v4(),
        context,
        revision,
        epoch: 1,
        left: left.clone(),
        right: right.clone(),
        settings: settings.clone(),
    };
    let comparison = compare(
        &left_text,
        &right_text,
        ComparisonRelationship::Explicit,
        &settings,
    )
    .unwrap();
    app.research_search.view = crate::app::research_search_state::View::Compare;
    app.research_search.compare_left = Some(left);
    app.research_search.compare_right = Some(right);
    app.research_search.compare_accepted = Some(request);
    app.research_search.comparison = Some(crate::research_compare::ResultView {
        left: left_text,
        right: right_text,
        comparison,
    });
    app.research_rows(100);
    app.update(Action::OmnibarMove(0));
    app
}

fn wheel(
    chrome: &mut ChromeSurfaces,
    app: &mut App,
    slot: usize,
    x: f32,
    y: f32,
    dy: f32,
) -> ResearchWheel {
    let route = chrome.route_research_wheel(app, slot, x, y, dy, 1024, 600);
    if let ResearchWheel::Advance(delta) = route {
        app.update(Action::OmnibarMove(delta));
    }
    chrome.sync(app, &[(0, 1024.0, 600.0), (1, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    route
}

#[test]
fn wheel_advances_bounded_comparison_rows_after_card_boundary() {
    let mut app = comparison_app();
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0), (1, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    assert!(
        app.research_search.rows.len() > app.omnibar.suggestions.len(),
        "comparison must have rows outside its bounded projection"
    );
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 0.0, 0.0, -100.0),
        ResearchWheel::Outside
    );
    assert_eq!(
        wheel(&mut chrome, &mut app, 1, 500.0, 160.0, -100.0),
        ResearchWheel::Outside
    );
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 500.0, 160.0, 0.0),
        ResearchWheel::Consumed
    );
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 500.0, 160.0, 100.0),
        ResearchWheel::Consumed
    );
    app.omnibar.open = false;
    assert_eq!(
        chrome.route_research_wheel(&app, 0, 500.0, 160.0, -100_000.0, 1024, 600),
        ResearchWheel::Consumed
    );
    app.omnibar.open = true;
    let first = app.research_search.first_row;
    let mut seen = false;
    for tick in 0..5 {
        let route = wheel(&mut chrome, &mut app, 0, 500.0, 160.0, -100_000.0);
        eprintln!(
            "tick={tick} route={route:?} first={} visible={} total={}",
            app.research_search.first_row,
            app.omnibar.suggestions.len(),
            app.research_search.rows.len()
        );
        let selector = taproot::Selector::class("omni-row-research").containing("Open whole left");
        if let Some((x, y)) = chrome.selector_point(&selector) {
            let intent = chrome.click(0, x, y, 1024, 600);
            let [ChromeIntent::CommitRow(index)] = intent.as_slice() else {
                panic!("open-left click: {intent:?}");
            };
            assert!(matches!(
                &app.omnibar.suggestions[*index],
                crate::ui::Suggestion::Act {
                    action: Action::ResearchSearch(
                        crate::app::research_search_state::Command::OpenCompare {
                            left: true,
                            span: None,
                            ..
                        }
                    ),
                    ..
                }
            ));
            seen = true;
            break;
        }
    }
    assert!(
        app.research_search.first_row > first,
        "wheel must advance the comparison window"
    );
    assert!(
        seen,
        "open-left must become visible and clickable within five wheel ticks"
    );
}

#[test]
fn oversized_row_scroll_survives_sync_then_advances_without_stale_offset() {
    let mut app = comparison_app();
    app.research_search.error = Some("long identity ".repeat(300));
    app.update(Action::OmnibarMove(0));
    let mut chrome = ChromeSurfaces::new();
    chrome.sync(&app, &[(0, 1024.0, 600.0), (1, 1024.0, 600.0)]);
    chrome.scene(0, 1024, 600);
    let initial = app.research_search.first_row;
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 500.0, 160.0, -100_000.0),
        ResearchWheel::Consumed
    );
    assert_eq!(
        app.research_search.first_row, initial,
        "first wheel scrolls inside the long row"
    );
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 500.0, 160.0, -100_000.0),
        ResearchWheel::Advance(1)
    );
    assert_eq!(app.research_search.first_row, initial + 1);
    assert_eq!(
        wheel(&mut chrome, &mut app, 0, 500.0, 160.0, 100_000.0),
        ResearchWheel::Advance(-1),
        "new projection starts at its top rather than retaining the old offset"
    );
    assert_eq!(app.research_search.first_row, initial);
}

#[test]
fn shrinking_rows_clamps_the_stored_projection_index() {
    let mut app = comparison_app();
    app.research_search.first_row = usize::MAX;
    app.research_rows(100);
    assert_eq!(
        app.research_search.first_row,
        app.research_search.rows.len() - 1
    );
    app.update(Action::OmnibarMove(-1));
    assert_eq!(
        app.research_search.first_row,
        app.research_search.rows.len() - 2
    );
}

#[test]
fn shrinking_structured_rows_clamps_the_stored_projection_index() {
    let mut app = comparison_app();
    app.research_search.view = crate::app::research_search_state::View::Structured;
    app.research_search.first_row = usize::MAX;
    app.research_rows(100);
    assert_eq!(
        app.research_search.first_row,
        app.research_search.rows.len() - 1
    );
    app.update(Action::OmnibarMove(-1));
    assert_eq!(
        app.research_search.first_row,
        app.research_search.rows.len() - 2
    );
}
