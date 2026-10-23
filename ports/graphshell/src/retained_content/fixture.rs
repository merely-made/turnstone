// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Explicit synthetic acceptance data, admitted through the actual owners.
use super::*;
pub async fn seed<B: muniment::Backend + Clone>(
    app: &mut crate::app::GraphshellApp<B>,
    limits: mere_document_lanes::retained_clip::ClipLimits,
) -> Result<(), String> {
    let context = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: Some("source-app".into()),
            entity: "selected".into(),
            revision: None,
        },
        capture_id: "retained-content-acceptance".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("explicit selected fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(7),
    };
    let mut source = eidetic::MemoryBackend::new();
    let text = serde_json::from_str(include_str!(
        "../../../../crates/fleece/tests/fixtures/portable_clip/knot.json"
    ))
    .map_err(|e| e.to_string())?;
    let text_id = fleece::portable_clip::retain_knot_export(
        &mut source,
        None,
        &text,
        Some("private prefix; café; private suffix".as_bytes().to_vec()),
        &context,
        limits,
    )
    .await
    .map_err(|e| e.to_string())?;
    let voice = serde_json::from_str(include_str!(
        "../../../../crates/fleece/tests/fixtures/portable_clip/redshank.json"
    ))
    .map_err(|e| e.to_string())?;
    let voice_id = fleece::portable_clip::retain_redshank_export(
        &mut source,
        None,
        &voice,
        Some(
            include_bytes!("../../../../crates/fleece/tests/fixtures/portable_clip/voice.wav")
                .to_vec(),
        ),
        &context,
        limits,
    )
    .await
    .map_err(|e| e.to_string())?;
    for id in [text_id, voice_id] {
        let package = fleece::portable_clip::export_selected_clip(
            &mut source,
            None,
            id,
            &["body".into()],
            limits,
        )
        .await
        .map_err(|e| e.to_string())?;
        let wire = serde_json::to_vec(&package).map_err(|e| e.to_string())?;
        let member = import_clip(app, &wire, limits, &context).await?;
        crate::local_intake::persist_intake(app, member, 7).await?;
    }
    let member = crate::local_intake::create_address(
        app,
        "https://retained.example/notes",
        "Native held HTML",
    )?;
    let key = app
        .host
        .graph()
        .get_node_key_by_id(member)
        .ok_or("Missing fixture member")?;
    app.host.mutate_product_graph(|graph| {
        use mere::kernel::graph::apply::{apply_graph_delta, GraphDelta};
        apply_graph_delta(graph, GraphDelta::SetNodeMimeHint { key, mime_hint: Some("text/html".into()) });
        apply_graph_delta(graph, GraphDelta::SetNodeBody { key, body: Some("<!doctype html><html><head><style>body{background:#faf8ef;color:#18292a;font-family:sans-serif;padding:18px}h1{color:#126975}a{color:#165bb0}</style></head><body><h1>Native retained content</h1><p>This saved HTML is rendered by Genet inside Graphshell's Pelt tile.</p><a href='https://retained.example/notes'>Held local link</a><p>Source bytes stay in this Mere. Browsing this view issues no page fetch.</p></body></html>".into()) });
    }).map_err(|e| e.to_string())?;
    crate::local_intake::persist_intake(app, member, 7).await?;
    Ok(())
}

