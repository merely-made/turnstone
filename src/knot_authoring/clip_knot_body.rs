// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The receiving Knot endpoint owns provenance and records it from the typed
/// clip intent. Inserting semantic blocks alone avoids a second document header
/// in an existing note. Formatting is an authoring concern of this consumer.
pub(super) fn clip_knot_body(fragment: &fleece::clip::ClipFragment) -> String {
    let mut body = String::new();
    inker::EngineDocument {
        address: fragment.source_url.clone(),
        title: None,
        content_type: "text/x-knot".into(),
        lang: None,
        provenance: inker::DocumentProvenance::default(),
        trust: inker::DocumentTrustState::Unknown,
        diagnostics: Vec::new(),
        navigation: Default::default(),
        blocks: fleece::clip::fragment_blocks(fragment),
    }
    .write_knot_body(&mut body);
    if fragment.source_url.starts_with("mere://saved-source/") {
        let citation = inker::EngineDocument {
            address: fragment.source_url.clone(),
            title: None,
            content_type: "text/x-knot".into(),
            lang: None,
            provenance: Default::default(),
            trust: inker::DocumentTrustState::Unknown,
            diagnostics: vec![],
            navigation: Default::default(),
            blocks: vec![inker::Block::Paragraph {
                spans: vec![inker::InlineSpan::Link {
                    url: fragment.source_url.clone(),
                    title: None,
                    spans: vec![inker::InlineSpan::Text(format!(
                        "Read saved passage: {}",
                        fragment.title.as_deref().unwrap_or("Saved source")
                    ))],
                    predicate: None,
                }],
            }],
        };
        citation.write_knot_body(&mut body);
    }
    body
}
