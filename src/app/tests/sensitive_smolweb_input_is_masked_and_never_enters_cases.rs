// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn sensitive_smolweb_input_is_masked_and_never_enters_graph_or_events() {
    let mut app = App::test_stub();
    let owner = "gemini://capsule.test/login";
    app.update(Action::OpenAddress(owner.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let request = app.content.active_fetch(node).unwrap();
    let _ = app.take_events();
    app.apply_update(Update::SmolwebInputRequested {
        request,
        node,
        url: owner.into(),
        input_url: owner.into(),
        prompt: "Password".into(),
        sensitive: true,
    });

    app.update(Action::OmnibarInsert("hunter 2".into()));
    let snapshot = crate::observe::snapshot(&app);
    assert_eq!(snapshot.omnibar.text, "\u{2022}".repeat(8));
    assert!(!format!("{snapshot:?}").contains("hunter"));
    assert!(
        crate::a11y::a11y_lines(&app)
            .iter()
            .all(|line| !line.contains("hunter"))
    );
    assert!(
        app.recall_query.is_empty(),
        "private input never asks recall"
    );

    let effects = app.update(Action::OmnibarCommit);
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { node: actual, url, owner_url, .. }
            if *actual == node
                && url == "gemini://capsule.test/login?hunter%202"
                && owner_url == owner
    )));
    let described: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(described.iter().all(|event| !event.contains("hunter")));
}

#[test]
fn gemini_identity_is_confirmed_once_and_reused_only_for_its_capsule() {
    let mut app = App::test_stub();
    let owner = "gemini://capsule.test/account";
    let target = "gemini://capsule.test/private";
    app.update(Action::OpenAddress(owner.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);
    let request = app.content.active_fetch(node).unwrap();

    let effects = app.apply_update(Update::GeminiIdentityRequested {
        request,
        node,
        url: owner.into(),
        identity_url: target.into(),
        prompt: "Identity required".into(),
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingIdentity)
    ));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::CloseContent { node: actual } if *actual == node)
        )
    );
    assert!(matches!(
        &app.omnibar.mode,
        crate::ui::OmnibarMode::GeminiIdentity(input)
            if input.identity_url == target && input.prompt == "Identity required"
    ));
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));

    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage {
            node: actual,
            url,
            owner_url,
            identity: Some(identity),
            ..
        } if *actual == node
            && url == target
            && owner_url == owner
            && identity.origin() == "gemini://capsule.test"
    )));
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Requested)
    ));
    assert_eq!(app.graph_runtimes.focused_url(), Some(owner));

    let same_capsule =
        app.fetch_page_effect(node, "gemini://capsule.test/again".into(), owner.into());
    assert!(matches!(
        same_capsule,
        Effect::FetchPage {
            identity: Some(_),
            ..
        }
    ));
    let other_capsule = app.fetch_page_effect(
        node,
        "gemini://other.test/".into(),
        "gemini://other.test/".into(),
    );
    assert!(matches!(
        other_capsule,
        Effect::FetchPage { identity: None, .. }
    ));

    let described: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(
        described
            .iter()
            .any(|event| event.contains("gemini-identity-requested"))
    );
    assert!(
        described
            .iter()
            .any(|event| event.contains("gemini-identity-bound"))
    );
}

#[test]
fn changed_gemini_certificate_requires_an_explicit_trust_commit_before_refetch() {
    let mut app = App::test_stub();
    let owner = "gemini://capsule.test/start";
    let fetch_url = "gemini://capsule.test:1966/private";
    let pinned = "11".repeat(32);
    let seen = "22".repeat(32);
    app.update(Action::OpenAddress(owner.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_requested(node);
    let request = app.content.active_fetch(node).unwrap();

    let effects = app.apply_update(Update::GeminiCertificateChanged {
        request,
        node,
        url: owner.into(),
        fetch_url: fetch_url.into(),
        target: "capsule.test:1966".into(),
        pinned: pinned.clone(),
        seen: seen.clone(),
    });
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingTrust)
    ));
    assert!(
        effects.iter().any(
            |effect| matches!(effect, Effect::CloseContent { node: actual } if *actual == node)
        )
    );
    assert!(matches!(
        &app.omnibar.mode,
        crate::ui::OmnibarMode::GeminiTrust(prompt)
            if prompt.target == "capsule.test:1966"
                && prompt.pinned == pinned
                && prompt.seen == seen
    ));

    let effects = app.update(Action::OmnibarCommit);
    assert_eq!(effects, vec![Effect::Redraw]);
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::AwaitingTrust)
    ));

    app.update(Action::OmnibarInsert("trust".into()));
    let effects = app.update(Action::OmnibarCommit);
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::ReplaceGeminiTrust {
            node: actual,
            fetch_url: actual_fetch,
            owner_url,
            target,
            pinned: actual_pinned,
            seen: actual_seen,
        } if *actual == node
            && actual_fetch == fetch_url
            && owner_url == owner
            && target == "capsule.test:1966"
            && actual_pinned == &pinned
            && actual_seen == &seen
    )));
    assert!(matches!(
        app.content.get(node),
        Some(crate::content::NodeContent::Requested)
    ));

    let described: Vec<_> = app
        .take_events()
        .into_iter()
        .map(|event| event.describe())
        .collect();
    assert!(
        described
            .iter()
            .any(|event| event.contains("gemini-certificate-changed"))
    );
}

