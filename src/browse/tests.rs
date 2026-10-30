// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Two nodes share one URL; the stamp lands on the CORRELATED node, not
/// whichever `get_node_by_url` answers first (the bug this lane fixes).
#[test]
fn enrichment_stamps_the_requesting_node_among_url_twins() {
    let mut canvas = Canvas::new();
    let first_key = canvas.visit("https://elsewhere.example");
    let first = canvas.graph().get_node(first_key).unwrap().id;
    let target_key = canvas.visit("https://twin.example");
    let target = canvas.graph().get_node(target_key).unwrap().id;
    // Navigate the first node onto the same URL: two nodes, one address.
    assert!(canvas.navigate_member(first, "https://twin.example"));

    let effects = apply_page(
        &mut canvas,
        target,
        "https://twin.example".to_string(),
        Ok(FetchedPage::text(
            Some("text/html".to_string()),
            "<html><head><title>Twin B</title></head></html>",
        )),
    );
    assert!(!effects.is_empty());
    assert!(
        effects
            .iter()
            .all(|effect| !matches!(effect, Effect::RecordPageText { .. })),
        "ordinary fetch enrichment must not emit retained page bodies"
    );
    let titles: Vec<_> = canvas
        .graph()
        .nodes()
        .map(|(_, n)| (n.id, n.title.clone()))
        .collect();
    for (id, title) in titles {
        if id == target {
            assert_eq!(title, "Twin B", "the requester is enriched");
        } else {
            assert_ne!(title, "Twin B", "its URL twin is untouched");
        }
    }
}

/// A late result against a node that navigated away drops explicitly.
#[test]
fn superseded_node_drops_the_late_result() {
    let mut canvas = Canvas::new();
    let key = canvas.visit("https://before.example");
    let node = canvas.graph().get_node(key).unwrap().id;
    canvas.navigate_member(node, "https://after.example");

    let effects = apply_page(
        &mut canvas,
        node,
        "https://before.example".to_string(),
        Ok(FetchedPage::text(
            Some("text/html".to_string()),
            "<html><head><title>Stale</title></head></html>",
        )),
    );
    assert!(effects.is_empty(), "no stamps, no save, no redraw");
    let (_, n) = canvas.graph().get_node_by_id(node).unwrap();
    assert_ne!(n.title, "Stale", "the stale title never landed");
}

/// The pending table keeps same-address requests distinct and never
/// guesses at an unmatched answer.
#[test]
fn pending_table_correlates_and_refuses_to_guess() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut pending = PendingFetches::default();
    pending.note_page(1, "https://x.example", a, "https://owner-a.example", false);
    pending.note_page(2, "https://x.example", b, "https://owner-b.example", false);
    assert_eq!(pending.take_page(2).map(|page| page.node), Some(b));
    assert_eq!(pending.take_page(1).map(|page| page.node), Some(a));
    assert!(pending.take_page(2).is_none());

    let unmatched = update_from_fetch(
        FetchUpdate::Favicon(fetch::FaviconOutcome {
            request: 99,
            result: Ok(vec![1, 2, 3]),
        }),
        &mut pending,
    );
    assert!(
        unmatched.is_none(),
        "an unmatched completion is dropped, not guessed"
    );
}

#[test]
fn subresource_waiters_share_one_fetch_and_clear_together() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut pending = PendingFetches::default();
    assert!(pending.note_subresource("gemini://x.test/picture.png", a));
    assert!(!pending.note_subresource("gemini://x.test/picture.png", b));
    assert!(!pending.note_subresource("gemini://x.test/picture.png", a));
    assert!(pending.any_in_flight());

    let mut requesters = pending.take_subresources("gemini://x.test/picture.png");
    requesters.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(requesters, expected);
    assert!(!pending.any_in_flight());
}

