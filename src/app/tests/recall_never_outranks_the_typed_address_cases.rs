// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Recall never displaces what the line already commits to: with an
/// address-shaped needle, the go row keeps the first-committed position and
/// the recalled pages sit under it.
#[test]
fn recall_never_outranks_the_typed_address() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    for c in "rust-lang.org".chars() {
        app.update(Action::OmnibarChar(c));
    }
    app.apply_update(Update::RecallHits {
        query: "rust-lang.org".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://docs.rs/".to_string(),
            title: None,
            at_ms: 42,
        }],
    });

    let rows = &app.omnibar.suggestions;
    let go = rows
        .iter()
        .position(|s| matches!(s, crate::ui::Suggestion::Go { .. }))
        .expect("an address-shaped needle offers its go row");
    let recalled = rows
        .iter()
        .position(|s| matches!(s, crate::ui::Suggestion::Recall { .. }))
        .expect("and the recalled page is offered too");
    assert!(go < recalled, "the typed address commits first: {rows:?}");
}

/// A late answer to text the user has typed past is dropped: the lane can
/// never show hits for a needle that is no longer in the line.
#[test]
fn superseded_recall_answers_drop() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('r'));
    app.update(Action::OmnibarChar('s'));
    app.update(Action::OmnibarChar('t'));

    app.apply_update(Update::RecallHits {
        query: "rs".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://stale.example/".to_string(),
            title: None,
            at_ms: 1,
        }],
    });
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Recall { .. })),
        "the answer to 'rs' is not shown against 'rst'"
    );
}

/// The lane's footing inside the configured row budget: a graph full of
/// matches displaces its least-recent node rows rather than swallowing every
/// recalled page, and the total still honors the setting.
#[test]
fn recall_keeps_a_footing_inside_the_row_budget() {
    let mut state = crate::ui::OmnibarState {
        open: true,
        text: "node".to_string(),
        ..crate::ui::OmnibarState::default()
    };
    let mut canvas = mere::canvas::Canvas::new();
    for i in 0..6 {
        canvas.visit(&format!("https://node{i}.example/"));
    }
    let recall: Vec<crate::action::RecallHit> = (0..5)
        .map(|i| crate::action::RecallHit {
            url: format!("https://recalled{i}.example/"),
            title: None,
            at_ms: i,
        })
        .collect();

    crate::ui::recompute_suggestions_with_limit(&mut state, &canvas, &[], &recall, 7);

    assert!(state.suggestions.len() <= 7, "the configured budget holds");
    let recalled = state
        .suggestions
        .iter()
        .filter(|s| matches!(s, crate::ui::Suggestion::Recall { .. }))
        .count();
    assert!(
        recalled > 0,
        "a full find lane does not swallow recall: {:?}",
        state.suggestions
    );
    assert!(
        state
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Node { .. })),
        "and node matches keep the larger share"
    );
}

/// The `>` actions lane searches intents, not pages: it asks for no recall,
/// and a needle narrowed below the floor drops the cached hits.
#[test]
fn the_actions_lane_and_short_needles_never_recall() {
    let mut app = App::test_stub();
    app.update(Action::OmnibarOpen { command: true });
    let effects = app.update(Action::OmnibarChar('f'));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::RecallQuery { .. })),
        "the > lane asks for no page recall"
    );

    app.update(Action::OmnibarClose);
    app.update(Action::OmnibarOpen { command: false });
    app.update(Action::OmnibarChar('a'));
    app.update(Action::OmnibarChar('b'));
    app.apply_update(Update::RecallHits {
        query: "ab".to_string(),
        hits: vec![crate::action::RecallHit {
            url: "https://ab.example/".to_string(),
            title: None,
            at_ms: 1,
        }],
    });
    assert!(
        !app.recall.is_empty(),
        "hits cached for the two-char needle"
    );
    app.update(Action::OmnibarBackspace);
    assert!(
        app.recall.is_empty(),
        "narrowing below the floor drops the pages the line is no longer about"
    );
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|s| matches!(s, crate::ui::Suggestion::Recall { .. })),
        "and the rows go with them in the same keystroke, not the next one"
    );
}

