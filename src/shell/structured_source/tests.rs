// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use fleece::JsonLdParseStatus;
use layout_dom_api::LayoutDom;

use super::*;
use crate::contributed_surface::SurfaceProviderRegistry;
use crate::panes::GraphId;
use crate::saved_sources::Key;

fn block(order: u64, text: &str) -> EmbeddedJsonLdBlock {
    EmbeddedJsonLdBlock {
        document_order: order,
        element_id: Some("evidence".into()),
        declared_type: "application/ld+json".into(),
        dom_text: text.into(),
        parse: JsonLdParseStatus::InvalidJson,
    }
}

#[test]
fn only_an_exact_preorder_match_reveals_a_block() {
    let origin = "https://example.test/response";
    let capture = "blake3:example";
    let first = block(2, "first");
    let second = block(4, "second");
    assert_eq!(
        Block::at(origin, capture, &[first.clone(), second.clone()], 4)
            .unwrap()
            .dom_text,
        "second"
    );
    assert!(Block::at(origin, capture, &[first.clone(), second], 3).is_err());
    assert!(Block::at(origin, capture, &[first.clone(), first], 2).is_err());
}

#[test]
fn the_reader_renders_literal_text_and_rejects_another_session() {
    let reader = Reader::default();
    let session = SessionId::new();
    let source = Reader::source(session);
    let text = "{\"description\":\"<a href='https://example.test'>link</a>\"}\n";
    let block = Block::at(
        "https://example.test/response",
        "blake3:example",
        &[block(4, text)],
        4,
    )
    .unwrap();
    *reader.current.borrow_mut() = Some(OpenBlock {
        admission: KeptSource {
            key: Key {
                session,
                graph: GraphId::new(),
                node: Uuid::new_v4(),
                annotation_manifest: "blake3:annotation".into(),
            },
            request: Uuid::new_v4(),
            capture_hash: "blake3:example".into(),
        },
        generation: Uuid::new_v4(),
        block: Rc::new(block),
    });
    let kind = PaneKindId::new(PANE_KIND);
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(Provider::new(reader.clone()))
        .unwrap();
    let pane = registry.admit(&kind, &source).unwrap();
    assert!(pane.availability().is_available());
    let dom = pane.dom_ref();
    let pre = dom.all_with_class(dom.document(), "source-block-text")[0];
    let children: Vec<_> = dom.dom_children(pre).collect();
    assert_eq!(children.len(), 1);
    assert_eq!(dom.text(children[0]), Some(text));

    let other = registry
        .admit(&kind, &Reader::source(SessionId::new()))
        .unwrap();
    assert!(!other.availability().is_available());
    reader.current.borrow_mut().take();
    let revoked = registry.admit(&kind, &source).unwrap();
    assert!(!revoked.availability().is_available());
}