#[test]
fn titan_address_opens_an_explicit_composer_without_uploading() {
    let mut app = App::test_stub();
    let target = "titan://capsule.test/posts/new";
    let effects = app.update(Action::OpenAddress(target.into()));
    assert!(!effects.iter().any(|effect| matches!(
        effect,
        Effect::FetchPage { .. } | Effect::SubmitSmolweb { .. }
    )));
    assert!(matches!(
        app.omnibar.mode,
        crate::ui::OmnibarMode::SmolwebSubmission(crate::ui::SmolwebSubmissionPrompt {
            protocol: crate::ui::SmolwebSubmissionProtocol::Titan,
            stage: crate::ui::SmolwebSubmissionStage::Body,
            ..
        })
    ));
}

#[test]
fn titan_composer_stages_body_mime_hidden_token_and_literal_confirmation() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress("titan://capsule.test/upload".into()));
    app.update(Action::OmnibarInsert("# Draft".into()));
    app.update(Action::OmnibarCommit);
    assert!(matches!(
        app.omnibar.mode,
        crate::ui::OmnibarMode::SmolwebSubmission(crate::ui::SmolwebSubmissionPrompt {
            stage: crate::ui::SmolwebSubmissionStage::Mime,
            ..
        })
    ));
    app.update(Action::OmnibarCommit);
    app.update(Action::OmnibarInsert("private-token".into()));
    assert!(app.omnibar.sensitive());
    assert!(!app.omnibar.presented_text().contains("private-token"));
    app.update(Action::OmnibarCommit);
    app.update(Action::OmnibarInsert("not yet".into()));
    assert!(
        !app.update(Action::OmnibarCommit)
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitSmolweb { .. }))
    );
    app.omnibar.text.clear();
    app.omnibar.cursor = 0;
    app.update(Action::OmnibarInsert("send".into()));
    let effects = app.update(Action::OmnibarCommit);
    let effect = effects
        .iter()
        .find(|effect| matches!(effect, Effect::SubmitSmolweb { .. }))
        .expect("confirmed write effect");
    let Effect::SubmitSmolweb {
        body,
        mime,
        token,
        protocol,
        ..
    } = effect
    else {
        unreachable!()
    };
    assert_eq!(body, b"# Draft");
    assert_eq!(mime, "text/gemini");
    assert_eq!(
        token.as_ref().map(|secret| secret.as_str()),
        Some("private-token")
    );
    assert_eq!(*protocol, crate::ui::SmolwebSubmissionProtocol::Titan);
    assert!(!format!("{effect:?}").contains("private-token"));
}

#[test]
fn spartan_prompt_target_resolves_against_its_source_member() {
    let mut app = App::test_stub();
    app.update(Action::OpenAddress(
        "spartan://capsule.test/guestbook".into(),
    ));
    let source = app.graph_runtimes.focused_member().expect("source member");
    app.update(Action::BeginSmolwebSubmission {
        source: Some(source),
        target: "/guestbook/sign".into(),
    });
    let crate::ui::OmnibarMode::SmolwebSubmission(prompt) = &app.omnibar.mode else {
        panic!("expected composer");
    };
    assert_eq!(prompt.target, "spartan://capsule.test/guestbook/sign");
    assert_eq!(
        prompt.protocol,
        crate::ui::SmolwebSubmissionProtocol::Spartan
    );
}

#[test]
fn a_left_place_refuses_reconnect_and_rejoins_by_clearing_its_mark() {
    let mut app = App::test_stub();
    assert_eq!(app.update(Action::RejoinPlace), vec![Effect::Redraw]);
    assert_eq!(app.place, crate::place::PlaceState::Personal);
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xb1; 32]),
        crate::place::SharedContainerId([0xb2; 32]),
        crate::place::ChatSpaceId([0xb3; 32]),
        "hall",
    )
    .unwrap();
    let retained = crate::place::PlaceLeftSummary {
        graph_nodes: 1,
        chat_messages: 4,
        members: 2,
    };
    app.next_place_generation = 8;
    app.place = crate::place::PlaceState::Left {
        binding: binding.clone(),
        retained,
    };
    // Reconnect reads Left as not joined: the mark would otherwise survive a
    // live reconnect and read Left again at the next start.
    assert_eq!(app.update(Action::ReconnectPlace), vec![Effect::Redraw]);
    assert!(matches!(app.place, crate::place::PlaceState::Left { .. }));
    assert_eq!(
        app.update(Action::RejoinPlace),
        vec![
            Effect::ClearPlaceLeftMark {
                session: app.session_id
            },
            Effect::ReconnectPlace {
                session: app.session_id,
                generation: 9,
                binding: binding.clone(),
                contact: None,
            },
        ]
    );
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Opening { generation: 9, .. }
    ));
}

