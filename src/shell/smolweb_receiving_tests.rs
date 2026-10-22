// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Product receiving keeps the supplied body MIME and adopts both shared read
//! guards. Native header acquisition and client limits have their own gates.

use super::*;
use crate::app::App;
use crate::browse::PendingFetches;
use inker::{Block, EngineDocument, InlineSpan};
use std::sync::Arc;

const SCROLL: &str = "##### Fifth Café 雪\r\n=> /citation Primary [Citation]\r\n";

fn assert_scroll(document: &EngineDocument, heading: &str) {
    assert!(
        document.blocks.iter().any(|block| matches!(block,
            Block::Heading { level: 5, spans } if inker::inline_text(spans) == heading
        )),
        "the response uses Scroll's five-level heading grammar"
    );
    assert!(
        document
            .walk_inline_spans()
            .into_iter()
            .any(|span| matches!(span,
                InlineSpan::Link { predicate: Some(predicate), .. } if predicate == "Citation"
            )),
        "native relation predicates survive retained lowering"
    );
    assert_eq!(
        document.provenance.source_label.as_deref(),
        Some("scrolltext")
    );
}

#[test]
fn scroll_response_mime_reaches_initial_controller_and_live_replacement() {
    let address = "scroll://archive.test/reading.scroll";
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(directory.path().join("app"));
    let opened = app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    let mut pending = PendingFetches::default();
    let requests = opened
        .iter()
        .flat_map(|effect| browse::fetch_commands_for(effect, &mut pending))
        .filter_map(|command| match command {
            FetchCommand::Page { request, .. } => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    app.update(Action::ToggleNodeContent);
    let mime = "text/scroll;charset=utf-8;lang=fr-CA";
    let update = browse::update_from_fetch(
        fetch::FetchUpdate::Page(fetch::FetchOutcome {
            request: requests[0],
            url: address.into(),
            result: Ok(fetch::Fetched {
                content_type: Some(mime.into()),
                content_disposition: None,
                bytes: SCROLL.as_bytes().to_vec(),
                body: SCROLL.into(),
            }),
        }),
        &mut pending,
    )
    .unwrap();
    let effects = app.apply_update(update);
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::SpawnContent { node: actual, url } if *actual == node && url == address
    )));
    let held = app.content.fetched(node, address).unwrap().clone();
    assert_eq!(held.bytes, SCROLL.as_bytes());
    assert_eq!(held.content_type.as_deref(), Some(mime));
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone"),
        view: None,
        node: None,
        address: address.into(),
        content_type: held.content_type.clone(),
        pinned_engine: None,
    });
    assert_eq!(decision.engine_id, inker::routing::ENGINE_NEMATIC_SCROLL);
    let prepared = prepare_content_controller(
        &app,
        node,
        address,
        &decision.engine_id,
        Some(held.clone()),
        None,
        (640, 480),
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.config.request.content_type.as_deref(), Some(mime));
    let mut controller = open_prepared_content_controller(
        &app,
        prepared,
        Arc::new(super::super::standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        super::super::ShellClock(std::time::Instant::now()),
    )
    .unwrap()
    .unwrap()
    .controller;
    {
        let native = controller
            .session_mut()
            .as_any()
            .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
            .unwrap();
        assert_scroll(native.document_mut().document(), "Fifth Café 雪");
        let replacement = crate::content::FetchedDocument {
            bytes: b"##### Replacement\r\n=> /next Continued [Citation]\r\n".to_vec(),
            body: "##### Replacement\r\n=> /next Continued [Citation]\r\n".into(),
            content_type: Some(mime.into()),
            effective_url: Some(address.into()),
            acquired_at_ms: 1,
        };
        // This is the production UpdateContent boundary, not a parallel parser.
        replace_fetched_smolweb_body(native, address, &replacement);
        assert_scroll(native.document_mut().document(), "Replacement");
        let plain = crate::content::FetchedDocument {
            content_type: Some("text/plain".into()),
            ..replacement
        };
        replace_fetched_smolweb_body(native, address, &plain);
        assert!(
            !native
                .document_mut()
                .document()
                .walk_inline_spans()
                .into_iter()
                .any(|span| matches!(
                    span,
                    InlineSpan::Link {
                        predicate: Some(_),
                        ..
                    }
                )),
            "an explicitly changed response MIME does not retain old Scroll grammar"
        );
    }
    assert_eq!(
        app.content.fetched(node, address).unwrap(),
        &held,
        "painting does not rewrite retained source"
    );
}

#[test]
fn ordinary_product_reads_and_bodyless_sessions_require_explicit_submit_without_contact() {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let engines = super::super::standard_content_engines();
    for address in [
        format!("titan://127.0.0.1:{port}/upload;mime=text/plain;size=4;token=fixture"),
        format!("spartan://127.0.0.1:{port}/?send=body"),
        format!("spartan://127.0.0.1:{port}/?"),
    ] {
        let result = runtime.block_on(fetch::fetch_page_capped(&address, 4096));
        assert!(result.unwrap_err().contains("explicit Submit"));
        let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("turnstone"),
            view: None,
            node: None,
            address: address.clone(),
            content_type: None,
            pinned_engine: None,
        });
        assert!(
            engines
                .spawn(&decision.engine_id, &SessionSpawnRequest::new(address))
                .is_err()
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "both registered reading paths reject the effect before transport contact"
        );
    }
}
