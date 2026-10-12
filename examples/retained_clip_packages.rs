// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Build selected packages from the owner-generated native export fixtures.
//! No source application process or source recording is used by a receiver.

fn main() {
    use eidetic::clip_profile::ClipSourceV1;
    use fleece::portable_clip::{CaptureContext, ClipLimits};
    let directory = std::path::PathBuf::from(std::env::args().nth(1).expect("output directory"));
    assert!(!directory.exists(), "package output must be new");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(".retained-clip-generated"),
        "generated selected packages",
    )
    .unwrap();
    let policy = ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    };
    let knot: serde_json::Value = serde_json::from_str(include_str!(
        "../scenarios/fixtures/retained_clips/knot.json"
    ))
    .unwrap();
    let voice: serde_json::Value = serde_json::from_str(include_str!(
        "../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    let mut note = voice.clone();
    note["annotation"]["id"] = serde_json::json!("text-1");
    note["annotation"]["body"] =
        serde_json::json!({"kind":"text","plain_text":"Portable note about a recording."});
    let mut ids = serde_json::Map::new();
    pollster::block_on(async {
        let mut source = eidetic::MemoryBackend::new();
        for (name, export, bytes) in [
            ("knot", &knot, None),
            ("note", &note, None),
            (
                "voice",
                &voice,
                Some(include_bytes!("../scenarios/fixtures/retained_clips/voice.wav").to_vec()),
            ),
        ] {
            let context = CaptureContext {
                source: ClipSourceV1 {
                    owner: Some(name.into()),
                    entity: format!("{name}-source"),
                    revision: None,
                },
                capture_id: format!("fixture-{name}"),
                provenance: eidetic::ProvenanceRecord::self_imported("native owner fixture"),
                trust: eidetic::TrustEnvelope::self_asserted(),
                captured_at: eidetic::Timestamp(7),
            };
            let id = if name == "knot" {
                fleece::portable_clip::retain_knot_export(
                    &mut source,
                    None,
                    export,
                    Some("private prefix; café; private suffix".as_bytes().to_vec()),
                    &context,
                    policy,
                )
                .await
                .unwrap()
            } else {
                fleece::portable_clip::retain_redshank_export(
                    &mut source,
                    None,
                    export,
                    bytes,
                    &context,
                    policy,
                )
                .await
                .unwrap()
            };
            let package = fleece::portable_clip::export_selected_clip(
                &mut source,
                None,
                id,
                &["body".into()],
                policy,
            )
            .await
            .unwrap();
            let receiver_record = eidetic::clip_profile::RetainedClipV1 {
                clip: package.clip.clone(),
                private_backing: vec![],
            };
            use eidetic::TypedPayload;
            let receiver_id =
                eidetic::ManifestId::of_blob(&receiver_record.serialize_to_bytes().unwrap());
            ids.insert(
                name.into(),
                serde_json::json!(turnstone::retained_clip::address(receiver_id)),
            );
            std::fs::write(
                directory.join(format!("{name}.mereclip")),
                serde_json::to_vec_pretty(&package).unwrap(),
            )
            .unwrap();
        }
    });
    std::fs::write(
        directory.join("addresses.json"),
        serde_json::to_vec_pretty(&ids).unwrap(),
    )
    .unwrap();
    println!("{}", serde_json::Value::Object(ids));
}
