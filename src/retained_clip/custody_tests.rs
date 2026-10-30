// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use eidetic::{PayloadSealer, Store};

fn policy() -> ClipLimits {
    ClipLimits {
        max_parts: 16,
        max_bytes: 1_000_000,
        max_record_bytes: 64_000,
    }
}
fn sealer() -> pandect::WalletEpochSealer {
    pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"fixture wallet epoch",
    )
}
fn context() -> CaptureContext {
    CaptureContext {
        source: ClipSourceV1 {
            owner: None,
            entity: "mere://knot/document/source".into(),
            revision: None,
        },
        capture_id: "selected-once".into(),
        provenance: ProvenanceRecord::self_imported("owner fixture"),
        trust: TrustEnvelope::self_asserted(),
        captured_at: Timestamp(7),
    }
}

#[test]
fn native_knot_custody_reopens_and_onward_copy_excludes_private_backing() {
    let root = tempfile::tempdir().unwrap();
    let source = "private prefix; café; private suffix";
    let start = source.find("café").unwrap();
    let selected = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start,
        end: start + "café".len(),
    };
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/document/source",
        source,
        &selected,
    )
    .unwrap();
    let seal = sealer();
    let id = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &context(),
        policy(),
        &seal,
    )
    .unwrap();
    assert_eq!(
        retain_native(
            root.path(),
            NativeClip::Knot {
                export: &export,
                backing: &backing
            },
            &context(),
            policy(),
            &seal
        )
        .unwrap(),
        id
    );
    let first = read_sealed(root.path(), id, policy(), &seal).unwrap();
    assert!(matches!(first.body, ClipBody::Text(ref text) if text == "café"));
    assert_eq!(first.clip.anchors[0].units, "utf8-bytes");
    assert!(first.clip.source.revision.is_none());
    let mut store = open(root.path(), false).unwrap();
    let held = pollster::block_on(eidetic::load_typed_sealed::<
        eidetic::clip_profile::RetainedClipV1,
    >(&mut store, &mut eidetic::NoFetcher, Some(&seal), id))
    .unwrap()
    .unwrap();
    assert_eq!(held.private_backing.len(), 1);
    for part in held.clip.parts.iter().chain(&held.private_backing) {
        let manifest =
            pollster::block_on(eidetic::manifest::load_manifest(&mut store, part.manifest))
                .unwrap()
                .unwrap();
        assert!(eidetic::seal::seal_marker(&manifest).unwrap().is_some());
        let bytes = pollster::block_on(store.get(&format!("blob:{}", part.manifest.0.to_hex())))
            .unwrap()
            .unwrap();
        assert_ne!(bytes, backing);
        assert_ne!(bytes, "café".as_bytes());
    }
    let backing_manifest = pollster::block_on(eidetic::manifest::load_manifest(
        &mut store,
        held.private_backing[0].manifest,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        pollster::block_on(eidetic::resolve_sealed_blob(
            &mut store,
            &mut eidetic::NoFetcher,
            Some(&seal),
            &backing_manifest
        ))
        .unwrap(),
        backing
    );
    drop(store);
    assert!(read_sealed(root.path(), id, policy(), &sealer()).is_err());
    let package_path = root.path().join("selected.mereclip");
    export_sealed(root.path(), id, &package_path, policy(), false, &seal).unwrap();
    let package: ClipPackageV1 =
        serde_json::from_slice(&std::fs::read(&package_path).unwrap()).unwrap();
    assert_eq!(package.payloads.len(), 1);
    assert_eq!(package.payloads[0].bytes, "café".as_bytes());
    let second = tempfile::tempdir().unwrap();
    let second_seal = sealer();
    let second_id =
        import_sealed(second.path(), &package_path, policy(), 10, &second_seal).unwrap();
    std::fs::remove_file(package_path).unwrap();
    assert_eq!(
        read_sealed(second.path(), second_id, policy(), &second_seal)
            .unwrap()
            .clip,
        first.clip
    );
    let mut second_store = open(second.path(), false).unwrap();
    let received = pollster::block_on(eidetic::load_typed_sealed::<
        eidetic::clip_profile::RetainedClipV1,
    >(
        &mut second_store,
        &mut eidetic::NoFetcher,
        Some(&second_seal),
        second_id,
    ))
    .unwrap()
    .unwrap();
    assert!(received.private_backing.is_empty());
}

struct Locked;
impl PayloadSealer for Locked {
    fn seal(&self, _: &Hash, _: &[u8]) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        Err(eidetic::Error::new("custody locked"))
    }
    fn unseal(&self, _: &Hash, _: &eidetic::SealedBlobRef, _: &[u8]) -> eidetic::Result<Vec<u8>> {
        Err(eidetic::Error::new("custody locked"))
    }
}