/// The row count is the configured maximum clamped by the window: a tall
/// window offers the whole setting, a short one offers what fits, and an
/// explicit setting is never raised by geometry.
#[test]
fn the_row_count_follows_the_viewport_under_the_configured_ceiling() {
    use crate::panes::ChromePlacement;
    use crate::ui::visible_row_limit;

    let tall = visible_row_limit(10, &ChromePlacement::Overlay, 1080.0, 1.0);
    assert_eq!(
        tall, 10,
        "a tall window offers the whole configured maximum"
    );

    // Use the actual card and font geometry. On this Mac the 24px rows
    // and 48px card chrome legitimately fit ten rows in the old 400px case.
    let (row_height, card_chrome_height) = crate::ui::ordinary_chrome_row_metrics(1.0);
    assert!(row_height.is_finite() && row_height > 0.0);
    assert!(card_chrome_height.is_finite() && card_chrome_height > 0.0);
    let short_viewport =
        crate::ui::CARD_TOP + card_chrome_height + crate::ui::CARD_BOTTOM_MARGIN + 5.5 * row_height;
    let short = visible_row_limit(10, &ChromePlacement::Overlay, short_viewport, 1.0);
    assert_eq!(
        short, 5,
        "the short window offers only the five rows that fit"
    );

    let cramped = visible_row_limit(10, &ChromePlacement::Overlay, 120.0, 1.0);
    assert_eq!(cramped, 3, "a window with no room still offers the floor");

    // Zoom only bites once the rows stop fitting: a 1080px window still has
    // room for the whole ceiling at 2.5x, so compare inside a window where
    // the text size is what runs out of room.
    let plain = visible_row_limit(10, &ChromePlacement::Overlay, 600.0, 1.0);
    let zoomed = visible_row_limit(10, &ChromePlacement::Overlay, 600.0, 2.5);
    assert!(
        zoomed < plain,
        "bigger text means fewer rows in the same window: {zoomed} vs {plain}"
    );

    assert_eq!(
        visible_row_limit(1, &ChromePlacement::Overlay, 1080.0, 1.0),
        1,
        "geometry never raises a deliberate setting above itself"
    );
}

#[test]
fn configured_row_limit_applies_to_the_live_omnibar_projection() {
    let mut app = App::test_stub();
    let mut chrome = app.shell_chrome_config().clone();
    chrome.omnibar.row_limit = 1;
    app.set_shell_chrome_config(chrome);
    app.update(Action::OmnibarOpen { command: true });
    assert_eq!(app.omnibar.suggestions.len(), 1);
}

#[test]
fn live_settings_snapshot_reconfigures_the_running_chrome_value_once() {
    let settings = pandect::ApplicationSettings {
        theme: Some(tabard::theme::choice::ThemeChoice::new(
            "theme:night",
            Some(tabard::theme::registry::Mode::Light),
        )),
        ui_zoom: 1.5,
        shellbar_edge: pandect::ShellbarEdge::Top,
        shellbar_hidden: true,
        ..pandect::ApplicationSettings::default()
    };
    let snapshot = crate::settings_pane::ChromeSettings::from(&settings);
    let mut app = App::test_stub();

    assert!(app.apply_chrome_settings_snapshot(&snapshot));
    assert_eq!(
        app.shell_chrome_config().shellbar.placement,
        crate::panes::ChromePlacement::Docked(crate::panes::ChromeEdge::Top)
    );
    assert!(!app.shell_chrome_config().shellbar.visible);
    assert_eq!(
        app.shell_chrome_config().appearance.theme_id.as_deref(),
        Some("theme:night")
    );
    assert_eq!(
        app.shell_chrome_config().appearance.theme_mode,
        crate::shell_services::ThemeMode::Light
    );
    assert_eq!(app.shell_chrome_config().appearance.zoom(), 1.5);
    assert!(
        !app.apply_chrome_settings_snapshot(&snapshot),
        "unchanged snapshots do not schedule an unbounded redraw loop"
    );
}

#[test]
fn actor_fetched_body_is_retained_for_the_requested_content_spawn() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);
    let request = app.content.active_fetch(node).unwrap();

    let effects = app.apply_update(Update::PageFetched {
        request,
        node,
        url: "gemini://capsule.test/".into(),
        result: Ok(crate::action::FetchedPage::text(
            Some("text/gemini".into()),
            "# One request",
        )),
    });

    assert_eq!(
        app.content
            .fetched(node, "gemini://capsule.test/")
            .map(|document| document.body.as_str()),
        Some("# One request")
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node: actual, url }
            if *actual == node && url == "gemini://capsule.test/"
    )));
}

