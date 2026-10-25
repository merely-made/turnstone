// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The server against a route that behaves like djinn's: it answers `Open`,
//! refuses what the owner has not granted, and checks the revision a caller
//! observed. Messages are the real JSON-RPC a harness sends.

use std::io::Cursor;

mod common;

use common::Route;
use graphshell_mcp::{Config, Server};
use serde_json::{Value, json};

fn server(route: Route) -> Server<Route> {
    Server::new(
        route,
        Config {
            name: "mere-mcp".into(),
            scope: "mock".into(),
            ..Config::default()
        },
    )
}

fn modern(id: u64, method: &str, params: Value) -> String {
    let mut params = params;
    params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28"});
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn call(server: &mut Server<Route>, line: String) -> Value {
    serde_json::from_str(&server.handle(&line).expect("a reply")).unwrap()
}

fn tool(server: &mut Server<Route>, name: &str, arguments: Value) -> Value {
    call(
        server,
        modern(
            9,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        ),
    )
}

fn structured(reply: &Value) -> &Value {
    &reply["result"]["structuredContent"]
}

#[test]
fn discover_names_both_eras_and_the_tools_and_resources() {
    let mut mcp = server(Route::new(&[]));
    let reply = call(&mut mcp, modern(1, "server/discover", json!({})));
    assert_eq!(reply["result"]["resultType"], "complete");
    assert_eq!(reply["result"]["supportedVersions"][0], "2026-07-28");
    assert!(
        reply["result"]["supportedVersions"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    assert_eq!(
        reply["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "mere-mcp"
    );
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    assert!(reply["result"]["capabilities"]["resources"].is_object());
}

#[test]
fn an_unknown_protocol_version_is_refused_with_what_is_supported() {
    let mut mcp = server(Route::new(&[]));
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list",
        "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": "1900-01-01"}}})
    .to_string();
    let reply = call(&mut mcp, line);
    assert_eq!(reply["error"]["code"], -32022);
    assert_eq!(reply["error"]["data"]["requested"], "1900-01-01");
    assert_eq!(reply["error"]["data"]["supported"][0], "2026-07-28");
}

#[test]
fn a_handshake_client_initializes_then_calls_without_per_request_metadata() {
    let mut mcp = server(Route::new(&["first"]));
    let bare = json!({"jsonrpc": "2.0", "id": 1, "method": "resources/list"}).to_string();
    assert!(
        call(&mut mcp, bare.clone())["error"].is_object(),
        "no handshake yet, no version"
    );

    let init = json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "claude-code", "version": "1"}}})
    .to_string();
    let reply = call(&mut mcp, init);
    assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
    assert!(
        reply["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("mere_invoke")
    );
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string();
    assert!(
        mcp.handle(&initialized).is_none(),
        "a notification gets no reply"
    );

    let reply = call(&mut mcp, bare);
    assert_eq!(reply["result"]["resources"][0]["uri"], "mere://mock/Notes");
    assert!(
        reply["result"].get("resultType").is_none(),
        "legacy results carry no resultType"
    );
}

