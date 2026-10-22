// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Real native page bytes through the product's correlation, reducer, standard
//! registry and controller boundary. These tests create no window or GPU.

use super::*;
use crate::app::App;
use crate::browse::PendingFetches;
use crate::nomadnet::{fetch_page_on_interface, parse_address};
use inker::SessionClick;
use retinue::endpoint::Endpoint;
use retinue::identity::PrivateIdentity;
use retinue::nomadnet::StaticNode;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const SOURCE: &[u8] = ">Café 雪\r\nSaved `!source`!\r\n`[About`:/page/about.mu]\r\n".as_bytes();
const PAGE_TIMEOUT: Duration = Duration::from_secs(30);
const PAGE_CAP: usize = 4 * 1024 * 1024;

fn request_from(
    app: &App,
    pending: &mut PendingFetches,
    effects: Vec<Effect>,
    address: &str,
) -> (Uuid, fetch::FetchRequestId) {
    let node = app
        .graph_runtimes
        .focused_member()
        .expect("focused native node");
    let commands = effects
        .iter()
        .flat_map(|effect| browse::fetch_commands_for(effect, pending));
    let requests = commands
        .filter_map(|command| match command {
            FetchCommand::Page {
                request,
                url,
                identity,
            } => {
                assert_eq!(url, address);
                assert!(
                    identity.is_none(),
                    "native reads do not borrow Gemini credentials"
                );
                Some(request)
            },
            FetchCommand::CancelPage { .. } => None,
            _ => panic!("unexpected native page command"),
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1, "one product page request");
    assert_eq!(app.content.active_fetch(node), Some(requests[0]));
    (node, requests[0])
}

fn navigate(
    app: &mut App,
    pending: &mut PendingFetches,
    address: &str,
) -> (Uuid, fetch::FetchRequestId) {
    let effects = app.update(Action::OpenAddress(address.to_owned()));
    let request = request_from(app, pending, effects, address);
    app.update(Action::ToggleNodeContent);
    assert!(matches!(
        app.content.get(request.0),
        Some(crate::content::NodeContent::Requested)
    ));
    request
}

fn admit(
    app: &mut App,
    pending: &mut PendingFetches,
    node: Uuid,
    request: fetch::FetchRequestId,
    address: &str,
    page: crate::action::FetchedPage,
) -> OpenedContentController {
    let bytes = page.bytes.clone();
    let body = page.body.clone();
    assert!(
        page.content_type.is_none(),
        "NomadNet does not invent a media type"
    );
    let update = browse::update_from_fetch(
        fetch::FetchUpdate::Page(fetch::FetchOutcome {
            request,
            url: address.to_owned(),
            result: Ok(fetch::Fetched {
                content_type: page.content_type,
                content_disposition: page.content_disposition,
                bytes: page.bytes,
                body: page.body,
            }),
        }),
        pending,
    )
    .expect("ordinary product request correlation accepts the answer");
    let effects = app.apply_update(update);
    assert!(
        effects.iter().any(|effect| matches!(effect,
            Effect::SpawnContent { node: actual, url } if *actual == node && url == address
        )),
        "the admitted native answer creates live content"
    );
    let held = app
        .content
        .fetched(node, address)
        .expect("product holds this response")
        .clone();
    assert_eq!(held.bytes, bytes);
    assert_eq!(held.body, body);
    assert!(held.content_type.is_none());
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone"),
        view: None,
        node: None,
        address: address.to_owned(),
        content_type: held.content_type.clone(),
        pinned_engine: crate::nomadnet::is_micron_address(address)
            .then(|| crate::nomadnet::ENGINE_ID.to_owned()),
    });
    assert_eq!(decision.engine_id, crate::nomadnet::ENGINE_ID);
    assert_eq!(
        dispatch_content_spawn(&decision.engine_id, false, &mut Vec::new(), node, address),
        ContentSpawnDispatch::Session
    );
    let prepared = prepare_content_controller(
        app,
        node,
        address,
        &decision.engine_id,
        Some(held),
        None,
        (640, 480),
    )
    .unwrap()
    .expect("the real product controller is prepared");
    assert_eq!(prepared.config.request.body.as_deref(), Some(body.as_str()));
    assert!(prepared.config.request.content_type.is_none());
    assert!(
        prepared.source.is_none(),
        "network receiving does not fabricate local source capture"
    );
    let mut opened = open_prepared_content_controller(
        app,
        prepared,
        Arc::new(super::super::standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        super::super::ShellClock(std::time::Instant::now()),
    )
    .unwrap()
    .expect("the registered Micron controller opens");
    assert!(opened.source.is_none());
    let scene = opened.controller.session_mut().frame(640, 480);
    if !body.is_empty() {
        assert!(
            scene
                .ops
                .iter()
                .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
        );
    }
    let native = opened
        .controller
        .session_mut()
        .as_any()
        .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
        .expect("the standard engine produced the retained native document session");
    assert_eq!(native.document_mut().document().address, address);
    app.apply_update(Update::ContentSpawned { node, facts: None });
    opened
}