#[test]
fn legacy_unsealed_content_requires_migration_for_private_reads_and_exports() {
    let root = tempfile::tempdir().unwrap();
    let export: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/knot.json"
    ))
    .unwrap();
    let mut memory = eidetic::MemoryBackend::new();
    let package = pollster::block_on(async {
        let id = fleece::portable_clip::retain_knot_export(
            &mut memory,
            None,
            &export,
            None,
            &context(),
            policy(),
        )
        .await
        .unwrap();
        fleece::portable_clip::export_selected_clip(
            &mut memory,
            None,
            id,
            &["body".into()],
            policy(),
        )
        .await
        .unwrap()
    });
    let file = root.path().join("legacy.mereclip");
    std::fs::write(&file, serde_json::to_vec(&package).unwrap()).unwrap();
    let id = import(root.path(), &file, policy(), 7).unwrap();
    assert!(
        read_sealed(root.path(), id, policy(), &Locked)
            .err()
            .expect("unsealed private read refused")
            .contains("migration")
    );
    let output = root.path().join("refused.mereclip");
    assert!(export_sealed(root.path(), id, &output, policy(), false, &Locked).is_err());
    assert!(!output.exists());
    assert!(
        import_sealed(root.path(), &file, policy(), 7, &sealer())
            .unwrap_err()
            .contains("migration")
    );
    assert!(
        read(root.path(), id, policy()).is_ok(),
        "refusal preserves the historical fixture"
    );
}

#[test]
fn unavailable_custody_and_stale_backing_refuse_before_creating_storage() {
    let root = tempfile::tempdir().unwrap();
    let source = "private café source";
    let selected = crate::producer_retention::KnotSelection {
        source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
        start: 8,
        end: 13,
    };
    let (export, backing) =
        crate::producer_retention::export_knot_selection("mere://knot/source", source, &selected)
            .unwrap();
    assert!(
        retain_native(
            root.path(),
            NativeClip::Knot {
                export: &export,
                backing: &backing
            },
            &context(),
            policy(),
            &Locked
        )
        .unwrap_err()
        .contains("locked")
    );
    assert!(!root.path().join(STORE_DIR).exists());
    assert!(
        retain_native(
            root.path(),
            NativeClip::Knot {
                export: &export,
                backing: b"changed source"
            },
            &context(),
            policy(),
            &sealer()
        )
        .is_err()
    );
    assert!(!root.path().join(STORE_DIR).exists());
}

#[test]
fn native_redshank_export_keeps_body_and_timed_target_distinct() {
    use redshank_model::*;
    let mut model = RedshankModel::default();
    let item = ItemId("recording".into());
    model
        .add_item(LibraryItem::DirectAudio {
            id: item.clone(),
            title: "Recording".into(),
            source: MediaSource::Enclosure {
                url: "https://unavailable.invalid/recording.wav".into(),
            },
        })
        .unwrap();
    let id = AnnotationId("voice".into());
    model
        .add_annotation(Annotation {
            id: id.clone(),
            target: TimedTarget {
                item_id: item,
                offset_ms: 1250,
                end_offset_ms: Some(4500),
                pressed_offset_ms: Some(1400),
                fingerprint: None,
                representation: RepresentationReceipt::default(),
            },
            body: NoteBody::Audio {
                blob_id: "voice-body".into(),
                media_type: "audio/wav".into(),
                duration_ms: 500,
            },
            created_at_ms: 7,
            privacy: NotePrivacy::Shareable,
        })
        .unwrap();
    let export = model.export_selected_note(&id).unwrap();
    let bytes = include_bytes!("../../scenarios/fixtures/retained_clips/voice.wav");
    let root = tempfile::tempdir().unwrap();
    let seal = sealer();
    let retained = retain_native(
        root.path(),
        NativeClip::Redshank {
            export: &export,
            voice_body: Some(bytes),
        },
        &context(),
        policy(),
        &seal,
    )
    .unwrap();
    drop(model);
    let view = read_sealed(root.path(), retained, policy(), &seal).unwrap();
    assert!(matches!(view.body, ClipBody::Voice(ref held) if held == bytes));
    assert_eq!(view.clip.anchors[0].units, "milliseconds");
    assert_eq!(view.clip.anchors[0].selector["offset_ms"], 1250);
    assert_eq!(view.clip.anchors[0].selector["pressed_offset_ms"], 1400);
    assert!(!root.path().join(STORE_DIR).join("playback").exists());
}

