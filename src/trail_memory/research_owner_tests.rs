// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::inference_service::RetainedSourceOwner;

#[test]
#[ignore = "requires an exited headed research-note profile"]
fn independently_inspect_exited_research_notes() {
    let root = PathBuf::from(std::env::var_os("TURNSTONE_NOTES_PROFILE").expect("profile"));
    let observation: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("TURNSTONE_NOTES_OBSERVATION").expect("source observation"),
        )
        .unwrap(),
    )
    .unwrap();
    let admissions: Vec<crate::saved_sources::Admission> =
        serde_json::from_value(observation["admissions"].clone()).unwrap();
    let mut source_store = FjallStore::open(
        root.join("sessions")
            .join(observation["session"].as_str().unwrap())
            .join("memory"),
    )
    .unwrap();
    let entries = saved_sources::load(&mut source_store, &admissions);
    assert!(entries.iter().all(|e| e.error.is_none()));
    let retained = knot::BlobClipEvidenceStore::open(root.join("knot-evidence"), 65536).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut reports = Vec::new();
    for file in std::fs::read_dir(root.join("notes")).unwrap() {
        let path = file.unwrap().path();
        let source = std::fs::read_to_string(&path).unwrap();
        let citations = knot::clip_citations(source.as_bytes()).unwrap();
        if citations.is_empty() {
            assert!(source.contains("Other note stays independent."));
            continue;
        }
        assert!(source.contains("My authored A1 commentary."));
        assert_eq!(citations.len(), 1);
        for citation in citations {
            let target = crate::saved_sources::parse_address(&citation.source_url)
                .unwrap()
                .unwrap();
            let entry = entries
                .iter()
                .find(|e| e.admission.key == target.key)
                .unwrap();
            let content = entry.content.as_ref().unwrap();
            let quote = crate::saved_sources::validate_selector(content, &target)
                .unwrap()
                .unwrap();
            assert_eq!(quote, "café");
            assert_eq!(
                content.text,
                "The Pelican observatory's access word is café."
            );
            let reference = &citation.evidence[0];
            let bytes = runtime
                .block_on(
                    retained
                        .blob_store()
                        .get_bytes(reference.blob_hash().unwrap()),
                )
                .unwrap();
            assert_eq!(bytes.as_ref(), content.text.as_bytes());
            assert_eq!(citation.read_passage(0, &bytes).unwrap(), quote);
            let mut corrupt = bytes.to_vec();
            corrupt[0] ^= 1;
            assert!(citation.read_passage(0, &corrupt).is_err());
            reports.push(serde_json::json!({"note":path.file_name().unwrap(),"source_url":citation.source_url,"target":target,"evidence":reference,"quote":quote,"verified_against_turnstone_capture":true,"portable_and_transport_hashes_verified":true,"corrupt_bytes_refused":true}));
        }
    }
    assert_eq!(reports.len(), 1);
    std::fs::write(std::env::var_os("TURNSTONE_NOTES_STORE_RECEIPT").expect("receipt"),serde_json::to_vec_pretty(&serde_json::json!({"result":"pass","independent_after_app_exit":true,"citations":reports})).unwrap()).unwrap();
}

#[test]
#[ignore = "requires an exited R4 headed profile and an output receipt path"]
fn independently_inspect_exited_research_profile() {
    let root = PathBuf::from(std::env::var_os("TURNSTONE_R4_PROFILE").expect("R4 profile"));
    let observations: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("TURNSTONE_R4_OBSERVATION").expect("R4 observation"))
            .unwrap(),
    )
    .unwrap();
    let session = observations["session"].as_str().unwrap();
    let admissions: Vec<crate::saved_sources::Admission> =
        serde_json::from_value(observations["admissions"].clone()).unwrap();
    let mut reports = Vec::new();
    for entry in std::fs::read_dir(root.join("sessions")).unwrap() {
        let directory = entry.unwrap().path();
        if !directory.join("memory/version").is_file() {
            continue;
        }
        let mut store = FjallStore::open(directory.join("memory")).unwrap();
        let references = pollster::block_on(PageTextStore::new(&store).references()).unwrap();
        assert!(
            references.is_empty(),
            "fresh ordinary history retained a PageText body"
        );
        let mut report = serde_json::json!({"session":directory.file_name().unwrap().to_string_lossy(),"page_text_references":references.len()});
        if directory.file_name().unwrap() == session {
            let entries = saved_sources::load(&mut store, &admissions);
            assert_eq!(entries.len(), 3);
            assert!(
                entries
                    .iter()
                    .all(|entry| entry.error.is_none() && entry.content.is_some())
            );
            report["validated_captures"] = serde_json::to_value(entries).unwrap();
        }
        reports.push(report);
    }
    assert!(
        reports
            .iter()
            .any(|report| report.get("validated_captures").is_some())
    );
    std::fs::write(std::env::var_os("TURNSTONE_R4_STORE_RECEIPT").expect("R4 store receipt"),
            serde_json::to_vec_pretty(&serde_json::json!({"result":"pass","independent_after_app_exit":true,"sessions":reports})).unwrap()).unwrap();
}

