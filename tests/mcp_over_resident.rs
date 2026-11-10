// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `graphshell-mcp` over a live mere route: the real reservoir, the real route
//! grants and the real session item, reached through the same endpoint catalog
//! the application door serves. The adapter holds no key; what it may do is
//! what the door admitted its app id for.
//!
//! The fixture is the resident's, public behind its `test-fixtures` feature
//! (engine split D82: the test moved here from `mere-resident` with the MCP
//! crate's home, D23).

use chirograph::{
    Carrier, CarrierError, CarrierNotice, CarrierRequest, CarrierRequestBody, CarrierResponseBody,
    SessionOpened, SessionStatus,
};
use graphshell_endpoint::{ProjectionCatalog, ProjectionSource, dispatch_common};
use graphshell_host::endpoint_catalog::ResidentEndpointSession;
use graphshell_mcp::{Config, Server};
use mere_overmap::wire::{
    APPEND_NOTE_INTENT, APPLY_EDITS_INTENT, AppendNoteV1, ApplyEditsV1, SEARCH_NOTES_INTENT,
    SearchNotesV1, StepV1, UNDO_INTENT,
};
use mere_resident::resident_mere::fixture::{Fixture, context, fixture, persona};
use mere_resident::resident_mere::{MERE_NOTICE_POLL, ambient_mere_route_id};
use ostiary::app_admission::{AppId, AppRequest, AppRouteId};
use ostiary::route::ResidentEndpointRoute;
use pandect::mere_dir;
use serde_json::{Value, json};

/// A node-adding edit for node `n`.
fn node(n: u128) -> pandect::CapturedDelta {
    pandect::CapturedDelta::ReplayAddNodeWithIdIfMissing {
        id: uuid::Uuid::from_u128(n).to_string(),
        url: format!("https://{n}.test/"),
        position: [0.0, 0.0],
    }
}

/// A retitling edit for node `n`.
fn retitle(n: u128, title: &str) -> pandect::CapturedDelta {
    pandect::CapturedDelta::ReplaySetNodeTitleById {
        node_id: uuid::Uuid::from_u128(n).to_string(),
        title: title.to_string(),
    }
}

/// How many nodes the session's graph projection shows, the session item aside.
fn graph_nodes(session: &mut ResidentEndpointSession) -> usize {
    let request = session.describe().projections[1].request.clone();
    session.snapshot(request).unwrap().scene.active_items_in_order().len() - 1
}

const AGENT: &str = "mcp.claude-code";

/// An admitted endpoint session as a `Carrier`: what `AppRouteCarrier` is over
/// the socket, without the socket.
struct Admitted {
    session: ResidentEndpointSession,
    next: u64,
}

impl Carrier for Admitted {
    fn request(&mut self, body: CarrierRequestBody) -> Result<CarrierResponseBody, CarrierError> {
        if let CarrierRequestBody::Open(_) = body {
            return Ok(CarrierResponseBody::Opened(Box::new(SessionOpened {
                version: chirograph::ProtocolVersion::V1,
                descriptor: self.session.describe(),
                status: SessionStatus::Live,
                expires_at_ms: None,
            })));
        }
        self.next += 1;
        let request = CarrierRequest {
            id: self.next,
            body,
        };
        let mut resume = |endpoint: &mut ResidentEndpointSession, request| endpoint.resume(request);
        match dispatch_common(&mut self.session, request, &mut resume) {
            Ok(response) => response
                .body
                .map_err(|failure| CarrierError::Refused(failure.message)),
            Err(plane) => Err(CarrierError::Refused(format!("session plane: {plane:?}"))),
        }
    }

    fn take_notice(&mut self) -> Option<CarrierNotice> {
        None
    }

    fn wait_for_notice(&mut self) -> Result<CarrierNotice, CarrierError> {
        Err(CarrierError::Disconnected("no notices".into()))
    }

    fn shutdown(&mut self) -> Result<(), CarrierError> {
        Ok(())
    }
}