#[test]
fn a_projection_reads_as_instances_with_their_disclosed_cards() {
    let mut mcp = server(Route::new(&["remember the pins", "receipts live here"]));
    let listed = call(&mut mcp, modern(1, "resources/list", json!({})));
    let uri = listed["result"]["resources"][0]["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let read = call(&mut mcp, modern(2, "resources/read", json!({"uri": uri})));
    let text = read["result"]["contents"][0]["text"].as_str().unwrap();
    let document: Value = serde_json::from_str(text).unwrap();
    assert_eq!(document["epoch"], 3);
    assert_eq!(document["revision"], 1);
    assert_eq!(document["items"].as_array().unwrap().len(), 2);
    assert_eq!(document["items"][1]["source"]["id"], "note-1");
    assert_eq!(
        document["items"][1]["offers"][0]["content"]["title"],
        "receipts live here"
    );
    assert_eq!(
        document["items"][0]["offers"][0]["semantics"]["actions"][0]["intent"],
        "notes.append"
    );
    assert_eq!(
        mcp_opens(&mcp),
        1,
        "the session is opened once and its descriptor kept"
    );
}

fn mcp_opens(server: &Server<Route>) -> u32 {
    server.carrier().seen_open
}

#[test]
fn an_unknown_resource_is_not_found_and_not_an_empty_document() {
    let mut mcp = server(Route::new(&[]));
    let reply = call(
        &mut mcp,
        modern(1, "resources/read", json!({"uri": "mere://mock/Nowhere"})),
    );
    assert_eq!(reply["error"]["code"], -32602);
    assert_eq!(reply["error"]["data"]["uri"], "mere://mock/Nowhere");
}

#[test]
fn actions_carry_the_revision_a_write_must_observe_and_a_write_lands() {
    let mut mcp = server(Route::new(&["seed"]));
    let listed = tool(&mut mcp, "mere_actions", json!({}));
    let projection = &structured(&listed)["projections"][0];
    assert_eq!(
        projection["actions"][0]["payload_schema"],
        "notes.append/v1"
    );
    let (epoch, revision) = (projection["epoch"].clone(), projection["revision"].clone());

    let done = tool(
        &mut mcp,
        "mere_invoke",
        json!({"projection": "Notes", "instance": 0, "intent": "notes.append",
               "payload": "decided: servers hold no keys", "observed_epoch": epoch,
               "observed_revision": revision}),
    );
    assert_eq!(done["result"]["isError"], false);
    assert_eq!(structured(&done)["result"], "accepted");

    let again = tool(
        &mut mcp,
        "mere_actions",
        json!({"projection": "mere://mock/Notes"}),
    );
    assert_eq!(structured(&again)["projections"][0]["revision"], 2);
    let read = call(
        &mut mcp,
        modern(3, "resources/read", json!({"uri": "mere://mock/Notes"})),
    );
    assert!(
        read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("servers hold no keys")
    );
}

#[test]
fn a_structured_payload_is_sent_as_its_json_text() {
    let mut mcp = server(Route::new(&["seed"]));
    tool(
        &mut mcp,
        "mere_invoke",
        json!({"projection": "Notes", "instance": 0, "intent": "notes.append",
               "payload": {"schema": "notes.append/v1"}, "observed_epoch": 3, "observed_revision": 1}),
    );
    let read = call(
        &mut mcp,
        modern(3, "resources/read", json!({"uri": "mere://mock/Notes"})),
    );
    assert!(
        read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("notes.append/v1")
    );
}

#[test]
fn a_stale_write_is_refused_with_the_current_revision() {
    let mut mcp = server(Route::new(&["seed"]));
    let args = |revision| {
        json!({"projection": "Notes", "instance": 0, "intent": "notes.append",
               "payload": "x", "observed_epoch": 3, "observed_revision": revision})
    };
    tool(&mut mcp, "mere_invoke", args(1));
    let stale = tool(&mut mcp, "mere_invoke", args(1));
    assert_eq!(stale["result"]["isError"], true);
    assert_eq!(structured(&stale)["result"], "stale");
    assert_eq!(structured(&stale)["current_revision"], 2);
}

#[test]
fn what_the_route_rejects_or_does_not_grant_reaches_the_model_as_a_tool_error() {
    let mut mcp = server(Route::new(&["seed"]));
    let unknown = tool(
        &mut mcp,
        "mere_invoke",
        json!({"projection": "Notes", "instance": 0, "intent": "notes.erase",
               "observed_epoch": 3, "observed_revision": 1}),
    );
    assert_eq!(unknown["result"]["isError"], true);
    assert_eq!(structured(&unknown)["result"], "rejected");

    let mut denied = Route::new(&["seed"]);
    denied.granted = false;
    let mut mcp = server(denied);
    let refused = tool(
        &mut mcp,
        "mere_invoke",
        json!({"projection": "Notes", "instance": 0, "intent": "notes.append",
               "payload": "x", "observed_epoch": 3, "observed_revision": 1}),
    );
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        structured(&refused)["error"]
            .as_str()
            .unwrap()
            .contains("no write grant")
    );
    assert!(!mcp.finished(), "a refusal leaves the session intact");
}

#[test]
fn malformed_arguments_are_tool_errors_the_model_can_correct() {
    let mut mcp = server(Route::new(&["seed"]));
    let reply = tool(
        &mut mcp,
        "mere_invoke",
        json!({"projection": "Notes", "instance": -1}),
    );
    assert_eq!(reply["result"]["isError"], true);
}

#[test]
fn read_only_offers_no_write_tool_and_refuses_it_by_name() {
    let config = Config {
        scope: "mock".into(),
        writes: false,
        ..Config::default()
    };
    let mut mcp = Server::new(Route::new(&["seed"]), config);
    let listed = call(&mut mcp, modern(1, "tools/list", json!({})));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["mere_actions"]);
    let reply = tool(&mut mcp, "mere_invoke", json!({}));
    assert_eq!(reply["error"]["code"], -32602);
}

#[test]
fn a_write_enabled_server_lists_both_tools_in_a_stable_order() {
    let mut mcp = server(Route::new(&[]));
    let names = |mcp: &mut Server<Route>| {
        let listed = call(mcp, modern(1, "tools/list", json!({})));
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&mut mcp), ["mere_actions", "mere_invoke"]);
    assert_eq!(
        names(&mut mcp),
        names(&mut server(Route::new(&["different"])))
    );
}

#[test]
fn a_disconnect_ends_serving_so_the_client_can_restart_the_process() {
    let mut route = Route::new(&["seed"]);
    route.gone = true;
    let mut mcp = server(route);
    let input = format!(
        "{}\n{}\n",
        modern(1, "resources/list", json!({})),
        modern(2, "tools/list", json!({}))
    );
    let mut out = Vec::new();
    mcp.serve(Cursor::new(input), &mut out).unwrap();
    assert!(mcp.finished());
    let lines: Vec<&str> = std::str::from_utf8(&out).unwrap().lines().collect();
    assert_eq!(lines.len(), 1, "serving stopped after the failed request");
    let reply: Value = serde_json::from_str(lines[0]).unwrap();
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("went away")
    );
}

#[test]
fn stdout_carries_one_json_message_per_request_and_nothing_for_notifications() {
    let mut mcp = server(Route::new(&["seed"]));
    let input = [
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}})
            .to_string(),
        String::new(),
        "not json".to_string(),
        modern(2, "tools/list", json!({})),
    ]
    .join("\n");
    let mut out = Vec::new();
    mcp.serve(Cursor::new(input), &mut out).unwrap();
    let lines: Vec<Value> = std::str::from_utf8(&out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("every line is JSON"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["id"], 2);
}
