// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium_rootstock::{A11yOwner, LeafRegistry, OwnedLayout, ProducerRegistry, WindowDom};
use genet_scripted_dom::ScriptedDom;
use layout_dom_api::{LayoutDom, LayoutDomMut};
use std::cell::Cell;
use std::collections::HashMap;

#[test]
fn canvas_member_uuid_reaches_envelopes_without_changing_described_keys() {
    let canvas = sample_canvas();
    let expected = canvas.borrow().describe_items(DESCRIBED_ITEMS);
    assert!(!expected.items.is_empty());
    let semantics = describe_canvas(&canvas.borrow(), Plant::None);
    for (item, child) in expected.items.iter().zip(&semantics.children) {
        assert_eq!(child.key, item.key);
        let source = child.domain_ref.as_ref().expect("canvas member reference");
        assert_eq!(source.adapter, GRAPH_SOURCE);
        assert_eq!(uuid::Uuid::parse_str(&source.id).unwrap(), item.member);
    }
    let mut dom = ScriptedDom::new();
    dom.set_inner_html(
        dom.document(),
        "<custom-leaf key='3' style='display:block;width:1200px;height:800px'></custom-leaf>",
    );
    let dom = WindowDom::document(&dom);
    let layout = OwnedLayout::new(
        &dom,
        &[""],
        WIDTH as f32,
        HEIGHT as f32,
        &[],
        &HashMap::new(),
    );
    let leaves = LeafRegistry::new();
    let mut producers = ProducerRegistry::new();
    producers
        .register(
            SLOT,
            Reader {
                canvas: canvas.clone(),
                plant: Plant::None,
            },
            &[],
        )
        .unwrap();
    let marker = Rc::new(Cell::new(true));
    let projection = cambium_rootstock::project_semantics(&dom, &layout, &leaves, &producers, None);
    let publication = projection
        .publication_envelopes(&A11yOwner::new(&marker), &dom, &leaves, &producers)
        .unwrap();
    assert_eq!(projection.tree.nodes.len(), publication.envelopes.len());
    for item in &expected.items {
        let source = cambium_rootstock::SourceRef::new(GRAPH_SOURCE, item.member.to_string());
        let mut commands = Vec::new();
        let mut structural = 0;
        for (id, envelope) in &publication.envelopes {
            if envelope.domain_ref.as_ref() != Some(&source) {
                continue;
            }
            if let Some(action) = projection.produced_action(id.0) {
                assert_eq!((action.slot, action.key), (SLOT, item.key));
                assert_eq!(envelope.action_ids, [action.id.clone()]);
                commands.push(action.id.clone());
            } else {
                assert!(envelope.action_ids.is_empty());
                structural += 1;
            }
        }
        commands.sort();
        let mut expected_commands: Vec<_> =
            item.actions.iter().map(|a| a.intent.0.clone()).collect();
        expected_commands.sort();
        assert_eq!(commands, expected_commands);
        assert_eq!(structural, 1);
    }
    println!(
        "P2: {} real canvas members retain UUIDs, keys and commands through Rootstock envelopes",
        expected.items.len()
    );
}