async fn receive(
    app: &mut App,
    pending: &mut PendingFetches,
    request: (Uuid, fetch::FetchRequestId),
    address: &str,
    tcp: SocketAddr,
    expected: &[u8],
) -> OpenedContentController {
    let page =
        fetch_page_on_interface(parse_address(address).unwrap(), tcp, PAGE_TIMEOUT, PAGE_CAP)
            .await
            .unwrap_or_else(|error| panic!("native page fetch {address} failed: {error}"));
    assert_eq!(
        page.bytes, expected,
        "the binary response envelope preserves exact source bytes"
    );
    assert_eq!(
        page.body.as_bytes(),
        expected,
        "fixture source is unchanged UTF-8"
    );
    admit(app, pending, request.0, request.1, address, page)
}

fn follow_native_link(
    app: &mut App,
    node: Uuid,
    opened: &mut OpenedContentController,
    target: &str,
) -> String {
    let session = opened.controller.session_mut();
    let link = session
        .links()
        .into_iter()
        .find(|link| link.url == target)
        .expect("the real retained session exposes the qualified same-node link");
    let [x, y, width, height] = link.rect;
    let SessionClick::Navigate(href) = session.click_at(x + width / 2.0, y + height / 2.0) else {
        panic!("the laid-out native link must navigate");
    };
    let resolved = super::super::content_link_target(app, node, &href);
    assert_eq!(
        resolved, target,
        "host navigation retains native destination authority"
    );
    resolved
}

struct NativeFixture {
    endpoint: Arc<Endpoint>,
    tcp: SocketAddr,
    destination: String,
    seen: Arc<Mutex<Vec<retinue::hash::AddressHash>>>,
    completed: Arc<AtomicUsize>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    worker: Option<tokio::task::JoinHandle<()>>,
}

impl NativeFixture {
    async fn start(resource: Vec<u8>) -> Self {
        let endpoint = Arc::new(Endpoint::new(PrivateIdentity::from_secret_bytes(
            &[0x75; 64],
        )));
        let tcp = endpoint
            .listen_tcp(([127, 0, 0, 1], 0).into())
            .await
            .unwrap();
        let mut node = StaticNode::new();
        node.insert_page(b"/page/index.mu", SOURCE.to_vec())
            .unwrap();
        node.insert_page(b"/page/about.mu", resource).unwrap();
        node.insert_page(b"/page/empty.mu", Vec::new()).unwrap();
        node.register(&endpoint);
        let destination = node.destination(endpoint.identity()).to_string();
        let node = Arc::new(node);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let completed = Arc::new(AtomicUsize::new(0));
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let server = Arc::clone(&endpoint);
        let recorded = Arc::clone(&seen);
        let responses = Arc::clone(&completed);
        let worker = tokio::spawn(async move {
            let mut sessions = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    accepted = server.accept_resource() => {
                        let mut accepted = accepted.unwrap();
                        let node = Arc::clone(&node);
                        let recorded = Arc::clone(&recorded);
                        let responses = Arc::clone(&responses);
                        sessions.spawn(async move {
                            let received = tokio::time::timeout(PAGE_TIMEOUT, accepted.session.receive_request()).await.unwrap().unwrap();
                            assert!(received.request.data.is_empty(), "ordinary reading sends no submission data");
                            recorded.lock().unwrap().push(received.request.path_hash);
                            node.respond_to_request(&mut accepted.session, received)
                                .await
                                .expect("the client drains the Resource proof before closing");
                            responses.fetch_add(1, Ordering::SeqCst);
                        });
                    },
                    joined = sessions.join_next(), if !sessions.is_empty() => {
                        joined.unwrap().unwrap();
                    },
                }
            }
            while let Some(joined) = sessions.join_next().await {
                joined.expect("every admitted response completes before fixture shutdown");
            }
        });
        Self {
            endpoint,
            tcp,
            destination,
            seen,
            completed,
            stop: Some(stop),
            worker: Some(worker),
        }
    }

    async fn finish(mut self) {
        self.stop.take().unwrap().send(()).unwrap();
        let mut worker = self.worker.take().unwrap();
        match tokio::time::timeout(Duration::from_secs(5), &mut worker).await {
            Ok(joined) => joined.unwrap(),
            Err(_) => {
                worker.abort();
                let _ = worker.await;
                panic!("native fixture responses did not complete before shutdown");
            }
        }
        assert_eq!(
            self.completed.load(Ordering::SeqCst),
            self.seen.lock().unwrap().len(),
            "all admitted native responses completed, including Resource proofs"
        );
        self.endpoint.close();
    }
}

impl Drop for NativeFixture {
    fn drop(&mut self) {
        self.endpoint.close();
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
    }
}

async fn index_and_resource(
    app: &mut App,
    pending: &mut PendingFetches,
    tcp: SocketAddr,
    address: &str,
    expected: &[u8],
    resource_address: &str,
    resource: &[u8],
) {
    let request = navigate(app, pending, address);
    let mut opened = receive(app, pending, request, address, tcp, expected).await;
    let target = follow_native_link(app, request.0, &mut opened, resource_address);
    let next = navigate(app, pending, &target);
    let _resource_controller = receive(app, pending, next, &target, tcp, resource).await;
}