/// Fresh process receipt driven by the fixture runner, never a live profile.
#[test]
#[ignore]
fn custody_process_roundtrip_fixture() {
    use djinn::door::custody_identity::CustodyIdentity;
    let root =
        PathBuf::from(std::env::var_os("EIDETIC_CLIENT_FIXTURE_ROOT").expect("fixture root"));
    assert_eq!(
        std::fs::read_to_string(root.join("fixture-marker")).unwrap(),
        "eidetic-retention-20261019\n"
    );
    let endpoint = std::env::var("EIDETIC_CUSTODY_FIXTURE_ENDPOINT").expect("fixture endpoint");
    let identity = crate::identity::RootIdentity::Custody(
        CustodyIdentity::connect_at(&endpoint, "turnstone").expect("fixture custodian"),
    );
    let references = root.join("references.json");
    let phase = std::env::var("EIDETIC_CLIENT_FIXTURE_PHASE").expect("fixture phase");
    if phase == "seed" {
        let source = "private prefix; café; private suffix";
        let start = source.find("café").unwrap();
        let selected = crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start,
            end: start + "café".len(),
        };
        let (export, backing) = crate::producer_retention::export_knot_selection(
            "mere://knot/process-source",
            source,
            &selected,
        )
        .unwrap();
        let first = root.join("first");
        let id = retain_native(
            &first,
            NativeClip::Knot {
                export: &export,
                backing: &backing,
            },
            &context(),
            policy(),
            &identity,
        )
        .unwrap();
        let package = root.join("selected.mereclip");
        export_sealed(&first, id, &package, policy(), false, &identity).unwrap();
        let second_id =
            import_sealed(&root.join("second"), &package, policy(), 10, &identity).unwrap();
        std::fs::remove_file(package).unwrap();
        let voice_package = PathBuf::from(
            std::env::var_os("EIDETIC_VOICE_FIXTURE_PACKAGE")
                .expect("owner-produced voice package"),
        );
        let voice_id =
            import_sealed(&root.join("voice"), &voice_package, policy(), 10, &identity).unwrap();
        std::fs::remove_file(&voice_package).unwrap();
        std::fs::write(
            references,
            serde_json::to_vec(
                &serde_json::json!({"first":id,"second":second_id,"voice":voice_id}),
            )
            .unwrap(),
        )
        .unwrap();
    } else {
        assert_eq!(phase, "reopen");
        let ids: serde_json::Value =
            serde_json::from_slice(&std::fs::read(references).unwrap()).unwrap();
        for key in ["first", "second"] {
            let id: ManifestId = serde_json::from_value(ids[key].clone()).unwrap();
            let held = read_sealed(&root.join(key), id, policy(), &identity).unwrap();
            assert!(matches!(held.body, ClipBody::Text(ref text) if text == "café"));
            assert_eq!(held.clip.anchors[0].units, "utf8-bytes");
        }
        let voice_id: ManifestId = serde_json::from_value(ids["voice"].clone()).unwrap();
        let voice = read_sealed(&root.join("voice"), voice_id, policy(), &identity).unwrap();
        assert!(matches!(voice.body, ClipBody::Voice(ref bytes) if bytes.starts_with(b"RIFF")));
        assert_eq!(voice.clip.anchors[0].units, "milliseconds");
        assert_eq!(voice.clip.anchors[0].selector["offset_ms"], 1250);
        assert_eq!(voice.clip.anchors[0].selector["pressed_offset_ms"], 1400);
        assert!(!root.join("selected.mereclip").exists());
    }
    std::fs::write(root.join(format!("{phase}-passed.json")), serde_json::to_vec(&serde_json::json!({"phase":phase,"pid":std::process::id(),"passed":true,"scope":"synthetic wallet-backed IPC, native producer and store; no headed UI claim"})).unwrap()).unwrap();
}

#[test]
fn model_free_clip_open_refusal_explains_setting_on_retain_import_and_export() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let mut note: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    note["annotation"]["body"] = serde_json::json!({"kind":"text", "plain_text":"Retained note"});
    let seal = sealer();
    let source_id = retain_native(
        source.path(),
        NativeClip::Redshank {
            export: &note,
            voice_body: None,
        },
        &context(),
        policy(),
        &seal,
    )
    .unwrap();
    let package = source.path().join("selected.clip.json");
    export_sealed(source.path(), source_id, &package, policy(), false, &seal).unwrap();
    let seeded = muniment::FjallStore::open(destination.path().join(STORE_DIR)).unwrap();
    pollster::block_on(seeded.put("oversized", &vec![42; 8192])).unwrap();
    seeded.sync_all().unwrap();
    drop(seeded);
    let mut limits = crate::research_storage::StorageLimits::default();
    limits.recovery.max_value_bytes = 16;
    let output = destination.path().join("refused.clip.json");
    let error = match open_with_limits(destination.path(), false, &limits) {
        Ok(_) => panic!("oversized retained store must refuse"),
        Err(error) => error,
    };
    assert!(matches!(
        error.storage_error(),
        Some(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::ValueBytes,
            limit: 16
        })
    ));

    let retain_error = retain_native_with_limits(
        destination.path(),
        NativeClip::Redshank {
            export: &note,
            voice_body: None,
        },
        &context(),
        policy(),
        &seal,
        &limits,
    )
    .unwrap_err();
    let import_error =
        import_sealed_with_limits(destination.path(), &package, policy(), 7, &seal, &limits)
            .unwrap_err();
    let export_error = export_sealed_with_limits(
        destination.path(),
        source_id,
        &output,
        policy(),
        false,
        &seal,
        &limits,
    )
    .unwrap_err();
    for error in [retain_error, import_error, export_error] {
        assert!(
            error.contains("Retained clip")
                && error.contains("open")
                && error.contains("ValueBytes")
                && error.contains("16")
                && error.contains("Open: value bytes")
                && error.contains("settings")
                && error.contains("retry")
                && error.contains("restart Turnstone if"),
            "{error}"
        );
    }
    assert!(!output.exists());
}