#[test]
fn failed_favicon_completion_clears_its_pending_request() {
    let mut pending = PendingFetches::default();
    let node = Uuid::new_v4();
    pending.note_favicon(20, "https://example.test/article", node);
    assert!(
        pending.any_in_flight(),
        "favicon work remains pending until its terminal actor answer"
    );
    assert!(
        update_from_fetch(
            FetchUpdate::Favicon(fetch::FaviconOutcome {
                request: 20,
                result: Err("404 Not Found".to_string()),
            }),
            &mut pending,
        )
        .is_none(),
        "a favicon failure stays invisible to the UI"
    );
    assert!(
        !pending.any_in_flight(),
        "the terminal failure still clears its exact request"
    );
}

#[test]
fn favicon_completions_remove_only_their_own_request() {
    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    let mut pending = PendingFetches::default();
    let owner_url = "https://example.test/article";
    pending.note_favicon(30, owner_url, first);
    pending.note_favicon(31, owner_url, second);

    let update = update_from_fetch(
        FetchUpdate::Favicon(fetch::FaviconOutcome {
            request: 31,
            result: Ok(vec![1, 2, 3]),
        }),
        &mut pending,
    );
    assert!(matches!(
        update,
        Some(Update::FaviconFetched { node, owner_url: update_owner_url, bytes })
            if node == second && update_owner_url == owner_url && bytes == vec![1, 2, 3]
    ));
    assert!(pending.any_in_flight(), "the older request remains pending");

    let update = update_from_fetch(
        FetchUpdate::Favicon(fetch::FaviconOutcome {
            request: 30,
            result: Err("gone".to_string()),
        }),
        &mut pending,
    );
    assert!(update.is_none(), "the older failure stays UI-silent");
    assert!(!pending.any_in_flight(), "both exact requests are retired");
}

#[test]
fn page_progress_keeps_the_request_pending_until_terminal_answer() {
    let node = Uuid::new_v4();
    let request = 7;
    let request_url = "gemini://capsule.test/live";
    let mut pending = PendingFetches::default();
    pending.note_page(request, request_url, node, request_url, false);

    let update = update_from_fetch(
        FetchUpdate::PageProgress(fetch::PageProgress {
            request,
            url: request_url.into(),
            response_url: request_url.into(),
            content_type: Some("text/gemini".into()),
            bytes: b"# Prefix\n".to_vec(),
        }),
        &mut pending,
    )
    .unwrap();
    assert!(matches!(
        update,
        Update::PageStreamed {
            node: actual,
            bytes,
            ..
        } if actual == node && bytes == b"# Prefix\n"
    ));
    assert!(pending.page_in_flight(request_url, node, request_url));

    let terminal = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request,
            url: request_url.into(),
            result: Ok(fetch::Fetched::text(
                Some("text/gemini".into()),
                "# Prefix\nTail\n",
            )),
        }),
        &mut pending,
    );
    assert!(matches!(terminal, Some(Update::PageFetched { .. })));
    assert!(!pending.any_in_flight());
}

#[test]
fn superseding_and_stopping_cancel_only_the_named_request() {
    let node = Uuid::new_v4();
    let url = "gemini://capsule.test/slow";
    let mut pending = PendingFetches::default();
    pending.note_page(12, url, node, url, false);

    let commands = fetch_commands_for(
        &Effect::FetchPage {
            request: 13,
            supersedes: Some(12),
            node,
            url: url.into(),
            owner_url: url.into(),
            identity: None,
        },
        &mut pending,
    );
    assert!(matches!(
        commands.as_slice(),
        [
            FetchCommand::CancelPage { request: 12 },
            FetchCommand::Page { request: 13, .. }
        ]
    ));
    assert!(pending.page(12).is_none());
    assert_eq!(pending.page(13).map(|page| page.node), Some(node));

    let commands = fetch_commands_for(&Effect::CancelPage { request: 13, node }, &mut pending);
    assert!(matches!(
        commands.as_slice(),
        [FetchCommand::CancelPage { request: 13 }]
    ));
    assert!(!pending.any_in_flight());

    let late = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request: 13,
            url: url.into(),
            result: Err(fetch::FetchFailure::Cancelled),
        }),
        &mut pending,
    );
    assert!(late.is_none(), "a cancelled request cannot be reattached");
}