#[tokio::test]
async fn native_received_bytes_reach_controller_and_keep_reload_stop_and_empty_read_semantics() {
    tokio::time::timeout(Duration::from_secs(50), async {
        let mut resource = "Resource café 雪\r\nNUL\0\r\n".as_bytes().to_vec();
        while resource.len() < 128 * 1024 { resource.extend_from_slice(b"Saved native resource\r\n"); }
        resource.truncate(128 * 1024);
        let fixture = NativeFixture::start(resource.clone()).await;
        let address = format!("{}:/page/index.mu", fixture.destination);
        let resource_address = format!("{}:/page/about.mu", fixture.destination);
        let empty_address = format!("{}:/page/empty.mu", fixture.destination);
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(directory.path().join("app"));
        let mut pending = PendingFetches::default();
        index_and_resource(&mut app, &mut pending, fixture.tcp, &address, SOURCE, &resource_address, &resource).await;

        let reloaded = app.update(Action::Reload);
        let reload = request_from(&app, &mut pending, reloaded, &resource_address);
        let _ = receive(&mut app, &mut pending, reload, &resource_address, fixture.tcp, &resource).await;
        let empty = navigate(&mut app, &mut pending, &empty_address);
        let empty_controller = receive(&mut app, &mut pending, empty, &empty_address, fixture.tcp, &[]).await;
        assert!(empty_controller.controller.session().inspect().unwrap().title.is_none());
        assert!(app.content.fetched(empty.0, &empty_address).unwrap().bytes.is_empty());
        // Native empty binary replies identify neither refusal nor success status.
        // Keep an ordinary empty document rather than inventing a revoked-page UI.

        let effects = app.update(Action::Reload);
        let stopped = request_from(&app, &mut pending, effects, &empty_address);
        let stop = app.update(Action::Stop);
        assert!(stop.iter().any(|effect| matches!(effect, Effect::CancelPage { request, node } if *request == stopped.1 && *node == stopped.0)));
        for effect in &stop { browse::fetch_commands_for(effect, &mut pending); }
        let late = browse::update_from_fetch(fetch::FetchUpdate::Page(fetch::FetchOutcome {
            request: stopped.1, url: empty_address.clone(), result: Ok(fetch::Fetched {
                content_type: None, content_disposition: None, bytes: b"late".to_vec(), body: "late".into(),
            }),
        }), &mut pending);
        assert!(late.is_none(), "Stop removes correlation before a late native answer");
        assert!(app.content.fetched(empty.0, &empty_address).is_none_or(|held| held.body != "late"));
        let seen = fixture.seen.lock().unwrap().clone();
        assert_eq!(seen, [
            retinue::hash::AddressHash::of(b"/page/index.mu"),
            retinue::hash::AddressHash::of(b"/page/about.mu"),
            retinue::hash::AddressHash::of(b"/page/about.mu"),
            retinue::hash::AddressHash::of(b"/page/empty.mu"),
        ], "controller creation performs no duplicate fetch and reading performs no map submission");
        fixture.finish().await;
    }).await.expect("bounded native product receiving case");
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GovernedFixture {
    version: u8,
    tcp: SocketAddr,
    address: String,
    expected_index: PathBuf,
    resource_address: String,
    expected_resource: PathBuf,
}

fn fixture_bytes(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() <= PAGE_CAP);
    std::str::from_utf8(&bytes).unwrap();
    bytes
}

#[tokio::test]
#[ignore = "requires TURNSTONE_MICRON_RECEIVING_FIXTURE from a live governed Djinn fixture"]
async fn governed_djinn_native_page_reaches_product_controller() {
    tokio::time::timeout(Duration::from_secs(50), async {
        let path = std::env::var_os("TURNSTONE_MICRON_RECEIVING_FIXTURE").expect("set live governed receiving fixture JSON");
        let json = std::fs::read(path).unwrap();
        assert!(json.len() <= 65536);
        let fixture: GovernedFixture = serde_json::from_slice(&json).unwrap();
        assert_eq!(fixture.version, 1);
        assert!(fixture.tcp.ip().is_loopback(), "the receiving fixture uses only its private interface");
        let expected = fixture_bytes(&fixture.expected_index);
        let resource = fixture_bytes(&fixture.expected_resource);
        assert!(resource.len() > 1024, "exercise native Resource response receiving");
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::test_stub_at(directory.path().join("app"));
        let mut pending = PendingFetches::default();
        index_and_resource(&mut app, &mut pending, fixture.tcp, &fixture.address, &expected, &fixture.resource_address, &resource).await;
        println!("\n{}", serde_json::json!({
            "governed_micron_browser_receiving":true,
            "native_page_bytes_exact":true,"index_bytes":expected.len(),"resource_bytes":resource.len(),
            "request_correlation":true,"product_reducer":true,"standard_registered_engine":crate::nomadnet::ENGINE_ID,
            "retained_controller_frame":true,"qualified_same_node_navigation":true,"native_media_type_absent":true,
            "window_gpu_headed_qualified":false,"independent_python_go_radio_qualified":false,
        }));
    }).await.expect("bounded governed Djinn browser receiving case");
}
