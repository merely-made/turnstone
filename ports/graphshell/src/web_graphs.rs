// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The graphs phase 3's pages draw: the fixture, or a generated one.
//!
//! `?nodes=<n>&seed=<s>` in the page URL asks for a generated graph of `n`
//! nodes, the same for the same seed, so the tree page and `GpuPresenter`'s
//! page draw the same graph when they are timed side by side. Each node links
//! to one earlier node, which keeps the graph connected, and half as many
//! extra links join random pairs. Both pages lay a graph out the same way.
//! `&links=tree` drops the extra links, leaving the spanning tree, and
//! `&links=none` every link: unlinked bodies, where Springs is exclusion and
//! the boundary alone, for receipts that measure repulsion (a linked random
//! graph packs into a ball under Springs, its long edges pulling inward).

use mere::canvas::{Canvas, project_canvas_strategy};
use mere::kernel::geometry::PortablePoint;
use mere::kernel::graph::apply::{add_node, assert_relation};
use mere::kernel::graph::{EdgeAssertion, Graph, SemanticSubKind};
use uuid::Uuid;

/// The arrangement both pages open with.
pub(crate) const LAYOUT: &str = "phyllotaxis.default";

/// The generated graph the page URL asks for, as `(nodes, seed)`.
pub(crate) fn requested() -> Option<(usize, u64)> {
    let search = web_sys::window()?.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    let nodes = params.get("nodes")?.parse().ok()?;
    let seed = params
        .get("seed")
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(1);
    Some((nodes, seed))
}

/// Which of the generated graph's links the page URL keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Links {
    /// The spanning tree and the extra links (the default).
    All,
    /// The spanning tree alone (`links=tree`).
    Tree,
    /// No links (`links=none`).
    None,
}

pub(crate) fn links() -> Links {
    let requested = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .and_then(|search| web_sys::UrlSearchParams::new_with_str(&search).ok())
        .and_then(|params| params.get("links"));
    match requested.as_deref() {
        Some("tree") => Links::Tree,
        Some("none") => Links::None,
        _ => Links::All,
    }
}

/// A connected graph of `nodes` nodes, the same for the same `seed`, keeping
/// the links `links` names.
pub(crate) fn generated(nodes: usize, seed: u64, links: Links) -> Graph {
    let mut graph = Graph::new();
    // xorshift64: small, and the same on every platform.
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let keys: Vec<_> = (0..nodes)
        .map(|index| {
            // A browser has no random ids, so each is derived from the seed.
            let id = Uuid::from_u128((u128::from(seed) << 64) | index as u128);
            add_node(
                &mut graph,
                Some(id),
                format!("https://node-{index}.generated.test/"),
                PortablePoint::new(0.0, 0.0),
            )
        })
        .collect();
    let link = || EdgeAssertion::Semantic {
        sub_kind: SemanticSubKind::Hyperlink,
        label: None,
        decay_progress: None,
    };
    for index in 1..nodes {
        let earlier = (next() % index as u64) as usize;
        if links != Links::None {
            assert_relation(&mut graph, keys[index], keys[earlier], link());
        }
    }
    if links == Links::All && nodes > 1 {
        for _ in 0..nodes / 2 {
            let from = (next() % nodes as u64) as usize;
            let to = (next() % nodes as u64) as usize;
            if from != to {
                assert_relation(&mut graph, keys[from], keys[to], link());
            }
        }
    }
    graph
}

/// A canvas holding `graph`, arranged and fitted to `width` by `height`.
pub(crate) fn prepared_canvas(graph: Graph, width: u32, height: u32) -> Canvas {
    let mut canvas = Canvas::with_graph(graph);
    canvas.resize(width, height);
    canvas.set_layout_strategy(Some(LAYOUT.to_string()));
    // The canvas passes its own registry (dynamics grammar plan, F87).
    let (registry, graph) = canvas.registry_and_graph();
    let positions = project_canvas_strategy(
        registry, LAYOUT, graph, None, width, height, None, None, true,
    );
    canvas.apply_strategy_positions(&positions);
    canvas.fit_to_content();
    canvas
}