#[test]
fn reload_after_a_failed_native_fetch_replaces_the_error_with_a_content_spawn() {
    let mut app = App::test_stub();
    let url = "7fc8950ec4e50695be76eddc8e539ee8:/page/turnstone.mu";
    app.update(Action::OpenAddress(url.into()));
    let node = app.graph_runtimes.focused_member().expect("opened node");
    app.content.note_requested(node);
    let first_request = app.content.active_fetch(node).expect("first request");
    app.apply_update(Update::PageFetched {
        request: first_request,
        node,
        url: url.into(),
        result: Err("native page temporarily unavailable".into()),
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Failed(_))
    ));

    let reload = app.update(Action::Reload);
    let request = reload
        .iter()
        .find_map(|effect| match effect {
            Effect::FetchPage {
                request,
                node: actual,
                url: actual_url,
                ..
            } if *actual == node && actual_url == url => Some(*request),
            _ => None,
        })
        .expect("reload requests the native page again");
    assert!(reload.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node: actual, url: actual_url }
            if *actual == node && actual_url == url
    )));
    assert_eq!(
        app.content.get(node),
        Some(&crate::content::NodeContent::Requested),
        "a successful replacement must be eligible to spawn"
    );

    let settled = app.apply_update(Update::PageFetched {
        request,
        node,
        url: url.into(),
        result: Ok(crate::action::FetchedPage::text(
            None,
            "> Turnstone\nRecovered native page\n",
        )),
    });
    assert!(settled.iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node: actual, url: actual_url }
            if *actual == node && actual_url == url
    )));
    assert_eq!(
        app.content
            .fetched(node, url)
            .map(|document| document.body.as_str()),
        Some("> Turnstone\nRecovered native page\n")
    );
}

#[test]
fn streamed_page_spawns_from_a_prefix_then_replaces_and_settles() {
    let mut app = App::test_stub();
    let url = "gemini://capsule.test/live";
    app.update(Action::OpenAddress(url.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);
    let request = app.content.active_fetch(node).unwrap();

    let first = app.apply_update(Update::PageStreamed {
        request,
        node,
        url: url.into(),
        response_url: url.into(),
        content_type: Some("text/gemini".into()),
        bytes: b"# Prefix\n".to_vec(),
    });
    assert!(first.contains(&Effect::SpawnContent {
        node,
        url: url.into(),
    }));
    app.apply_update(Update::ContentSpawned { node, facts: None });

    let second = app.apply_update(Update::PageStreamed {
        request,
        node,
        url: url.into(),
        response_url: url.into(),
        content_type: Some("text/gemini".into()),
        bytes: b"Tail\n".to_vec(),
    });
    assert!(second.contains(&Effect::UpdateContent {
        node,
        url: url.into(),
    }));
    assert_eq!(
        app.content
            .fetched(node, url)
            .map(|document| document.body.as_str()),
        Some("# Prefix\nTail\n")
    );

    let settled = app.apply_update(Update::PageFetched {
        request,
        node,
        url: url.into(),
        result: Ok(crate::action::FetchedPage::text(
            Some("text/gemini".into()),
            "# Prefix\nTail\n",
        )),
    });
    assert!(settled.contains(&Effect::UpdateContent {
        node,
        url: url.into(),
    }));
    assert!(matches!(
        app.content.fetch_phase(node),
        Some(crate::content::PageFetchPhase::Settled { received_bytes: 14 })
    ));
}

#[test]
fn ordinary_smolweb_input_navigates_and_refetches_with_a_percent_encoded_query() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("gemini://capsule.test/search".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);
    let request = app.content.active_fetch(node).unwrap();

    let effects = app.apply_update(Update::SmolwebInputRequested {
        request,
        node,
        url: "gemini://capsule.test/search".into(),
        input_url: "gemini://capsule.test/search".into(),
        prompt: "Search terms".into(),
        sensitive: false,
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingInput)
    ));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::CloseContent { node: actual } if *actual == node)
        )
    );
    assert!(matches!(
        &app.omnibar.mode,
        crate::ui::OmnibarMode::SmolwebInput(input)
            if input.prompt == "Search terms" && !input.sensitive
    ));

    app.update(Action::OmnibarInsert("two words/?".into()));
    let effects = app.update(Action::OmnibarCommit);
    let target = "gemini://capsule.test/search?two%20words%2F%3F";
    assert_eq!(app.graph_runtimes.focused_url(), Some(target));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { node: actual, url, owner_url, .. }
            if *actual == node && url == target && owner_url == target
    )));
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Requested)
    ));
}
