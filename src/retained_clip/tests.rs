// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use inker::SessionEngine;
use serde_json::json;

fn policy() -> ClipLimits {
    ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    }
}
fn package(voice: bool) -> ClipPackageV1 {
    pollster::block_on(async {
        let mut source = eidetic::MemoryBackend::new();
        let context = CaptureContext {
            source: ClipSourceV1 {
                owner: Some("owner".into()),
                entity: "original".into(),
                revision: None,
            },
            capture_id: "capture".into(),
            provenance: ProvenanceRecord::self_imported("native owner fixture"),
            trust: TrustEnvelope::self_asserted(),
            captured_at: Timestamp(7),
        };
        let id = if voice {
            let export = serde_json::from_str(include_str!(
                "../../scenarios/fixtures/retained_clips/redshank.json"
            ))
            .unwrap();
            fleece::portable_clip::retain_redshank_export(
                &mut source,
                None,
                &export,
                Some(include_bytes!("../../scenarios/fixtures/retained_clips/voice.wav").to_vec()),
                &context,
                policy(),
            )
            .await
            .unwrap()
        } else {
            let export = serde_json::from_str(include_str!(
                "../../scenarios/fixtures/retained_clips/knot.json"
            ))
            .unwrap();
            fleece::portable_clip::retain_knot_export(
                &mut source,
                None,
                &export,
                Some("private prefix; café; private suffix".as_bytes().to_vec()),
                &context,
                policy(),
            )
            .await
            .unwrap()
        };
        fleece::portable_clip::export_selected_clip(
            &mut source,
            None,
            id,
            &["body".into()],
            policy(),
        )
        .await
        .unwrap()
    })
}

#[test]
fn native_export_renders_through_real_session_after_receiver_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("selected.mereclip");
    let package = package(false);
    std::fs::write(&file, serde_json::to_vec(&package).unwrap()).unwrap();
    let root = directory.path().join("receiver");
    let id = import(&root, &file, policy(), 1).unwrap();
    std::fs::remove_file(&file).unwrap();
    let held = read(&root, id, policy()).unwrap();
    assert!(matches!(&held.body, ClipBody::Text(body) if body == "café"));
    let document = held.document();
    assert!(document.outgoing_links().is_empty());
    assert!(document.blocks.iter().any(|b| matches!(b, Block::MetadataRow { label, value } if label == "Source anchor (utf8-bytes)" && value.contains("mere://knot/document/coffee"))));
    let spawn = inker::SessionSpawnRequest::new(address(id))
        .with_body(serde_json::to_string(&document).unwrap())
        .with_viewport(800, 600);
    let mut session = ClipSessionEngine.spawn(&spawn).unwrap();
    assert_eq!(session.inspect().unwrap().title.as_deref(), Some("Coffee"));
    session.frame(800, 600);
    assert!(session.subresources().is_empty());
    let mut store = open(&root, false).unwrap();
    let retained = pollster::block_on(
        eidetic::load_typed::<eidetic::clip_profile::RetainedClipV1>(
            &mut store,
            &mut eidetic::NoFetcher,
            id,
        ),
    )
    .unwrap()
    .unwrap();
    assert!(retained.private_backing.is_empty());
}

#[test]
fn voice_bytes_are_local_and_recording_stays_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("selected.mereclip");
    std::fs::write(&file, serde_json::to_vec(&package(true)).unwrap()).unwrap();
    let root = directory.path().join("receiver");
    let id = import(&root, &file, policy(), 1).unwrap();
    std::fs::remove_file(&file).unwrap();
    let held = read(&root, id, policy()).unwrap();
    let doc = held.document();
    assert!(
        doc.outgoing_links()
            .iter()
            .all(|link| link.starts_with("mere://clip/"))
    );
    assert!(doc.blocks.iter().any(
        |b| matches!(b, Block::Badge { text } if text.contains("Source recording unavailable"))
    ));
    let cache = held.playback_path(&root).unwrap();
    assert_eq!(
        std::fs::read(&cache).unwrap(),
        include_bytes!("../../scenarios/fixtures/retained_clips/voice.wav")
    );
    std::fs::write(&cache, b"changed").unwrap();
    assert!(held.playback_path(&root).is_err());
}

#[test]
fn missing_unsupported_or_oversized_material_creates_no_receiver_store() {
    for kind in ["missing", "unsupported", "altered", "oversized"] {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("selected.mereclip");
        let mut selected = package(false);
        let mut limits = policy();
        match kind {
            "missing" => selected.payloads.clear(),
            "unsupported" => selected.clip.parts[0].media_type = "application/unknown".into(),
            "altered" => selected.payloads[0].bytes[0] ^= 1,
            "oversized" => limits.max_bytes = 1,
            _ => unreachable!(),
        }
        std::fs::write(&file, serde_json::to_vec(&selected).unwrap()).unwrap();
        let root = directory.path().join("receiver");
        assert!(import(&root, &file, limits, 1).is_err());
        assert!(!root.exists());
    }
}

#[test]
fn custody_and_session_refusals_cannot_create_clip_storage() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::test_stub_at(root.path().into());
    assert!(app.clip_destination().is_ok());
    app.identity = None;
    assert!(app.clip_destination().is_err());
    assert!(!root.path().join(STORE_DIR).exists());
    assert_eq!(
        crate::ring::ring_of(&crate::action::Action::ImportClipPackage("any".into())),
        crate::ring::Ring::HostOnly
    );
}

#[test]
fn internal_play_controls_are_host_only_and_refuse_paths_or_foreign_shapes() {
    let id = ManifestId::of_blob(b"fixture");
    assert_eq!(request(&address(id)).unwrap(), Some(ClipRequest::Read(id)));
    assert_eq!(
        crate::ring::ring_of(&crate::action::Action::OpenAddress(address(id))),
        crate::ring::Ring::HostOnly
    );
    let play = format!("{}/play", address(id));
    assert_eq!(
        crate::ring::ring_of(&crate::action::Action::OpenAddress(play)),
        crate::ring::Ring::HostOnly
    );
    for bad in [
        "mere://clip/../voice.wav",
        "mere://clip/voice.wav",
        "mere://clip/",
    ] {
        assert!(request(bad).is_err());
    }
    assert!(request(&format!("{}?path=private", address(id))).is_err());
    let mut foreign = serde_json::to_value(package(false)).unwrap();
    foreign["future_fields"] = json!("must not vanish");
    assert!(serde_json::from_value::<ClipPackageV1>(foreign).is_err());
}

#[test]
fn clip_opens_target_the_new_reference_while_workbench_has_another_member() {
    let root = tempfile::tempdir().unwrap();
    let mut app = crate::app::App::test_stub_at(root.path().into());
    let first = address(ManifestId::of_blob(b"first"));
    app.update(crate::action::Action::OpenAddress(first.clone()));
    app.update(crate::action::Action::OpenInWorkbench);
    let next = address(ManifestId::of_blob(b"second"));
    let effects = app.update(crate::action::Action::OpenAddress(next.clone()));
    assert!(effects.iter().any(
        |effect| matches!(effect, crate::action::Effect::SpawnContent { url, .. } if *url == next)
    ));
    assert!(!effects.iter().any(
        |effect| matches!(effect, crate::action::Effect::SpawnContent { url, .. } if *url == first)
    ));
}
