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
