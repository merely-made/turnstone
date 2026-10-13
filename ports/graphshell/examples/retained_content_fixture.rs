// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Export synthetic selected packages through the actual Fleece owner.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("catalog") {
        let mut app = graphshell::app::GraphshellApp::fixture(
            muniment::MemoryBackend::default(),
            graphshell::mere_host::SelectedPersonaRef {
                persona: graphshell::mere_host::FIXTURE_PERSONA_ADDRESS.into(),
                profile: "fixture".into(),
            },
        )?;
        pollster::block_on(graphshell::retained_content::fixture::seed(
            &mut app,
            mere_document_lanes::retained_clip::ClipLimits {
                max_parts: 64,
                max_bytes: 1024 * 1024,
                max_record_bytes: 1024 * 1024,
            },
        ))?;
        let catalog: Vec<_> = app
            .host
            .graph()
            .nodes()
            .map(|(_, node)| serde_json::json!({"title":node.title,"address":node.url()}))
            .collect();
        println!("{}", serde_json::to_string(&catalog)?);
        return Ok(());
    }
    let voice = std::env::args().nth(1).as_deref() == Some("voice");
    let package = pollster::block_on(async {
        let mut store = eidetic::MemoryBackend::new();
        let limits = mere_document_lanes::retained_clip::ClipLimits {
            max_parts: 16,
            max_bytes: 1024 * 1024,
            max_record_bytes: 128 * 1024,
        };
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
        let id = if voice {
            let selected = serde_json::from_str(include_str!(
                "../../../crates/fleece/tests/fixtures/portable_clip/redshank.json"
            ))
            .unwrap();
            fleece::portable_clip::retain_redshank_export(
                &mut store,
                None,
                &selected,
                Some(
                    include_bytes!("../../../crates/fleece/tests/fixtures/portable_clip/voice.wav")
                        .to_vec(),
                ),
                &context,
                limits,
            )
            .await?
        } else {
            let selected = serde_json::from_str(include_str!(
                "../../../crates/fleece/tests/fixtures/portable_clip/knot.json"
            ))
            .unwrap();
            fleece::portable_clip::retain_knot_export(
                &mut store,
                None,
                &selected,
                Some("private prefix; café; private suffix".as_bytes().to_vec()),
                &context,
                limits,
            )
            .await?
        };
        fleece::portable_clip::export_selected_clip(&mut store, None, id, &["body".into()], limits)
            .await
    })?;
    println!("{}", serde_json::to_string(&package)?);
    Ok(())
}
