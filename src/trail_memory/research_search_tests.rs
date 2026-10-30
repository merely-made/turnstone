// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::research::{Bridge, ClipAccess, ClipAdmission, Context};
use crate::research_search::{Member, Request};
use eidetic_search::query::{Coverage, ProducerKind, QueryMode, SearchQuery, SearchSettings};
use std::sync::Arc;

#[test]
fn model_free_mixed_producers_scope_coverage_and_stale_revision() {
    let root = tempfile::tempdir().unwrap();
    let session = crate::panes::SessionId::new();
    let directory = root
        .path()
        .join(session.as_uuid().to_string())
        .join("memory");
    std::fs::create_dir_all(&directory).unwrap();
    let mut store = FjallStore::open(&directory).unwrap();
    let html = "<html><main>Capture café and 🐦</main></html>";
    let capture = capture_source_document_in(
        &mut store,
        &crate::content::FetchedDocument {
            bytes: html.as_bytes().to_vec(),
            body: html.into(),
            content_type: Some("text/html".into()),
            effective_url: Some("https://gone.test/capture".into()),
            acquired_at_ms: 1_000,
        },
    )
    .unwrap();
    drop(store);
    let custody = custody_test_fixture::custody();
    let body = "PRIVATE café → harbor PRIVATE";
    let selected = "café → harbor";
    let start = body.find(selected).unwrap();
    let (export, backing) = crate::producer_retention::export_knot_selection(
        "mere://knot/source",
        body,
        &crate::producer_retention::KnotSelection {
            source_hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
            start,
            end: start + selected.len(),
        },
        1_000_000,
    )
    .unwrap();
    let capture_context = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: None,
            entity: "producer absent".into(),
            revision: None,
        },
        capture_id: "fixture".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(1),
    };
    let knot = crate::retained_clip::retain_native(
        root.path(),
        crate::retained_clip::NativeClip::Knot {
            export: &export,
            backing: &backing,
        },
        &capture_context,
        crate::retained_clip::limits().unwrap(),
        &*custody.sealer,
    )
    .unwrap();
    let word_item =
        knot_composition::CollectionItem::new(knot_composition::ItemKind::Word, "Word", "coffee")
            .with_document_source(
                knot_composition::DocumentAnchor::capture(
                    "mere://knot/source",
                    body,
                    start..start + "café".len(),
                )
                .unwrap(),
            );
    let word_export =
        knot_composition::selected_export::export_selected_content(&word_item).unwrap();
    let word = crate::retained_clip::retain_native(
        root.path(),
        crate::retained_clip::NativeClip::Knot {
            export: &word_export,
            backing: body.as_bytes(),
        },
        &capture_context,
        crate::retained_clip::limits().unwrap(),
        &*custody.sealer,
    )
    .unwrap();
    let word_view = crate::retained_clip::read_sealed(
        root.path(),
        word,
        crate::retained_clip::limits().unwrap(),
        &*custody.sealer,
    )
    .unwrap();
    assert_eq!(
        crate::research_search::producer(&word_view),
        ProducerKind::Selection
    );
    assert!(
        matches!(&word_view.body, crate::retained_clip::ClipBody::Text(body) if body == "coffee")
    );
    assert_eq!(word_view.clip.anchors[0].selector["exact_quote"], "café");
    let mut note: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    note["annotation"]["body"] = serde_json::json!({"kind":"text", "plain_text":"Note café 🐦"});
    let redshank = crate::retained_clip::retain_native(
        root.path(),
        crate::retained_clip::NativeClip::Redshank {
            export: &note,
            voice_body: None,
        },
        &capture_context,
        crate::retained_clip::limits().unwrap(),
        &*custody.sealer,
    )
    .unwrap();
    let graph = crate::panes::GraphId::new();
    let clip = |manifest| ClipAdmission {
        session,
        graph,
        node: uuid::Uuid::new_v4(),
        manifest,
    };
    let clips = vec![clip(knot), clip(redshank)];
    let admission = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            session,
            graph,
            node: uuid::Uuid::new_v4(),
            annotation_manifest: capture.annotation_manifest,
        },
        occurrence: 0,
        title: "Unique capture title".into(),
        url: "https://gone.test/capture".into(),
        error: None,
    };
    let unavailable = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            annotation_manifest: eidetic::ManifestId::of_blob(b"missing").to_string(),
            ..admission.key.clone()
        },
        title: "Unavailable".into(),
        ..admission.clone()
    };
    let context = Context {
        session,
        generation: uuid::Uuid::new_v4(),
        admissions: vec![
            admission.clone(),
            crate::saved_sources::Admission {
                occurrence: 2,
                ..admission.clone()
            },
            unavailable,
        ],
        clips: clips.clone(),
    };
    let (actor, _) = spawn_trail_for_session(
        Arc::new(|| {}),
        Some(directory),
        Default::default(),
        context.generation,
    );
    assert!(actor.command(TrailCommand::SetCustody(Some(custody.clone()))));
    let actor = Arc::new(actor);
    let bridge = Bridge::default();
    bridge.set_available(true);
    bridge.project(context.clone());
    bridge.access(Some(ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    let mut knot_view = bridge.clip_view(&context, &clips[0]).unwrap();
    assert_eq!(
        crate::research_search::producer(&knot_view),
        ProducerKind::Selection
    );
    knot_view.clip.source_evidence["kind"] = serde_json::Value::Null;
    assert_eq!(
        crate::research_search::producer(&knot_view),
        ProducerKind::Other
    );
    knot_view.clip.source_evidence["kind"] = serde_json::json!("passage");
    knot_view.clip.source_evidence["source_tags"] = serde_json::json!(42);
    assert_eq!(
        crate::research_search::producer(&knot_view),
        ProducerKind::Other
    );
    let mut note_view = bridge.clip_view(&context, &clips[1]).unwrap();
    assert_eq!(
        crate::research_search::producer(&note_view),
        ProducerKind::Note
    );
    note_view.clip.anchors[0].evidence["body"] = serde_json::Value::Null;
    assert_eq!(
        crate::research_search::producer(&note_view),
        ProducerKind::Other
    );
    let (_, revision) = bridge.context_revision().unwrap();
    let mut request = Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        query_revision: 1,
        selected: None,
        query: SearchQuery::default(),
        settings: SearchSettings::default(),
    };
    let (wake_tx, wake_rx) = std::sync::mpsc::channel();
    let worker = crate::research_search::Worker::spawn(
        bridge.clone(),
        actor.clone(),
        Arc::new(move || {
            let _ = wake_tx.send(());
        }),
    );
    worker.submit(request.clone());
    wake_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let completed = worker.take().unwrap();
    assert_eq!(completed.request.id, request.id);
    assert_eq!(completed.result.unwrap().total, 3);
    let mut latest = request.clone();
    for revision in 2..=12 {
        latest.id = uuid::Uuid::new_v4();
        latest.query_revision = revision;
        worker.submit(latest.clone());
    }
    loop {
        wake_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        if let Some(response) = worker.take() {
            if response.request.id == latest.id {
                assert_eq!(response.result.unwrap().total, 3);
                break;
            }
        }
    }
    worker.submit(request.clone());
    worker.cancel();
    assert!(worker.take().is_none());
    drop(worker);
    let result = crate::research_search::run(&bridge, &actor, &request).unwrap();
    assert_eq!(result.total, 3);
    assert!(matches!(result.coverage, Coverage::Incomplete { .. }));
    // Exercise real capture and sealed clip owners, including exact Unicode ranges.
    let left_input = result
        .inputs
        .iter()
        .find(|i| i.producer == ProducerKind::Capture)
        .unwrap();
    let right_input = result
        .inputs
        .iter()
        .find(|i| i.producer == ProducerKind::Selection)
        .unwrap();
    let mut comparison_request = crate::research_compare::Request {
        id: uuid::Uuid::new_v4(),
        context: context.clone(),
        revision,
        epoch: 1,
        left: crate::research::target(&left_input.source, &left_input.version).unwrap(),
        right: crate::research::target(&right_input.source, &right_input.version).unwrap(),
        settings: Default::default(),
    };
    let compared = crate::research_compare::run(&bridge, &actor, &comparison_request).unwrap();
    assert_eq!(
        compared.comparison.relationship,
        eidetic_search::compare::ComparisonRelationship::Explicit
    );
    assert_ne!(
        compared.comparison.old.source,
        compared.comparison.new.source
    );
    for (text, left) in [(&compared.left.text, true), (&compared.right.text, false)] {
        let rebuilt: String = compared
            .comparison
            .spans
            .iter()
            .filter_map(|span| {
                if left {
                    span.old.as_ref()
                } else {
                    span.new.as_ref()
                }
            })
            .flat_map(|selector| {
                text.chars()
                    .skip(selector.start)
                    .take(selector.end - selector.start)
            })
            .collect();
        assert_eq!(&rebuilt, text);
    }
    let original_left = comparison_request.left.clone();
    comparison_request.left = crate::research::Target::Capture(crate::saved_sources::Target {
        key: context.admissions[2].key.clone(),
        text_hash: original_left.text_hash().into(),
        selector: None,
    });
    assert!(
        crate::research_compare::run(&bridge, &actor, &comparison_request)
            .unwrap_err()
            .contains("Left unavailable")
    );
    comparison_request.left = original_left;
    comparison_request.settings.max_diff_work = 0;
    assert!(
        crate::research_compare::run(&bridge, &actor, &comparison_request)
            .unwrap_err()
            .contains("refused")
    );
    comparison_request.settings = Default::default();
    let mut changed = context.clone();
    changed.clips.retain(|clip| clip != &clips[0]);
    bridge.project(changed);
    bridge.project(context.clone());
    assert!(
        crate::research_compare::run(&bridge, &actor, &comparison_request)
            .unwrap_err()
            .contains("admissions changed")
    );
    bridge.project(context.clone());
    let (_, current_revision) = bridge.context_revision().unwrap();
    request.revision = current_revision;
    for kind in [
        ProducerKind::Capture,
        ProducerKind::Selection,
        ProducerKind::Note,
    ] {
        assert!(result.hits.iter().any(|hit| hit.producer == kind));
    }
    // Two individually admissible capture metadata records exceed the owner budget together.
    let second_capture = crate::saved_sources::Admission {
        key: crate::saved_sources::Key {
            node: uuid::Uuid::new_v4(),
            ..admission.key.clone()
        },
        title: "Second capture with long metadata".repeat(8),
        ..admission.clone()
    };
    let mut owner_context = context.clone();
    owner_context.admissions = vec![admission.clone(), second_capture.clone()];
    let load_owner = |keys: Vec<crate::saved_sources::Key>, budget: Option<usize>| {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        assert!(actor.command(TrailCommand::ResearchSnapshot {
            context: owner_context.clone(),
            selected: Some(keys),
            max_chars: Some(1_000_000),
            max_metadata_chars: budget,
            budget: None,
            reply
        }));
        receive.recv_timeout(Duration::from_secs(5)).unwrap()
    };
    let first = load_owner(vec![admission.key.clone()], None)
        .unwrap()
        .remove(0);
    let second = load_owner(vec![second_capture.key.clone()], None)
        .unwrap()
        .remove(0);
    let first_cost = crate::research_search::capture_metadata_cost(&first).unwrap();
    let second_cost = crate::research_search::capture_metadata_cost(&second).unwrap();
    let budget = first_cost.max(second_cost);
    assert!(load_owner(vec![admission.key.clone()], Some(budget)).is_ok());
    assert!(load_owner(vec![second_capture.key.clone()], Some(budget)).is_ok());
    assert!(
        load_owner(
            vec![admission.key.clone(), second_capture.key.clone()],
            Some(budget)
        )
        .unwrap_err()
        .contains("metadata")
    );
    // The clip adapter also refuses only when the combined metadata crosses the budget.
    let mut clip_costs = Vec::new();
    for clip in &clips {
        request.selected = Some(vec![Member::Clip(clip.clone())]);
        let single = crate::research_search::run(&bridge, &actor, &request).unwrap();
        let input = &single.inputs[0];
        let source = eidetic_search::query::ResearchSource {
            retained: eidetic::models::recall::RetainedSourceText {
                source: input.source.clone(),
                version: input.version.clone(),
                title: input.title.clone(),
                text: String::new(),
            },
            producer: input.producer.clone(),
            address: input.address.clone(),
            date: input.date,
            tags: input.tags.clone(),
        };
        clip_costs.push(crate::research_search::source_metadata_cost(&source).unwrap());
    }
    let generation_cost = format!("{}:{}", request.context.generation, request.revision)
        .chars()
        .count();
    let clip_budget = generation_cost + *clip_costs.iter().max().unwrap();
    request.settings.max_metadata_chars = clip_budget;
    for clip in &clips {
        request.selected = Some(vec![Member::Clip(clip.clone())]);
        assert!(crate::research_search::run(&bridge, &actor, &request).is_ok());
    }
    request.selected = Some(clips.iter().cloned().map(Member::Clip).collect());
    assert!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap_err()
            .contains("while loading retained clips")
    );
    request.selected = None;
    request.settings.max_metadata_chars = SearchSettings::default().max_metadata_chars;
    let original_budget = request.settings.max_metadata_chars;
    request.settings.max_metadata_chars = 100;
    assert!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap_err()
            .contains("metadata")
    );
    request.selected = Some(vec![Member::Clip(clips[0].clone())]);
    assert!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap_err()
            .contains("metadata")
    );
    request.selected = None;
    request.settings.max_metadata_chars = original_budget;
    request.settings.limit = 1;
    let first = crate::research_search::run(&bridge, &actor, &request).unwrap();
    request.settings.offset = 1;
    let second = crate::research_search::run(&bridge, &actor, &request).unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(second.total, 3);
    assert_ne!(first.hits[0].source, second.hits[0].source);
    request.settings.limit = 50;
    request.settings.offset = 0;
    request.query.kinds = vec![ProducerKind::Note];
    assert_eq!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap()
            .total,
        1
    );
    request.query.kinds.clear();
    request.query.date_from = Some(1);
    let dated = crate::research_search::run(&bridge, &actor, &request).unwrap();
    assert_eq!(dated.total, 1);
    assert_eq!(dated.hits[0].producer, ProducerKind::Capture);
    request.query.date_from = None;
    request.query.tags = vec![eidetic_search::query::TagId {
        owner: "not asserted".into(),
        concept: "unknown".into(),
    }];
    assert_eq!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap()
            .total,
        0
    );
    request.query.tags.clear();
    request.query.mode = QueryMode::Phrase("café →".into());
    let result = crate::research_search::run(&bridge, &actor, &request).unwrap();
    let hit = result
        .hits
        .iter()
        .find(|hit| hit.producer == ProducerKind::Selection)
        .unwrap();
    let passage = hit.passage.as_ref().unwrap();
    assert!(passage.text.contains("café →"));
    let mut target = crate::research::target(&hit.source, &hit.version).unwrap();
    target.set_selector(crate::saved_sources::Selector {
        start: passage.selector.start,
        end: passage.selector.end,
    });
    let crate::research::Target::Clip(target) = target else {
        panic!("selection hit must open the retained clip")
    };
    let view = bridge.clip_view(&context, &target.clip).unwrap();
    assert!(crate::research::clip_document(&view, &target).is_ok());
    request.query.mode = QueryMode::Terms("unique".into());
    let result = crate::research_search::run(&bridge, &actor, &request).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert!(result.hits[0].passage.is_none());
    request.selected = Some(vec![]);
    request.query.mode = QueryMode::Browse;
    assert_eq!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap()
            .total,
        0
    );
    request.selected = Some(vec![Member::Clip(clips[0].clone())]);
    assert_eq!(
        crate::research_search::run(&bridge, &actor, &request)
            .unwrap()
            .total,
        1
    );
    bridge.project(Context {
        clips: vec![],
        ..context.clone()
    });
    bridge.project(context);
    assert!(crate::research_search::run(&bridge, &actor, &request).is_err());
}