#[test]
fn input_response_keeps_actor_target_separate_from_graph_owner() {
    let node = Uuid::new_v4();
    let owner = "gemini://capsule.test/login";
    let request = 8;
    let request_url = "gemini://capsule.test/login?secret";
    let mut pending = PendingFetches::default();
    pending.note_page(request, request_url, node, owner, false);

    let update = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request,
            url: request_url.into(),
            result: Err(fetch::FetchFailure::InputRequired {
                url: request_url.into(),
                prompt: "Again".into(),
                sensitive: true,
            }),
        }),
        &mut pending,
    )
    .unwrap();
    assert!(matches!(
        update,
        Update::SmolwebInputRequested {
            node: actual,
            url,
            input_url,
            prompt,
            sensitive: true,
            ..
        } if actual == node
            && url == owner
            && input_url == request_url
            && prompt == "Again"
    ));
}

#[test]
fn certificate_request_prompts_once_then_reports_rejection() {
    let node = Uuid::new_v4();
    let owner = "gemini://capsule.test/account";
    let target = "gemini://capsule.test/private";
    let failure = || fetch::FetchFailure::ClientCertificateRequired {
        url: target.into(),
        prompt: "Identity required".into(),
        code: Some(60),
    };

    let mut anonymous = PendingFetches::default();
    anonymous.note_page(9, target, node, owner, false);
    let update = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request: 9,
            url: target.into(),
            result: Err(failure()),
        }),
        &mut anonymous,
    )
    .unwrap();
    assert!(matches!(
        update,
        Update::GeminiIdentityRequested {
            node: actual,
            url,
            identity_url,
            prompt,
            ..
        } if actual == node
            && url == owner
            && identity_url == target
            && prompt == "Identity required"
    ));

    let mut identified = PendingFetches::default();
    identified.note_page(10, target, node, owner, true);
    let update = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request: 10,
            url: target.into(),
            result: Err(failure()),
        }),
        &mut identified,
    )
    .unwrap();
    assert!(matches!(
        update,
        Update::PageFetched { result: Err(error), .. }
            if error == "client certificate rejected (60): Identity required"
    ));
}

#[test]
fn changed_certificate_preserves_the_fetch_target_and_both_fingerprints() {
    let node = Uuid::new_v4();
    let owner = "gemini://capsule.test/start";
    let request = 11;
    let request_url = "gemini://capsule.test:1966/private";
    let mut pending = PendingFetches::default();
    pending.note_page(request, request_url, node, owner, false);

    let update = update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request,
            url: request_url.into(),
            result: Err(fetch::FetchFailure::CertificateChanged {
                url: request_url.into(),
                target: "capsule.test:1966".into(),
                pinned: "11".repeat(32),
                seen: "22".repeat(32),
            }),
        }),
        &mut pending,
    )
    .unwrap();

    assert!(matches!(
        update,
        Update::GeminiCertificateChanged {
            node: actual,
            url,
            fetch_url,
            target,
            pinned,
            seen,
            ..
        } if actual == node
            && url == owner
            && fetch_url == request_url
            && target == "capsule.test:1966"
            && pinned == "11".repeat(32)
            && seen == "22".repeat(32)
    ));
}

#[test]
fn favicon_write_stores_png_bytes_under_their_digest() {
    let mut canvas = Canvas::new();
    let url = "https://icon.example/";
    let key = canvas.visit(url);
    let node = canvas.graph().get_node(key).unwrap().id;
    let fetched = crate::session::encode_rgba_png(&[255, 0, 0, 255], 1, 1).unwrap();

    let effects = apply_favicon(&mut canvas, node, url, &fetched);
    let (hex, stored) = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::StoreImage { hex, bytes } => Some((hex, bytes)),
            _ => None,
        })
        .expect("the favicon is durably stored");
    assert_eq!(&stored[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(hex, &eidetic::Hash::of(stored).to_hex());
    assert_eq!(
        canvas
            .graph()
            .get_node(key)
            .unwrap()
            .favicon()
            .unwrap()
            .hex(),
        *hex
    );
}