#[test]
fn reconnect_place_requires_binding_and_ignores_departed_generation() {
    let mut app = App::test_stub();
    assert_eq!(app.update(Action::ReconnectPlace), vec![Effect::Redraw]);
    assert_eq!(app.place, crate::place::PlaceState::Personal);
    let binding = crate::place::PlaceBindingV1::new(
        crate::place::PlaceId([0xa1; 32]),
        crate::place::SharedContainerId([0xa2; 32]),
        crate::place::ChatSpaceId([0xa3; 32]),
        "hall",
    )
    .unwrap();
    app.next_place_generation = 8;
    app.place = crate::place::PlaceState::Offline {
        binding: binding.clone(),
        generation: 8,
        snapshot: crate::place::OfflinePlaceSnapshot::default(),
    };
    assert_eq!(
        app.update(Action::ReconnectPlace),
        vec![Effect::ReconnectPlace {
            session: app.session_id,
            generation: 9,
            binding: binding.clone(),
            contact: None,
        }]
    );
    let stale = Update::PlaceOpened {
        session: app.session_id,
        generation: 8,
        result: Ok(crate::place::OfflinePlaceSnapshot::default()),
    };
    assert!(app.apply_update(stale).is_empty());
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Opening { generation: 9, .. }
    ));
    app.apply_update(Update::PlaceOpened {
        session: app.session_id,
        generation: 9,
        result: Err("offer expired".into()),
    });
    assert!(matches!(
        app.place,
        crate::place::PlaceState::Degraded { generation: 9, .. }
    ));
    assert_eq!(app.place.binding(), Some(&binding));
    assert_eq!(
        app.update(Action::ReconnectPlace),
        vec![Effect::ReconnectPlace {
            session: app.session_id,
            generation: 10,
            binding,
            contact: None,
        }]
    );
    assert!(
        app.apply_update(Update::PlaceOpened {
            session: app.session_id,
            generation: 9,
            result: Ok(crate::place::OfflinePlaceSnapshot::default()),
        })
        .is_empty()
    );
}

#[test]
fn micron_form_editor_is_explicit_redacts_masked_input_and_emits_one_request() {
    let mut app = App::test_stub();
    let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
    let source = "#!c=0\n`[Submit`:/page/capture.mu`mnprobe_text|mnprobe_mask|mnprobe_fixed=ready]\nText: `<mnprobe_text`seed-text>\nMasked: `<!|mnprobe_mask`mask-seed>\n";
    app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_fetched(
        node,
        address.into(),
        crate::content::FetchedDocument {
            bytes: source.as_bytes().to_vec(),
            content_type: None,
            body: source.into(),
            effective_url: Some(address.into()),
            acquired_at_ms: 1,
        },
        source.len(),
    );

    assert!(
        !app.update(Action::ComposeFocusedMicronForm)
            .iter()
            .any(|effect| matches!(effect, Effect::SubmitMicron { .. }))
    );
    assert!(matches!(
        app.omnibar.mode,
        crate::ui::OmnibarMode::MicronForm(_)
    ));
    app.update(Action::OmnibarCommit); // text
    for _ in 0.."mask-seed".len() {
        app.update(Action::OmnibarBackspace);
    }
    app.update(Action::OmnibarInsert("not-for-debug".into()));
    assert!(app.omnibar.sensitive());
    assert!(!format!("{:?}", app.omnibar).contains("not-for-debug"));
    app.update(Action::OmnibarCommit); // masked
    app.update(Action::OmnibarInsert("send".into()));
    let effects = app.update(Action::OmnibarCommit);
    let requests = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::SubmitMicron { submission, .. } => Some(submission),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].target,
        "7fc8950ec4e50695be76eddc8e539ee8:/page/capture.mu"
    );
    assert_eq!(
        requests[0].values.get("field_mnprobe_text"),
        Some(&"seed-text".into())
    );
    assert_eq!(
        requests[0].values.get("field_mnprobe_mask"),
        Some(&"not-for-debug".into())
    );
    assert_eq!(
        requests[0].values.get("var_mnprobe_fixed"),
        Some(&"ready".into())
    );
    assert!(!format!("{:?}", effects).contains("not-for-debug"));
}
