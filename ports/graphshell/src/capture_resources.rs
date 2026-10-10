// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use eidetic::{CaptureContent, PageTextStore, ResourceCaptureStore};
use mere::kernel::graph::Graph;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use muniment::MemoryBackend;

#[test]
fn page_versions_follow_resources_through_aliases_navigation_and_reopen() {
    pollster::block_on(async {
        let backend = MemoryBackend::new();
        let text = PageTextStore::new(&backend);
        let mut graph = Graph::new();
        let first = add_node(
            &mut graph,
            None,
            "https://Example.test/page?utm_source=x#top".into(),
            Default::default(),
        );
        let alias = add_node(
            &mut graph,
            None,
            "https://example.test/page".into(),
            Default::default(),
        );
        let original = graph.shown_resource_id(first).unwrap();
        assert_eq!(graph.shown_resource_id(alias), Some(original));
        let first_hash = text
            .put("https://example.test/page", "first version", 1)
            .await
            .unwrap();
        let second_hash = text
            .put("https://example.test/page#new", "second version", 2)
            .await
            .unwrap();
        assert_ne!(first_hash, second_hash);
        apply_graph_delta(
            &mut graph,
            GraphDelta::NavigateNode {
                key: first,
                url: "https://example.test/next".into(),
            },
        );
        let next = graph.shown_resource_id(first).unwrap();
        assert_ne!(next, original);
        text.put("https://example.test/next", "next page", 3)
            .await
            .unwrap();
        let reopened = ResourceCaptureStore::new(&backend);
        let original_versions = reopened.for_resource(&original.to_string()).await.unwrap();
        assert_eq!(original_versions.len(), 2);
        assert!(
            original_versions
                .iter()
                .all(|version| version.content == CaptureContent::PageText)
        );
        assert_eq!(
            original_versions[0].content_hash.to_hex(),
            first_hash.to_hex()
        );
        assert_eq!(
            original_versions[1].content_hash.to_hex(),
            second_hash.to_hex()
        );
        assert_eq!(
            reopened
                .for_resource(&next.to_string())
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(graph.shown_resource_id(alias), Some(original));
        assert!(
            reopened
                .for_url("https://absent.test")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            text.text_for("https://example.test/page")
                .await
                .unwrap()
                .as_deref(),
            Some("second version")
        );
    });
}
