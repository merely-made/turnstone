// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn key(node: uuid::Uuid, annotation: &str) -> Key {
    Key {
        session: crate::panes::SessionId::new(),
        graph: crate::panes::GraphId::new(),
        node,
        annotation_manifest: annotation.into(),
    }
}

fn entry(key: Key, occurrence: usize, text: &str, title: &str) -> Entry {
    Entry {
        admission: Admission {
            key,
            occurrence,
            title: title.into(),
            url: format!("https://saved.test/{title}"),
            error: None,
        },
        content: Some(Content {
            title: title.into(),
            origin: format!("https://saved.test/{title}"),
            captured_at_ms: 42,
            text: text.into(),
            text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
            raw_manifest: "raw".into(),
        }),
        error: None,
    }
}

#[test]
fn sealed_address_round_trips_without_legacy_namespace_alias() {
    let node = uuid::Uuid::new_v4();
    let annotation = eidetic::Hash::of(b"sealed annotation").to_string();
    let target = Target {
        key: key(node, &format!("sealed-v1:{annotation}")),
        text_hash: eidetic::Hash::of(b"private text").to_string(),
        selector: Some(Selector { start: 1, end: 3 }),
    };
    let address = address(&target).unwrap();
    assert!(address.starts_with("mere://saved-source/sealed-v1/"));
    assert_eq!(parse_address(&address).unwrap(), Some(target.clone()));
    let legacy = address.replace("/sealed-v1/", "/");
    assert_ne!(parse_address(&legacy).unwrap(), Some(target));
}

#[test]
fn internal_address_round_trips_and_rejects_noncanonical_routes() {
    let target = Target {
        key: key(
            uuid::Uuid::new_v4(),
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        text_hash: eidetic::Hash::of("hello 🐦".as_bytes()).to_string(),
        selector: Some(Selector { start: 6, end: 7 }),
    };
    let value = address(&target).unwrap();
    assert_eq!(parse_address(&value).unwrap(), Some(target));
    assert_eq!(parse_address("https://example.test").unwrap(), None);
    assert!(parse_address(&value.replace("start=6", "start=6&extra=1")).is_err());
    assert!(parse_address(&value.replace("start=6", "start=06")).is_err());
}

#[test]
fn document_keeps_original_address_as_an_explicit_link() {
    let entry = entry(
        key(
            uuid::Uuid::new_v4(),
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        0,
        "the full retained body",
        "Capture title",
    );
    let content = entry.content.unwrap();
    let target = Target {
        key: entry.admission.key,
        text_hash: content.text_hash.clone(),
        selector: Some(Selector { start: 4, end: 8 }),
    };
    let rendered = document(&content, &target).unwrap();
    assert_eq!(rendered.address, address(&target).unwrap());
    assert!(rendered.blocks.iter().any(|block| matches!(
            block,
            inker::Block::Paragraph { spans }
                if spans.iter().any(|span| matches!(span, inker::InlineSpan::Link { url, .. } if url == &content.origin))
        )));
}
#[test]
fn long_reader_body_and_quote_are_bounded_without_truncating_canonical_unicode() {
    let text = "naïve — 東京 🐦 \\ ".repeat(10000);
    let entry = entry(
        key(
            uuid::Uuid::new_v4(),
            &eidetic::ManifestId::of_blob(b"large").to_string(),
        ),
        0,
        &text,
        "Long source",
    );
    let content = entry.content.unwrap();
    let target = Target {
        key: entry.admission.key,
        text_hash: content.text_hash.clone(),
        selector: Some(Selector {
            start: 0,
            end: text.chars().count(),
        }),
    };
    let doc = document(&content, &target).unwrap();
    let inker::Block::Quote {
        blocks: quote_blocks,
    } = &doc.blocks[4]
    else {
        panic!("selected quotation")
    };
    let quote: Vec<&str> = quote_blocks
        .iter()
        .map(|block| {
            let inker::Block::Paragraph { spans } = block else {
                panic!("quote paragraph")
            };
            let inker::InlineSpan::Strong(spans) = &spans[0] else {
                panic!("highlighted quote")
            };
            let inker::InlineSpan::Text(text) = &spans[0] else {
                panic!("exact quote text")
            };
            text.as_str()
        })
        .collect();
    let body: Vec<&str> = doc.blocks[5..doc.blocks.len() - 1]
        .iter()
        .map(|block| {
            let inker::Block::Paragraph { spans } = block else {
                panic!("body paragraph")
            };
            let inker::InlineSpan::Text(text) = &spans[0] else {
                panic!("exact body text")
            };
            text.as_str()
        })
        .collect();
    assert!(quote.len() > 1 && body.len() > 1);
    assert!(
        quote
            .iter()
            .chain(&body)
            .all(|part| part.len() <= DISPLAY_BLOCK_BYTES)
    );
    assert_eq!(quote.concat(), text);
    assert_eq!(body.concat(), text);
    assert_eq!(doc.address, address(&target).unwrap());
    assert_eq!(
        eidetic::Hash::of(body.concat().as_bytes()).to_string(),
        target.text_hash
    );
    let unbroken = "é🐦".repeat(10000);
    let parts = display_parts(&unbroken);
    assert!(parts.iter().all(|part| part.len() <= DISPLAY_BLOCK_BYTES));
    assert_eq!(parts.concat(), unbroken);
}
