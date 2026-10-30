// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn snapshot(body: &str, source: &str) -> crate::place::OfflinePlaceSnapshot {
    crate::place::OfflinePlaceSnapshot {
        captured: crate::place::CapturedCollectionCache {
            groups: vec![crate::place::CapturedContentGroup {
                extraction_schema: "fleece/1".into(),
                normalization: "plain".into(),
                reader_profile: "reader".into(),
                canonical_text_hash: "a".repeat(64),
                canonical_text_iri: "urn:captured:test".into(),
                canonical_text: body.into(),
                contributions: vec![crate::place::CapturedContribution {
                    share_operation: [1; 32],
                    annotation_manifest: [2; 32],
                    contributor: [3; 32],
                    source: source.into(),
                    title: "Field notes".into(),
                    shared_at_ms: 1,
                }],
            }],
            ..Default::default()
        },
        ..Default::default()
    }
}

fn offline(snapshot: crate::place::OfflinePlaceSnapshot) -> crate::place::PlaceState {
    crate::place::PlaceState::Offline {
        binding: crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([1; 32]),
            crate::place::SharedContainerId([2; 32]),
            crate::place::ChatSpaceId([3; 32]),
            "hall",
        )
        .unwrap(),
        generation: 1,
        snapshot,
    }
}

#[test]
fn captured_page_mode_finds_body_text_without_recall_rows() {
    let source = "https://example.test/field-notes";
    let mut app = App::test_stub();
    app.place = offline(snapshot("a body-only phrase", source));
    app.recall.push(crate::action::RecallHit {
        url: "https://recall.test/unrelated".into(),
        title: Some("a body-only phrase".into()),
        at_ms: 1,
    });

    app.update(Action::SearchCapturedPages);
    assert!(app.recall.is_empty() && app.recall_query.is_empty());
    app.update(Action::OmnibarInsert("body-only phrase".into()));

    assert!(app.omnibar.suggestions.iter().any(|row| {
        matches!(
            row,
            Suggestion::Act { label, action }
                if label == "Open source: Field notes"
                    && action == &Action::OpenAddress(source.into())
        )
    }));
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Recall { .. })),
        "captured-page search never blends browsing recall"
    );
}

#[test]
fn captured_page_mode_drops_results_after_stale_or_empty_refresh() {
    let source = "https://example.test/field-notes";
    let mut app = App::test_stub();
    app.place = offline(snapshot("a body-only phrase", source));
    app.update(Action::SearchCapturedPages);
    app.update(Action::OmnibarInsert("body-only phrase".into()));
    assert!(
        app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Act { .. }))
    );

    let requested = crate::place::PlaceCollectionVersion {
        moot: crate::place::PlaceId([1; 32]),
        collection: crate::place::PlaceCollectionId([4; 32]),
        frontier: vec![[5; 32]],
        membership_commitment: [6; 32],
    };
    let mut stale = snapshot("a body-only phrase", source);
    stale.captured_selection = crate::place::CapturedCollectionSelection::Collection {
        requested: requested.clone(),
        status: crate::place::CapturedCollectionSelectionStatus::Stale { current: requested },
    };
    app.place = offline(stale);
    app.recompute_omnibar_suggestions();
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Act { .. })),
        "a stale scope cannot retain prior hits"
    );

    app.place = offline(snapshot("a body-only phrase", source));
    app.recompute_omnibar_suggestions();
    assert!(
        app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Act { .. })),
        "the receipt starts with a visible captured result"
    );
    assert_eq!(
        app.apply_update(crate::action::Update::PlaceOpened {
            session: app.session_id,
            generation: 1,
            result: Ok(crate::place::OfflinePlaceSnapshot::default()),
        }),
        vec![Effect::Redraw]
    );
    assert!(
        !app.omnibar
            .suggestions
            .iter()
            .any(|row| matches!(row, Suggestion::Act { .. })),
        "a PlaceOpened replacement reflows the open field without typing"
    );
}

#[test]
fn captured_search_respects_a_single_row_setting() {
    let source = "https://example.test/field-notes";
    let mut app = App::test_stub();
    app.place = offline(snapshot("a body-only phrase", source));
    app.update(Action::SearchCapturedPages);
    app.update(Action::OmnibarInsert("body-only phrase".into()));
    app.recompute_captured_page_suggestions(1);
    assert!(matches!(app.omnibar.suggestions.as_slice(),
            [Suggestion::Act { action: Action::OpenAddress(url), .. }] if url == source));
    app.omnibar.text = "no-matching-content".into();
    app.recompute_captured_page_suggestions(1);
    assert_eq!(
        app.omnibar.suggestions,
        vec![Suggestion::Hint("no captured pages match")]
    );
}

#[test]
fn committing_a_captured_source_opens_its_source_address() {
    let source = "https://example.test/field-notes";
    let mut app = App::test_stub();
    app.place = offline(snapshot("a body-only phrase", source));
    app.update(Action::SearchCapturedPages);
    app.update(Action::OmnibarInsert("body-only phrase".into()));

    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { url, .. } if url == source
    )));
    assert_eq!(app.graph_runtimes.focused_url(), Some(source));
    let entry = app
        .shell_transcript()
        .entries()
        .last()
        .expect("captured source navigation enters the shell transcript");
    assert!(matches!(
        &entry.resolved_intent,
        crate::shell_services::ShellIntent::Navigate { url } if url == source
    ));
}