use crate::saved_sources::{Admission, Key, Target};
#[test]
fn research_owner_loads_only_live_captures_and_reopens_exact_versions_without_page_text_fallback() {
    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join(session.as_uuid().to_string())
        .join("memory");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = FjallStore::open(&directory).unwrap();
    let make = |label: &str| {
        let body =
            format!("<html><main>{label}: naïve — 東京 🐦 saved answer evidence</main></html>");
        crate::content::FetchedDocument {
            bytes: body.as_bytes().to_vec(),
            body,
            content_type: Some("text/html".into()),
            effective_url: Some("https://gone.test/A".into()),
            acquired_at_ms: 123456,
        }
    };
    let a1 = capture_source_document_in(&mut store, &make("A1")).unwrap();
    let a2 = capture_source_document_in(&mut store, &make("A2")).unwrap();
    let orphan = capture_source_document_in(&mut store, &make("orphan foreign sentinel")).unwrap();
    drop(store);
    let node = uuid::Uuid::new_v4();
    let admitted = |manifest: String| Admission {
        key: Key {
            session,
            graph: crate::panes::GraphId::nil(),
            node,
            annotation_manifest: manifest,
        },
        occurrence: 0,
        title: "Source A".into(),
        url: "https://gone.test/A".into(),
        error: None,
    };
    let generation = uuid::Uuid::new_v4();
    let context = crate::research::Context {
        session,
        generation,
        clips: vec![],
        admissions: vec![
            admitted(a1.annotation_manifest.clone()),
            admitted(a2.annotation_manifest.clone()),
        ],
    };
    let (actor, _) = spawn_trail_for_session(
        std::sync::Arc::new(|| {}),
        Some(directory.clone()),
        Default::default(),
        generation,
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(
        custody_test_fixture::custody(),
    ))));
    let actor = std::sync::Arc::new(actor);
    let bridge = crate::research::Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    let owner = CaptureInferenceOwner {
        trail: actor.clone(),
        bridge: bridge.clone(),
    };
    let namespace = crate::research::namespace(session);
    let sources = owner.snapshot(&namespace).unwrap();
    assert_eq!(sources.len(), 2);
    assert!(sources.iter().all(|source| !source.text.contains("orphan")));
    let source_a1 = sources
        .iter()
        .find(|source| source.text.starts_with("A1"))
        .unwrap();
    assert!(
        owner
            .resolve(&source_a1.source, &source_a1.version)
            .unwrap()
            .starts_with("A1")
    );
    let a1_target = crate::research::target(&source_a1.source, &source_a1.version).unwrap();
    assert!(crate::research::scope(&namespace, &[a1_target.clone()], &sources).is_ok());
    assert!(crate::research::scope(&namespace, &[], &sources).is_err());
    let opened = crate::inference_service::OpenSource {
        run: format!("{namespace}/citation"),
        handle: 1,
        retained: source_a1.clone(),
        selector: eidetic::models::recall::PassageSelector {
            start: 0,
            end: source_a1.text.chars().count(),
        },
    };
    owner.open_source(&opened, Some(generation)).unwrap();
    let intents = bridge.drain();
    assert_eq!(
        intents.len(),
        1,
        "citation completion queues an App action without any surface frame"
    );
    match &intents[0] {
        crate::research::Intent::Citation {
            context: actual,
            target,
        } => {
            assert_eq!(actual, &context);
            assert_eq!(target.node(), a1_target.node());
            assert!(target.selector().is_some());
        },
        _ => panic!("expected exact citation intent"),
    }
    assert!(
        owner
            .open_source(&opened, Some(uuid::Uuid::new_v4()))
            .is_err()
    );
    assert!(bridge.drain().is_empty());

    assert!(owner.snapshot("turnstone/wrong/research/v1").is_err());
    let mut false_source = source_a1.source.clone();
    false_source.owner = Some("foreign".into());
    assert!(owner.resolve(&false_source, &source_a1.version).is_err());
    let crate::research::Target::Capture(a1_capture) = a1_target.clone() else {
        unreachable!()
    };
    let unadmitted = Target {
        key: Key {
            annotation_manifest: orphan.annotation_manifest,
            ..a1_capture.key.clone()
        },
        ..a1_capture.clone()
    };
    assert!(
        owner
            .resolve(
                &crate::research::source_id(&unadmitted.clone().into()).unwrap(),
                &unadmitted.text_hash
            )
            .is_err()
    );
    // Selection is ask intent only: old A1 remains resolvable alongside A2.
    assert!(owner.resolve(&source_a1.source, &source_a1.version).is_ok());
    bridge.project(crate::research::Context {
        admissions: vec![admitted(a2.annotation_manifest)],
        ..context.clone()
    });
    assert!(
        owner
            .resolve(&source_a1.source, &source_a1.version)
            .unwrap_err()
            .contains("withdrawn")
    );
    bridge.project(crate::research::Context {
        generation: uuid::Uuid::new_v4(),
        ..context
    });
    assert!(
        owner.snapshot(&namespace).is_err(),
        "an old opening cannot read the new actor context"
    );
    let (ack, receive) = std::sync::mpsc::sync_channel(1);
    actor.command(TrailCommand::Release(ack));
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(owner);
    std::sync::Arc::try_unwrap(actor)
        .ok()
        .unwrap()
        .join()
        .unwrap();
    let mut reopened = FjallStore::open(&directory).unwrap();
    assert!(
        saved_sources::load(&mut reopened, &[admitted(a1.annotation_manifest)])[0]
            .content
            .as_ref()
            .unwrap()
            .text
            .starts_with("A1")
    );
}
