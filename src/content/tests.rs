// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn flip_toggles_and_failure_retries() {
    let node = Uuid::new_v4();
    let mut states = ContentStates::default();
    assert!(states.flip_spawns(node), "empty flips ON");
    states.note_requested(node);
    assert!(
        !states.flip_spawns(node),
        "in-flight flips OFF, not double-spawns"
    );
    states.note_live(node, None);
    assert!(!states.flip_spawns(node), "live flips OFF");
    states.note_closed(node);
    assert!(states.flip_spawns(node), "closed flips ON again");
    states.note_awaiting_input(node);
    assert!(
        !states.flip_spawns(node),
        "an input conversation toggles OFF"
    );
    states.note_closed(node);
    states.note_requested(node);
    states.note_failed(node, "no port".into());
    assert!(states.flip_spawns(node), "failed retries on the next flip");
}

#[test]
fn live_nodes_lists_only_live() {
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mut states = ContentStates::default();
    states.note_live(a, None);
    states.note_requested(b);
    states.note_failed(c, "x".into());
    let live: Vec<_> = states.live_nodes().collect();
    assert_eq!(live, vec![a]);
}

#[test]
fn fetched_documents_are_member_and_address_scoped() {
    let node = Uuid::new_v4();
    let document = FetchedDocument {
        bytes: b"# Capsule".to_vec(),
        content_type: Some("text/gemini".into()),
        body: "# Capsule".into(),
        effective_url: Some("gemini://example/one".into()),
        acquired_at_ms: 1,
    };
    let mut states = ContentStates::default();
    states.note_fetched(node, "gemini://example/one".into(), document.clone(), 9);
    assert_eq!(
        states.fetched(node, "gemini://example/one"),
        Some(&document)
    );
    assert!(states.fetched(node, "gemini://example/two").is_none());
}

#[test]
fn source_candidates_are_html_and_address_scoped() {
    let node = Uuid::new_v4();
    let mut states = ContentStates::default();
    states.note_fetched(
        node,
        "https://example.test/page".into(),
        FetchedDocument {
            bytes: b"<p>exact</p>".to_vec(),
            content_type: Some("text/html; charset=utf-8".into()),
            body: "<p>exact</p>".into(),
            effective_url: Some("https://example.test/page".into()),
            acquired_at_ms: 7,
        },
        12,
    );
    assert_eq!(
        states
            .source_capture_candidate(node, "https://example.test/page")
            .unwrap()
            .bytes,
        b"<p>exact</p>"
    );
    assert!(
        states
            .source_capture_candidate(node, "https://example.test/other")
            .is_none()
    );
    states.note_fetched(
        node,
        "https://example.test/page".into(),
        FetchedDocument {
            bytes: b"<p>unobserved target</p>".to_vec(),
            content_type: Some("text/html".into()),
            body: "<p>unobserved target</p>".into(),
            effective_url: None,
            acquired_at_ms: 8,
        },
        24,
    );
    assert!(
        states
            .source_capture_candidate(node, "https://example.test/page")
            .is_none(),
        "the request owner is never promoted to final-source evidence"
    );
}

#[test]
fn custody_retirement_clears_fetched_bodies_and_invalidates_late_answers() {
    let node = Uuid::new_v4();
    let mut states = ContentStates::default();
    states.begin_fetch(node, 41);
    states.note_fetched(
        node,
        "https://private.example/source".into(),
        FetchedDocument {
            bytes: b"private capture canary".to_vec(),
            content_type: Some("text/html".into()),
            body: "private capture canary".into(),
            effective_url: Some("https://private.example/source".into()),
            acquired_at_ms: 7,
        },
        22,
    );
    assert!(
        states
            .source_capture_candidate(node, "https://private.example/source")
            .is_some()
    );

    states.clear_source_documents();

    assert!(
        states
            .source_capture_candidate(node, "https://private.example/source")
            .is_none()
    );
    assert!(
        !states.finish_fetch(node, 41),
        "late response must be stale"
    );
}

#[test]
fn streamed_documents_accumulate_exact_bytes_before_decoding() {
    let node = Uuid::new_v4();
    let mut states = ContentStates::default();
    states.note_requested(node);
    states.note_streamed(
        node,
        "gemini://example/live".into(),
        "gemini://example/live".into(),
        Some("text/gemini".into()),
        &[b'#', b' ', 0xc3],
    );
    let received = states.note_streamed(
        node,
        "gemini://example/live".into(),
        "gemini://example/live".into(),
        Some("text/gemini".into()),
        &[0xa9, b'\n'],
    );
    assert_eq!(received, 5);
    assert_eq!(
        states
            .fetched(node, "gemini://example/live")
            .map(|document| document.body.as_str()),
        Some("# é\n")
    );
    assert!(matches!(
        states.fetch_phase(node),
        Some(PageFetchPhase::Streaming {
            received_bytes: 5,
            ..
        })
    ));
}