/// Opt-in reading fixture. It uses the ordinary normalized-document lane and
/// persisted graph intake, so captures exercise the real portable text layout.
pub async fn seed_reading<B: muniment::Backend + Clone>(
    app: &mut crate::app::GraphshellApp<B>,
) -> Result<(), String> {
    use inker::{Block, EngineDocument, InlineSpan};
    let address = "mere://reading/field-notes";
    let paragraph = |text: &str| Block::Paragraph {
        spans: vec![InlineSpan::Text(text.into())],
    };
    let mut document = EngineDocument {
        address: address.into(),
        title: Some("Reading from the graph".into()),
        content_type: "text/plain".into(),
        lang: Some("en".into()),
        provenance: Default::default(),
        trust: Default::default(),
        diagnostics: Vec::new(),
        navigation: Default::default(),
        blocks: vec![
            Block::Heading { level: 1, spans: vec![InlineSpan::Text("Reading from the graph".into())] },
            paragraph("A useful reading space keeps the text close enough to follow and leaves room for a thought to settle. The document remains part of the graph: its sources, connections and saved places are available without crowding the page."),
            paragraph("A Map and a Read are different projections of the same material. Moving between them changes the presentation, while the document keeps its identity. An Inspector can follow the current view or stay pinned to a source that is worth keeping nearby."),
            Block::Heading { level: 2, spans: vec![InlineSpan::Text("Room to think".into())] },
            paragraph("Long lines make it easy to lose the next sentence. A measured column and a little space between lines give the eye a reliable place to return. On a narrow window the column follows the available space; the text stays readable without a second horizontal journey."),
            Block::CodeBlock { language: None, text: "document → reading projection\n         → map projection\n         → related sources".into() },
            Block::Paragraph { spans: vec![InlineSpan::Link { url: "https://retained.example/notes".into(), title: None, spans: vec![InlineSpan::Text("Visit the related field notes".into())], predicate: None }] },
            paragraph("The surrounding controls use a smaller interface scale. Their labels remain clear, but their spacing gives the page priority. This local document is a deterministic layout sample; it does not fetch remote content."),
        ],
    };
    document.provenance.source_kind = Some(pelt_core::native::DOCUMENT_ENGINE_ID.into());
    for index in 1..=8 {
        document.blocks.push(paragraph(&format!("Field observation {index}. A saved place in a document is a passage, with enough nearby wording to find it again when the page changes shape. Returning should leave the surrounding map where it was and keep a pinned inspector on its chosen source.")));
    }
    let body = serde_json::to_string(&document).map_err(|e| e.to_string())?;
    let member = crate::local_intake::create_address(app, address, "Reading from the graph")?;
    let key = app.host.graph().get_node_key_by_id(member).ok_or("Missing reading fixture member")?;
    app.host.mutate_product_graph(|graph| {
        use mere::kernel::graph::apply::{apply_graph_delta, GraphDelta};
        apply_graph_delta(graph, GraphDelta::SetNodeMimeHint { key, mime_hint: Some("application/vnd.mere.document+json".into()) });
        apply_graph_delta(graph, GraphDelta::SetNodeBody { key, body: Some(body) });
    }).map_err(|e| e.to_string())?;
    crate::local_intake::persist_intake(app, member, 7).await?;
    let related_address="mere://reading/related-note";
    let mut related=document.clone();
    related.address=related_address.into();related.title=Some("Relationship field notes".into());
    related.blocks=vec![Block::Heading{level:1,spans:vec![InlineSpan::Text("Relationship field notes".into())]},
        paragraph("A relationship is a recorded connection between sources. A preview lets the reader inspect a destination without losing the passage they came from."),
        paragraph("Read follows the connection. Graph shows the destination in its surrounding scope. Closing this preview keeps the reading pane and its source binding in place.")];
    let related_body=serde_json::to_string(&related).map_err(|e|e.to_string())?;
    let other=crate::local_intake::create_address(app,related_address,"Relationship field notes")?;
    let other_key=app.host.graph().get_node_key_by_id(other).ok_or("Missing related fixture member")?;
    app.host.mutate_product_graph(|graph| {
        use mere::kernel::graph::{apply::{apply_graph_delta,GraphDelta,assert_relation},EdgeAssertion,SemanticSubKind};
        apply_graph_delta(graph,GraphDelta::SetNodeMimeHint{key:other_key,mime_hint:Some("application/vnd.mere.document+json".into())});
        apply_graph_delta(graph,GraphDelta::SetNodeBody{key:other_key,body:Some(related_body)});
        assert_relation(graph,key,other_key,EdgeAssertion::Semantic{sub_kind:SemanticSubKind::Cites,label:None}).expect("fixture endpoints");
    }).map_err(|e|e.to_string())?;
    crate::local_intake::persist_intake(app,other,8).await?;
    Ok(())
}
