// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::io::Read;

use super::*;

const GOLDEN: &str = include_str!("../../../../../tests/fixtures/uxtree-snapshot.json");
const URI: &str = "uxtree://snapshot";

#[test]
fn uxtree_resource_returns_the_product_golden_and_keeps_the_tool_list() {
    for writes in [false, true] {
        for legacy in [false, true] {
            let config = Config {
                writes,
                ..Config::default()
            };
            let mut baseline = Server::new(Route::new(&["first"]), config.clone());
            let mut attached = Server::new(Route::new(&["first"]), config)
                .with_uxtree_snapshot(GOLDEN.as_bytes())
                .unwrap();
            let request = |method, params| {
                if legacy {
                    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                        .to_string()
                } else {
                    modern(1, method, params)
                }
            };
            if legacy {
                for server in [&mut baseline, &mut attached] {
                    let reply = call(
                        server,
                        request("initialize", json!({"protocolVersion":"2025-06-18"})),
                    );
                    assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
                }
            }

            let tools = request("tools/list", json!({}));
            assert_eq!(
                call(&mut baseline, tools.clone()),
                call(&mut attached, tools)
            );

            let list = request("resources/list", json!({}));
            let original = call(&mut baseline, list.clone());
            let with_snapshot = call(&mut attached, list);
            let original = original["result"]["resources"].as_array().unwrap();
            let resources = with_snapshot["result"]["resources"].as_array().unwrap();
            assert_eq!(&resources[..original.len()], original);
            assert_eq!(resources.len(), original.len() + 1);
            assert_eq!(resources.last().unwrap()["uri"], URI);

            let read = request("resources/read", json!({"uri":URI}));
            assert!(call(&mut baseline, read.clone())["error"].is_object());
            let reply = call(&mut attached, read);
            let content = &reply["result"]["contents"][0];
            assert_eq!(content["mimeType"], "application/json");
            let text = content["text"].as_str().unwrap();
            assert_eq!(text, GOLDEN);
            assert!(!text.contains("P10-PLANTED-PRIVATE-TEXT"));
            let document: Value = serde_json::from_str(text).unwrap();
            assert_eq!(document["trees"][0]["nodes"][2]["text_redacted"], true);
            assert_eq!(document["trees"][0]["nodes"][2]["text"], Value::Null);
            assert_eq!(document["trees"][1]["nodes"][0]["text_redacted"], true);
        }
    }
}

#[test]
fn uxtree_resource_refuses_malformed_unrelated_and_oversized_inputs() {
    for input in [
        "{",
        r#"{"trees":[]}"#,
        r#"{"kind":"turnstone-uxtree-snapshot"}"#,
    ] {
        assert!(
            server(Route::new(&[]))
                .with_uxtree_snapshot(input.as_bytes())
                .is_err()
        );
    }
    let padding = std::io::repeat(b' ').take(8 * 1024 * 1024);
    let oversized = GOLDEN.as_bytes().chain(padding);
    assert!(
        server(Route::new(&[]))
            .with_uxtree_snapshot(oversized)
            .is_err()
    );
}
