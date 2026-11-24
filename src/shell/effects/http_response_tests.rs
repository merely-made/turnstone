// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn http_content_waits_for_terminal_response_policy() {
    let address = "https://example.test/start";
    let prefix_url = "https://example.test/earlier/";
    let final_url = "https://example.test/final/";
    let body = "<iframe src='child.html'></iframe>";
    let directory = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::test_stub_at(directory.path().join("app"));
    let opened = app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let mut pending = browse::PendingFetches::default();
    let mut requests = opened
        .iter()
        .flat_map(|effect| browse::fetch_commands_for(effect, &mut pending))
        .filter_map(|command| match command {
            FetchCommand::Page { request, .. } => Some(request),
            _ => None,
        });
    let request = requests
        .next()
        .expect("opening the address requests a page");
    assert_eq!(requests.next(), None);
    app.update(Action::ToggleNodeContent);

    let progress = browse::update_from_fetch(
        FetchUpdate::PageProgress(fetch::PageProgress {
            request,
            url: address.into(),
            response_url: prefix_url.into(),
            content_type: Some("text/html".into()),
            bytes: body.as_bytes().to_vec(),
        }),
        &mut pending,
    )
    .unwrap();
    assert!(!app.apply_update(progress).iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { .. } | Effect::UpdateContent { .. }
    )));
    assert!(app.content.fetched_for_spawn(node, address).is_none());
    let prefix_acquired_at_ms = app.content.fetched(node, address).unwrap().acquired_at_ms;

    let terminal = browse::update_from_fetch(
        FetchUpdate::Page(fetch::FetchOutcome {
            request,
            url: address.into(),
            result: Ok(fetch::Fetched {
                http_response: Some(page_load::HttpDocumentResponse::new(
                    final_url,
                    [("Referrer-Policy".into(), "origin".into())],
                )),
                ..fetch::Fetched::text(Some("text/html".into()), body)
            }),
        }),
        &mut pending,
    )
    .unwrap();
    assert!(app.apply_update(terminal).iter().any(|effect| matches!(
        effect,
        Effect::SpawnContent { node: actual, url } if *actual == node && url == address
    )));

    let fetched = app
        .content
        .fetched_for_spawn(node, address)
        .unwrap()
        .clone();
    assert_eq!(fetched.bytes, body.as_bytes());
    assert_eq!(fetched.acquired_at_ms, prefix_acquired_at_ms);
    assert_eq!(fetched.effective_url.as_deref(), Some(final_url));
    let prepared = prepare_content_controller(
        &app,
        node,
        address,
        inker::routing::ENGINE_GENET_SCRIPTED,
        Some(fetched),
        None,
        (640, 480),
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.config.load_mode, pelt_core::PeltLoadMode::Host);
    let request = prepared.config.request;
    assert_eq!(request.address, address);
    assert_eq!(request.body.as_deref(), Some(body));
    assert_eq!(request.content_type.as_deref(), Some("text/html"));
    let response = request
        .http_response
        .expect("the prepared request carries the terminal HTTP policy");
    assert_eq!(response.final_url(), final_url);
    assert_eq!(
        response.policy_headers(),
        &[("Referrer-Policy".to_owned(), "origin".to_owned())]
    );
}

#[test]
fn live_http_document_reopens_with_terminal_response_policy() {
    let address = "https://example.test/document";
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let request = app.content.active_fetch(node).unwrap();
    app.apply_update(Update::ContentSpawned {
        node,
        facts: Some(crate::content::ContentFacts {
            engine: inker::routing::ENGINE_GENET_SCRIPTED.into(),
            lane: crate::content::ContentLane::Document,
            structure: None,
            lineage: None,
            capabilities: Default::default(),
        }),
    });

    let mut terminal = crate::action::FetchedPage::text(Some("text/html".into()), "<p>Changed</p>");
    terminal.http_response = Some(page_load::HttpDocumentResponse::new(
        address,
        [("Content-Security-Policy".into(), "frame-src 'none'".into())],
    ));
    let effects = app.apply_update(Update::PageFetched {
        request,
        node,
        url: address.into(),
        result: Ok(terminal),
    });
    assert!(effects.contains(&Effect::SpawnContent {
        node,
        url: address.into(),
    }));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::UpdateContent { .. }))
    );
}

#[test]
fn linked_reading_resolves_against_terminal_response_url() {
    let address = "https://example.test/requested/notes.jsonld";
    let final_url = "https://example.test/final/notes.jsonld";
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let request = app.content.active_fetch(node).unwrap();
    let mut terminal = crate::action::FetchedPage::text(
        Some("application/ld+json".into()),
        r#"{
          "@context":"http://www.w3.org/ns/anno.jsonld",
          "id":"urn:example:annotation", "type":"Annotation",
          "target":{"source":"take.wav",
                    "selector":{"type":"FragmentSelector","conformsTo":"http://www.w3.org/TR/media-frags/","value":"t=1.250,4.500"}}
        }"#,
    );
    terminal.http_response = Some(page_load::HttpDocumentResponse::new(final_url, []));
    app.apply_update(Update::PageFetched {
        request,
        node,
        url: address.into(),
        result: Ok(terminal),
    });

    let (_, source, reading) = app.content.linked_reading(node, address).unwrap();
    assert_eq!(source, final_url);
    assert_eq!(reading.passages.len(), 1);
    assert_eq!(
        reading.passages[0].address.as_str(),
        "https://example.test/final/take.wav#t=1.250,4.500"
    );
}
