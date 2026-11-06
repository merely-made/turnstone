// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit synthetic data for the goal surface's tests and headed proof:
//! page text admitted through the real `PageTextStore`, titles and links in a
//! real graph. Nothing here is a person's content.

use super::RetainedPages;
use eidetic::PageTextStore;
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta, assert_relation};
use mere::kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
use muniment::MemoryBackend;

pub const OWNER: &str = "graphshell-goal-fixture";
pub const INDEX: &str = "https://kitchen.example/bread-index";
pub const STARTER: &str = "https://kitchen.example/starter-care";
pub const TRAVEL: &str = "https://kitchen.example/travel-baking";
pub const OVEN: &str = "https://kitchen.example/oven-spring";
pub const FLOUR: &str = "https://kitchen.example/flour-types";
/// Linked from the index but never retained.
pub const HOTEL: &str = "https://elsewhere.example/hotel-kitchens";
/// A goal that shares no term with any fixture page; only a model can place it.
pub const GOAL_PARAPHRASE: &str = "preserve my sourdough while I am on holiday";
pub const STARTER_V1: &str = "Starter care. Feed the starter every day with equal parts flour and water and leave it somewhere warm.";

const PAGES: [(&str, &str, &str); 5] = [
    (INDEX, "Bread notes index", "Bread notes index. Pages on culture care, trips away, oven spring and flour types. Start here when planning a bake."),
    (STARTER, "Starter care", "Starter care. Feed the starter every day with equal parts flour and water. To keep a starter alive while you are away, refrigerate it and feed it once a week; a starter survives a month in the cold. Discard half before each feeding."),
    (TRAVEL, "Baking while travelling", "Baking while travelling. For a trip longer than a month, dry a thin smear of starter into flakes and carry the flakes in a sealed bag. At your destination rehydrate the flakes with warm water and flour; the starter wakes in two days. Hotel kitchens rarely have ovens."),
    (OVEN, "Oven spring", "Oven spring. Steam in the first ten minutes lets the loaf expand. A heavy lidded pot works as well as a baker's oven. Score the dough just before baking."),
    (FLOUR, "Flour types", "Flour types. Whole wheat ferments quickly; rye feeds a starter fastest. Bread flour holds more water than all purpose flour."),
];

/// A graph with the pages as surfaces and `Cites` relations between them,
/// beside a store holding their text. `STARTER` is captured twice, so it has
/// an earlier version.
pub async fn backend_and_graph() -> (MemoryBackend, Graph) {
    let backend = MemoryBackend::new();
    let store = PageTextStore::new(backend.clone());
    store.put(STARTER, STARTER_V1, 1).await.expect("earlier version");
    for (i, (url, _, text)) in PAGES.iter().enumerate() {
        store.put(url, text, 10 + i as u64).await.expect("page text");
    }
    let mut graph = Graph::new();
    let mut key = |url: &str, title: &str| {
        let key = add_node(&mut graph, None, url.into(), Default::default());
        apply_graph_delta(&mut graph, GraphDelta::SetNodeTitle { key, title: title.into() });
        key
    };
    let keys: Vec<_> = PAGES.iter().map(|(url, title, _)| key(url, title)).collect();
    let hotel = key(HOTEL, "Hotel kitchens");
    let cite = |graph: &mut Graph, from, to, label: &str| {
        assert_relation(
            graph,
            from,
            to,
            EdgeAssertion::Semantic {
                sub_kind: SemanticSubKind::Cites,
                label: Some(label.into()),
            },
        )
        .expect("fixture endpoints");
    };
    let [index, starter, travel, oven, flour] = [keys[0], keys[1], keys[2], keys[3], keys[4]];
    cite(&mut graph, index, oven, "baking");
    cite(&mut graph, index, flour, "ingredients");
    cite(&mut graph, index, hotel, "places to bake");
    cite(&mut graph, index, travel, "trips");
    cite(&mut graph, index, starter, "storage");
    cite(&mut graph, travel, starter, "rehydrate");
    cite(&mut graph, flour, starter, "feeding");
    (backend, graph)
}

pub async fn pages() -> RetainedPages {
    let (backend, graph) = backend_and_graph().await;
    RetainedPages::load(OWNER, &backend, &graph, 1 << 20).await.expect("fixture load")
}
