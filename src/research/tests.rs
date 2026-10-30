// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::panes::{GraphId, SessionId};
fn fixture() -> (Target, RetainedSourceText) {
    let text = "α first 🐦 exact source 東京";
    let target = crate::saved_sources::Target {
        key: crate::saved_sources::Key {
            session: SessionId::new(),
            graph: GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: eidetic::ManifestId::of_blob(b"A1").to_string(),
        },
        text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
        selector: None,
    };
    let source = RetainedSourceText {
        source: source_id(&target.clone().into()).unwrap(),
        version: target.text_hash.clone(),
        title: Some("A".into()),
        text: text.into(),
    };
    (target.into(), source)
}
#[test]
fn scope_is_nonempty_exact_and_never_falls_back_to_the_corpus() {
    let (a, source_a) = fixture();
    let mut b = a.clone();
    if let Target::Capture(t) = &mut b {
        t.key.node = uuid::Uuid::new_v4();
    }
    let source_b = RetainedSourceText {
        source: source_id(&b).unwrap(),
        version: source_a.version.clone(),
        title: source_a.title.clone(),
        text: source_a.text.clone(),
    };
    let available = vec![source_a.clone(), source_b];
    let owner = namespace(a.session());
    assert!(scope(&owner, &[], &available).is_err());
    assert_eq!(
        scope(&owner, &[a.clone()], &available).unwrap(),
        vec![source_a.source.clone()]
    );
    assert_eq!(
        scope(&owner, &[a.clone(), b.clone()], &available)
            .unwrap()
            .len(),
        2,
        "equal text in different members remains distinct"
    );
    assert!(scope("turnstone/foreign/research/v1", &[a.clone()], &available).is_err());
    let mut missing = b;
    if let Target::Capture(t) = &mut missing {
        t.key.annotation_manifest = eidetic::ManifestId::of_blob(b"A2").to_string();
    }
    assert!(
        scope(&owner, &[a, missing], &available).is_err(),
        "one invalid explicitly chosen source fails the entire ask"
    );
}
#[test]
fn source_identity_roundtrips_all_owners_and_versions_and_rejects_substitution() {
    let (a, retained) = fixture();
    assert_eq!(target(&retained.source, &retained.version).unwrap(), a);
    let mut wrong = retained.source.clone();
    wrong.node = Some(uuid::Uuid::new_v4().to_string());
    assert!(target(&wrong, &retained.version).is_err());
    let mut wrong = retained.source.clone();
    wrong.owner = Some("foreign".into());
    assert!(target(&wrong, &retained.version).is_err());
    let mut wrong = retained.source.clone();
    wrong.kind = RecallSourceKind::PageText;
    assert!(target(&wrong, &retained.version).is_err());
    assert!(target(&retained.source, &eidetic::Hash::of(b"A2").to_string()).is_err());
    let mut passage = a;
    passage.set_selector(crate::saved_sources::Selector { start: 0, end: 1 });
    assert!(source_id(&passage).is_err());
}
#[test]
fn retained_clip_targets_roundtrip_separately_from_capture_targets() {
    let (capture, _) = fixture();
    let Target::Capture(c) = capture.clone() else {
        unreachable!()
    };
    assert_eq!(
        serde_json::to_value(&capture).unwrap(),
        serde_json::to_value(&c).unwrap()
    );
    let target = Target::Clip(ClipTarget {
        clip: ClipAdmission {
            session: c.key.session,
            graph: c.key.graph,
            node: c.key.node,
            manifest: eidetic::ManifestId::of_blob(b"retained clip"),
        },
        text_hash: c.text_hash,
        selector: None,
    });
    let source = source_id(&target).unwrap();
    assert_eq!(super::target(&source, target.text_hash()).unwrap(), target);
    assert_ne!(source, source_id(&capture).unwrap());
    let serialized = serde_json::to_value(&target).unwrap();
    assert_eq!(
        serde_json::from_value::<Target>(serialized.clone()).unwrap(),
        target
    );
    let mut ambiguous = serde_json::to_value(&capture).unwrap();
    ambiguous["clip"] = serialized["clip"].clone();
    assert!(serde_json::from_value::<Target>(ambiguous).is_err());
    let mut malformed = source.clone();
    malformed.source.push_str("?start=0&end=1");
    assert!(super::target(&malformed, target.text_hash()).is_err());
    malformed = source.clone();
    malformed.node = Some(uuid::Uuid::new_v4().to_string());
    assert!(super::target(&malformed, target.text_hash()).is_err());
    assert!(super::target(&source, &eidetic::Hash::of(b"foreign text").to_string()).is_err());
}
#[test]
fn sealed_knot_and_redshank_bodies_reopen_without_sources_or_private_backing() {
    use crate::retained_clip::{NativeClip, retain_native};
    let root = tempfile::tempdir().unwrap();
    let sealer = Arc::new(pandect::WalletEpochSealer::from_epoch(
        pandect::PersonaId::new(),
        pandect::KeyEpochId::new(),
        b"recall fixture",
    ));
    let source = "PRIVATE PREFIX café → harbor PRIVATE SUFFIX";
    let text = "café → harbor";
    let start = source.find(text).unwrap();
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/document/source",
        source,
        &crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            start,
            end: start + text.len(),
        },
    )
    .unwrap();
    let capture = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: None,
            entity: "producer absent".into(),
            revision: None,
        },
        capture_id: "recall".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("recall fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(1),
    };
    let policy = crate::retained_clip::limits().unwrap();
    let knot = retain_native(
        root.path(),
        NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &capture,
        policy,
        &*sealer,
    )
    .unwrap();
    let mut note: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    note["annotation"]["body"] =
        serde_json::json!({"kind":"text", "plain_text":"A retained timed note 🐦"});
    let redshank = retain_native(
        root.path(),
        NativeClip::Redshank {
            export: &note,
            voice_body: None,
        },
        &capture,
        policy,
        &*sealer,
    )
    .unwrap();
    // The producer buffers go away before the receiving owner resolves anything.
    drop(backing);
    drop(export);
    drop(note);
    let session = SessionId::new();
    let clips = [knot, redshank]
        .into_iter()
        .map(|manifest| ClipAdmission {
            session,
            graph: GraphId::new(),
            node: uuid::Uuid::new_v4(),
            manifest,
        })
        .collect::<Vec<_>>();
    let context = Context {
        session,
        generation: uuid::Uuid::new_v4(),
        admissions: vec![],
        clips: clips.clone(),
    };
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: sealer.clone(),
        epoch: Arc::new(|| Some(1)),
    }));
    for _ in 0..2 {
        assert_eq!(bridge.clip_text(&context, &clips[0]).unwrap().1, text);
        assert_eq!(
            bridge.clip_text(&context, &clips[1]).unwrap().1,
            "A retained timed note 🐦"
        );
    }
    let view = crate::retained_clip::read_sealed(root.path(), clips[0].manifest, policy, &*sealer)
        .unwrap();
    let mut exact = ClipTarget {
        clip: clips[0].clone(),
        text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
        selector: Some(crate::saved_sources::Selector { start: 0, end: 4 }),
    };
    let document = clip_document(&view, &exact).unwrap();
    assert!(document.blocks.iter().any(|block| matches!(block,
        inker::Block::Quote { blocks } if matches!(blocks.as_slice(),
            [inker::Block::Paragraph { spans }] if matches!(spans.as_slice(), [inker::InlineSpan::Text(value)] if value == "café")))));
    exact.selector = Some(crate::saved_sources::Selector { start: 5, end: 6 });
    let document = clip_document(&view, &exact).unwrap();
    assert!(document.blocks.iter().any(|block| matches!(block,
        inker::Block::Quote { blocks } if matches!(blocks.as_slice(),
            [inker::Block::Paragraph { spans }] if matches!(spans.as_slice(), [inker::InlineSpan::Text(value)] if value == "→")))));
    exact.selector = Some(crate::saved_sources::Selector { start: 0, end: 999 });
    assert!(clip_document(&view, &exact).is_err());
    exact.selector = None;
    assert!(clip_document(&view, &exact).is_ok());
    exact.text_hash = eidetic::Hash::of(b"wrong text").to_string();
    assert!(clip_document(&view, &exact).is_err());
    let mut foreign = clips[0].clone();
    foreign.node = uuid::Uuid::new_v4();
    assert!(bridge.clip_text(&context, &foreign).is_err());
    let voice_export: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    let voice = retain_native(
        root.path(),
        NativeClip::Redshank {
            export: &voice_export,
            voice_body: Some(include_bytes!(
                "../../scenarios/fixtures/retained_clips/voice.wav"
            )),
        },
        &capture,
        policy,
        &*sealer,
    )
    .unwrap();
    let voice_admission = ClipAdmission {
        manifest: voice,
        ..clips[0].clone()
    };
    let voice_context = Context {
        clips: vec![voice_admission.clone()],
        ..context.clone()
    };
    bridge.project(voice_context.clone());
    assert!(
        bridge
            .clip_text(&voice_context, &voice_admission)
            .unwrap_err()
            .contains("no recallable text")
    );
    bridge.project(context.clone());
    bridge.access(None);
    assert!(bridge.clip_text(&context, &clips[0]).is_err());
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer,
        epoch: Arc::new(|| Some(1)),
    }));
    bridge.project(Context {
        clips: vec![],
        ..context.clone()
    });
    assert!(bridge.clip_text(&context, &clips[0]).is_err());
}
#[test]
fn malformed_unicode_reference_is_reported_without_poisoning_the_owner_bridge() {
    let (Target::Capture(target), _) = fixture() else {
        unreachable!()
    };
    let admission = Admission {
        key: crate::saved_sources::Key {
            annotation_manifest: "🦊".repeat(8),
            ..target.key
        },
        occurrence: 0,
        title: "Unreadable reference".into(),
        url: "https://gone.test/invalid".into(),
        error: None,
    };
    let context = Context {
        session: admission.key.session,
        generation: uuid::Uuid::new_v4(),
        admissions: vec![admission.clone()],
        clips: vec![],
    };
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge
        .loaded(
            &context,
            &[Entry {
                admission,
                content: None,
                error: Some("malformed manifest identity".into()),
            }],
            vec![],
        )
        .unwrap();
    let choices = bridge.choices();
    assert_eq!(choices.len(), 1);
    assert!(choices[0].target.is_none());
    assert!(choices[0].error.is_some());
    assert!(choices[0].label.contains('🦊'));
    assert_eq!(bridge.context().unwrap(), context);
}

#[test]
fn withdrawn_and_readmitted_source_cannot_replay_an_old_citation_intent() {
    let (target, _) = fixture();
    let Target::Capture(capture) = &target else {
        unreachable!()
    };
    let admission = Admission {
        key: capture.key.clone(),
        occurrence: 0,
        title: "Source".into(),
        url: "https://example.test/source".into(),
        error: None,
    };
    let context = Context {
        session: capture.key.session,
        generation: uuid::Uuid::new_v4(),
        admissions: vec![admission],
        clips: vec![],
    };
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge.send(Intent::Citation {
        context: context.clone(),
        target,
    });
    bridge.project(Context {
        admissions: vec![],
        ..context.clone()
    });
    bridge.project(context);
    assert!(
        bridge.drain().is_empty(),
        "withdrawal must retire pending private citation intents even if the same identity is admitted again"
    );
}