fn modern(id: u64, method: &str, params: Value) -> String {
    let mut params = params;
    params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn ask(server: &mut Server<Admitted>, method: &str, params: Value) -> Value {
    let reply = server.handle(&modern(1, method, params)).expect("a reply");
    serde_json::from_str(&reply).unwrap()
}

fn tool(server: &mut Server<Admitted>, name: &str, arguments: Value) -> Value {
    ask(
        server,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

fn read(server: &mut Server<Admitted>, uri: &str) -> Value {
    let reply = ask(server, "resources/read", json!({ "uri": uri }));
    serde_json::from_str(reply["result"]["contents"][0]["text"].as_str().unwrap()).unwrap()
}

async fn server_for(fixture: &Fixture, route: &str, app: &str, writes: bool) -> Server<Admitted> {
    let session = fixture
        .catalog
        .update(|catalog| catalog.open(route, &context(app)))
        .await
        .unwrap();
    let config = Config {
        scope: route.into(),
        writes,
        ..Config::default()
    };
    Server::new(Admitted { session, next: 0 }, config)
}

/// The resource URI of the projection the route labels `name`.
fn resource_named(server: &mut Server<Admitted>, name: &str) -> String {
    let listed = ask(server, "resources/list", json!({}));
    listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["name"] == name)
        .unwrap_or_else(|| panic!("the route offers no {name}: {listed}"))["uri"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The resource URI of the attached session's graph projection.
fn graph_resource(server: &mut Server<Admitted>) -> String {
    resource_named(server, "Attached session")
}

/// The graph projection's session-item action named `intent`, with the
/// session, epoch and revision it was read at.
fn advertised(server: &mut Server<Admitted>, intent: &str) -> (Value, Value, Value) {
    let listed = tool(server, "mere_actions", json!({}));
    for projection in listed["result"]["structuredContent"]["projections"]
        .as_array()
        .unwrap()
    {
        for action in projection["actions"].as_array().unwrap() {
            if action["intent"] == intent {
                return (
                    action["instance"].clone(),
                    projection["epoch"].clone(),
                    projection["revision"].clone(),
                );
            }
        }
    }
    panic!("the route does not advertise {intent}: {listed}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_writes_a_mere_as_its_granted_app_and_the_door_names_it() {
    let fixture = fixture(41, "memory").await;
    let mut route_request = AppRequest::new(AppId::new(AGENT), AppRouteId::new(&fixture.route).unwrap());
    // The door admits by declaration only (capability door D24).
    route_request.declaration = Some(ostiary::declaration::AppDeclaration {
        routes: vec![ostiary::declaration::NamePattern::exact(&fixture.route)],
        ..Default::default()
    });
    assert!(
        fixture.grants.current().admit(&route_request).is_err(),
        "an app the owner has not admitted reaches no mere"
    );
    fixture.grants.grant(
        AppId::new(AGENT),
        ResidentEndpointRoute::new("identity", MERE_NOTICE_POLL).unwrap(),
    );
    assert!(fixture.grants.current().admit(&route_request).is_ok());

    let mut mcp = server_for(&fixture, &fixture.route, AGENT, true).await;
    let listed = ask(&mut mcp, "resources/list", json!({}));
    let names: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| resource["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Sessions", "Attached session", "Archive", "Notes"]);
    let graph_uri = graph_resource(&mut mcp);

    let before = read(&mut mcp, &graph_uri);
    let (session_item, epoch, revision) = advertised(&mut mcp, APPLY_EDITS_INTENT);
    assert_eq!(
        before["items"].as_array().unwrap().len(),
        1,
        "only the session item"
    );

    let note = ApplyEditsV1::new(vec![node(7), retitle(7, "decided: servers hold no keys")]);
    let done = tool(
        &mut mcp,
        "mere_invoke",
        json!({
            "projection": graph_uri,
            "instance": session_item,
            "intent": APPLY_EDITS_INTENT,
            "payload": serde_json::to_value(&note).unwrap(),
            "observed_epoch": epoch,
            "observed_revision": revision,
        }),
    );
    assert_eq!(done["result"]["isError"], false, "{done}");
    assert_eq!(done["result"]["structuredContent"]["result"], "accepted");

    let after = read(&mut mcp, &graph_uri);
    assert_eq!(
        after["items"].as_array().unwrap().len(),
        2,
        "the note is a node now"
    );
    assert!(after["revision"].as_u64() > before["revision"].as_u64());
    assert!(
        after.to_string().contains("decided: servers hold no keys"),
        "the retitle is readable back through the resource: {after}"
    );

    // The agent's own undo takes its edit back, and only its own.
    let (item, epoch, revision) = advertised(&mut mcp, UNDO_INTENT);
    let undone = tool(
        &mut mcp,
        "mere_invoke",
        json!({
            "projection": graph_uri,
            "instance": item,
            "intent": UNDO_INTENT,
            "payload": serde_json::to_value(StepV1::default()).unwrap(),
            "observed_epoch": epoch,
            "observed_revision": revision,
        }),
    );
    assert_eq!(undone["result"]["isError"], false, "{undone}");
    assert_eq!(
        read(&mut mcp, &graph_uri)["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // The stored changes name the app the door admitted, never a client claim.
    let mere = fixture.reservoir.meres().await.remove(0);
    let sessions = mere_dir(fixture.root(), persona(41), mere.id).join("sessions");
    let mut changes = String::new();
    for entry in std::fs::read_dir(sessions).unwrap() {
        changes += &std::fs::read_to_string(entry.unwrap().path().join("changes.jsonl")).unwrap();
    }
    assert!(
        changes.contains(&format!("\"via\":\"{AGENT}\"")),
        "{changes}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_edit_is_the_routes_refusal_and_leaves_the_mere_alone() {
    let fixture = fixture(42, "memory").await;
    fixture.grants.grant(
        AppId::new(AGENT),
        ResidentEndpointRoute::new("identity", MERE_NOTICE_POLL).unwrap(),
    );
    let mut mcp = server_for(&fixture, &fixture.route, AGENT, true).await;
    let uri = graph_resource(&mut mcp);
    let (item, epoch, revision) = advertised(&mut mcp, APPLY_EDITS_INTENT);
    let refused = tool(
        &mut mcp,
        "mere_invoke",
        json!({
            "projection": uri, "instance": item, "intent": APPLY_EDITS_INTENT,
            "payload": {"schema": "mere.session.apply/v1", "edits": "not a list"},
            "observed_epoch": epoch, "observed_revision": revision,
        }),
    );
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    assert_eq!(read(&mut mcp, &uri)["items"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_read_only_ambient_route_offers_an_agent_no_way_to_write() {
    let fixture = fixture(43, "memory").await;
    fixture.grants.grant(
        AppId::new(AGENT),
        ResidentEndpointRoute::new("identity", MERE_NOTICE_POLL).unwrap(),
    );
    let mere = fixture.reservoir.meres().await.remove(0);
    fixture
        .routes
        .set_access(
            &fixture.reservoir,
            mere.id,
            AppId::new(AGENT),
            false,
            true,
            AppId::new("turnstone"),
        )
        .await
        .unwrap();
    // Ambient reading needs live material, which an explicit attach creates.
    let mut explicit = server_for(&fixture, &fixture.route, AGENT, true).await;
    let uri = graph_resource(&mut explicit);
    read(&mut explicit, &uri);
    let ambient = ambient_mere_route_id("memory");
    let mut mcp = server_for(&fixture, &ambient, AGENT, true).await;
    let listed = ask(&mut mcp, "resources/list", json!({}));
    assert_eq!(listed["result"]["resources"].as_array().unwrap().len(), 1);
    let actions = tool(&mut mcp, "mere_actions", json!({}));
    let rows = actions["result"]["structuredContent"]["projections"][0]["actions"]
        .as_array()
        .unwrap();
    assert!(
        rows.iter().all(|row| row["intent"] != APPLY_EDITS_INTENT),
        "the ambient route advertises no edit: {rows:?}"
    );
    let refused = tool(
        &mut mcp,
        "mere_invoke",
        json!({
            "projection": listed["result"]["resources"][0]["uri"], "instance": 0,
            "intent": APPLY_EDITS_INTENT,
            "payload": serde_json::to_value(ApplyEditsV1::new(vec![node(9)])).unwrap(),
            "observed_epoch": 0, "observed_revision": 0,
        }),
    );
    assert_eq!(refused["result"]["isError"], true, "{refused}");
}

/// Invoke `intent` on the Notes card with `payload`, at the position the
/// route last advertised.
fn on_notes(server: &mut Server<Admitted>, intent: &str, payload: Value) -> Value {
    let uri = resource_named(server, "Notes");
    let (instance, epoch, revision) = advertised(server, intent);
    tool(
        server,
        "mere_invoke",
        json!({
            "projection": uri, "instance": instance, "intent": intent, "payload": payload,
            "observed_epoch": epoch, "observed_revision": revision,
        }),
    )
}

/// The notes the Notes projection lists now, as their cards.
fn listed_notes(server: &mut Server<Admitted>) -> Vec<Value> {
    let uri = resource_named(server, "Notes");
    let mut items = read(server, &uri)["items"].as_array().unwrap().clone();
    assert_eq!(items[0]["offers"][0]["content"]["title"], "Notes");
    items
        .split_off(1)
        .into_iter()
        .map(|item| item["offers"][0]["content"].clone())
        .collect()
}

fn admitted(fixture: &Fixture) {
    fixture.grants.grant(
        AppId::new(AGENT),
        ResidentEndpointRoute::new("identity", MERE_NOTICE_POLL).unwrap(),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_admitted_agent_appends_a_note_and_finds_it_by_text() {
    let fixture = fixture(44, "memory").await;
    admitted(&fixture);
    let mut mcp = server_for(&fixture, &fixture.route, AGENT, true).await;
    assert!(listed_notes(&mut mcp).is_empty());

    let mut decision = AppendNoteV1::new("decision", "Servers hold no keys");
    decision.body = "The door admits the app id; _custody_ keeps the key.".into();
    decision.evidence = vec![
        "https://merelyllc.com/roadmap/".into(),
        "notes://550e8400-e29b-41d4-a716-446655440000".into(),
    ];
    let todo = AppendNoteV1::new("todo", "Wire the conformance suite");
    for note in [&decision, &todo] {
        let done = on_notes(
            &mut mcp,
            APPEND_NOTE_INTENT,
            serde_json::to_value(note).unwrap(),
        );
        assert_eq!(
            done["result"]["structuredContent"]["result"], "accepted",
            "{done}"
        );
    }
    assert_eq!(
        listed_notes(&mut mcp).len(),
        2,
        "both notes, before any search"
    );

    // Every word must match, case folded, across subject, body and evidence.
    let search = SearchNotesV1::new("CUSTODY roadmap");
    let done = on_notes(
        &mut mcp,
        SEARCH_NOTES_INTENT,
        serde_json::to_value(search).unwrap(),
    );
    assert_eq!(done["result"]["isError"], false, "{done}");
    let found = listed_notes(&mut mcp);
    assert_eq!(found.len(), 1, "{found:?}");
    let note = &found[0];
    assert_eq!(note["title"], "Servers hold no keys");
    let values = note["values"].as_array().unwrap();
    let value = |label: &str| -> Vec<&str> {
        values
            .iter()
            .filter(|row| row["label"] == label)
            .map(|row| row["value"].as_str().unwrap())
            .collect()
    };
    assert_eq!(value("Kind"), ["decision"]);
    assert_eq!(value("Body"), [decision.body.as_str()]);
    assert_eq!(
        value("Evidence"),
        [
            "https://merelyllc.com/roadmap/",
            "notes://550e8400-e29b-41d4-a716-446655440000"
        ]
    );
    let address = value("Note")[0].to_string();
    assert!(address.starts_with("notes://"), "{address}");

    let none = SearchNotesV1::new("keys todo");
    on_notes(
        &mut mcp,
        SEARCH_NOTES_INTENT,
        serde_json::to_value(none).unwrap(),
    );
    assert!(
        listed_notes(&mut mcp).is_empty(),
        "no note holds both words"
    );

    // A note is a node of the session graph, a Knot note any client reads.
    let graph_uri = graph_resource(&mut mcp);
    let graph = read(&mut mcp, &graph_uri);
    assert_eq!(
        graph["items"].as_array().unwrap().len(),
        3,
        "session item and two notes"
    );
    assert!(graph.to_string().contains(&address), "{graph}");

    // Attributed to the persona via the app the door admitted.
    let mere = fixture.reservoir.meres().await.remove(0);
    let sessions = mere_dir(fixture.root(), persona(44), mere.id).join("sessions");
    let mut changes = String::new();
    for entry in std::fs::read_dir(sessions).unwrap() {
        changes += &std::fs::read_to_string(entry.unwrap().path().join("changes.jsonl")).unwrap();
    }
    // The agent minted the session on attach; each note is one edit after it.
    let edits = changes
        .lines()
        .filter(|line| line.contains(&format!("\"via\":\"{AGENT}\"")))
        .filter(|line| line.contains("\"kind\":\"edit\""))
        .count();
    assert_eq!(edits, 2, "{changes}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unadmitted_app_reaches_no_notes() {
    let fixture = fixture(45, "memory").await;
    let refused = fixture
        .catalog
        .update(|catalog| catalog.open(&fixture.route, &context(AGENT)))
        .await;
    assert!(
        refused.is_err(),
        "an app the owner has not admitted opens no mere"
    );

    // Admitted, then denied by the owner: the open door reaches nothing.
    admitted(&fixture);
    let mut mcp = server_for(&fixture, &fixture.route, AGENT, true).await;
    let notes = resource_named(&mut mcp, "Notes");
    let (instance, epoch, revision) = advertised(&mut mcp, APPEND_NOTE_INTENT);
    let mere = fixture.reservoir.meres().await.remove(0);
    fixture
        .routes
        .set_access(
            &fixture.reservoir,
            mere.id,
            AppId::new(AGENT),
            true,
            false,
            AppId::new("turnstone"),
        )
        .await
        .unwrap();
    let note = AppendNoteV1::new("decision", "written after the denial");
    let denied = tool(
        &mut mcp,
        "mere_invoke",
        json!({
            "projection": notes, "instance": instance, "intent": APPEND_NOTE_INTENT,
            "payload": serde_json::to_value(note).unwrap(),
            "observed_epoch": epoch, "observed_revision": revision,
        }),
    );
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let read_back = ask(&mut mcp, "resources/read", json!({ "uri": notes }));
    assert!(read_back.get("result").is_none(), "{read_back}");
    let listed = tool(&mut mcp, "mere_actions", json!({}));
    assert!(
        listed.get("result").is_none() || listed["result"]["isError"] == true,
        "{listed}"
    );

    let mut owner = fixture.attach("turnstone").await;
    assert_eq!(graph_nodes(&mut owner), 0, "nothing was written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_note_is_refused_without_mutating_the_mere() {
    let fixture = fixture(46, "memory").await;
    admitted(&fixture);
    let mut mcp = server_for(&fixture, &fixture.route, AGENT, true).await;
    let graph_uri = graph_resource(&mut mcp);
    let before = read(&mut mcp, &graph_uri);
    let note = |patch: Value| {
        let mut note = serde_json::to_value(AppendNoteV1::new("decision", "a subject")).unwrap();
        for (key, value) in patch.as_object().unwrap() {
            note[key] = value.clone();
        }
        note
    };
    for payload in [
        json!("not a note"),
        note(json!({"schema": "mere.notes.append/v2"})),
        note(json!({"kind": "Decision!"})),
        note(json!({"kind": ""})),
        note(json!({"subject": "   "})),
        note(json!({"subject": "two\nlines"})),
        note(json!({"subject": "x".repeat(201)})),
        note(json!({"body": "x".repeat(64 * 1024 + 1)})),
        note(json!({"evidence": ["has a space"]})),
        note(json!({"evidence": vec!["https://x.test/"; 33]})),
        note(json!({"author": "someone else"})),
    ] {
        let refused = on_notes(&mut mcp, APPEND_NOTE_INTENT, payload.clone());
        assert_eq!(refused["result"]["isError"], true, "{payload}: {refused}");
    }
    let after = read(&mut mcp, &graph_uri);
    assert_eq!(
        after["revision"], before["revision"],
        "no change was written"
    );
    assert_eq!(after["items"], before["items"]);
    assert!(listed_notes(&mut mcp).is_empty());
}
