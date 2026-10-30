// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

pub(super) fn verify_three_owners(
    root: &tempfile::TempDir,
    bridge: &Bridge,
    actor: &armillary::ActorHandle<TrailCommand>,
    context: &Context,
    request: &QueryRequest,
    lexical: &crate::research_search::Request,
    admission: &crate::saved_sources::Admission,
    session: crate::panes::SessionId,
    graph_id: crate::panes::GraphId,
    generation: uuid::Uuid,
    manifest: eidetic::ManifestId,
) {
    // Add a retained note to the real capture and sealed property fixture.
    // Each owner must fit alone; the whole query must pay one common ledger.
    let access = bridge.clip_access().unwrap();
    let mut note: serde_json::Value = serde_json::from_str(include_str!(
        "../../scenarios/fixtures/retained_clips/redshank.json"
    ))
    .unwrap();
    note["annotation"]["body"] = serde_json::json!({"kind":"text", "plain_text":"Ferry note"});
    let capture_context = fleece::portable_clip::CaptureContext {
        source: eidetic::clip_profile::ClipSourceV1 {
            owner: None,
            entity: "fixture".into(),
            revision: None,
        },
        capture_id: "fixture".into(),
        provenance: eidetic::ProvenanceRecord::self_imported("fixture"),
        trust: eidetic::TrustEnvelope::self_asserted(),
        captured_at: eidetic::Timestamp(1),
    };
    let note_id = crate::retained_clip::retain_native(
        root.path(),
        crate::retained_clip::NativeClip::Redshank {
            export: &note,
            voice_body: None,
        },
        &capture_context,
        crate::retained_clip::limits().unwrap(),
        &*access.sealer,
    )
    .unwrap();
    // Exercise the Mere reader directly so an Eidetic wrapper cannot hide a
    // typed storage failure behind a display string.
    let mut refusal_limits = crate::research_storage::StorageLimits::default();
    refusal_limits.operation_read.max_visited_items = 0;
    let clip_owner = muniment::FjallStore::open_with_limits(
        root.path().join("retained-clips"),
        refusal_limits.owner(),
    )
    .unwrap();
    let mut adapter = crate::research_storage::BudgetStore {
        store: &clip_owner,
        budget: refusal_limits.budget(),
    };
    let error = match pollster::block_on(mere_document_lanes::retained_clip::read_store(
        &mut adapter,
        Some(&*access.sealer),
        note_id,
        crate::retained_clip::limits().unwrap(),
    )) {
        Ok(_) => panic!("zero visits must refuse the retained note"),
        Err(error) => error,
    };
    assert!(matches!(
        error.storage_error(),
        Some(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::VisitedItems,
            limit: 0
        })
    ));
    drop(adapter);
    drop(clip_owner);
    let clip = crate::research::ClipAdmission {
        session,
        graph: graph_id,
        node: uuid::Uuid::new_v4(),
        manifest: note_id,
    };
    let mut mixed_context = context.clone();
    mixed_context.clips.push(clip.clone());
    bridge.project(mixed_context.clone());
    let (_, mixed_revision) = bridge.context_revision().unwrap();
    let mut mixed = request.clone();
    mixed.scope.context = mixed_context.clone();
    mixed.scope.revision = mixed_revision;
    mixed.scope.selected = Some(vec![
        Member::Capture(admission.key.clone()),
        Member::Clip(clip.clone()),
    ]);
    mixed.selected = mixed.scope.selected.clone();
    mixed.candidates = vec![mixed.candidates[0].clone()];
    let mut mixed_lexical = lexical.clone();
    mixed_lexical.context = mixed_context;
    mixed_lexical.revision = mixed_revision;
    let mut lo = 0;
    let mut hi = 1_000_000;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let mut limits = crate::research_storage::StorageLimits::default();
        limits.operation_read.max_visited_items = mid;
        bridge.set_storage_limits(Ok(limits.clone()));
        let each_source_fits = [
            Member::Capture(admission.key.clone()),
            Member::Clip(clip.clone()),
        ]
        .into_iter()
        .all(|member| {
            mixed_lexical.selected = Some(vec![member]);
            matches!(crate::research_search::snapshot(bridge, actor, &mixed_lexical), Ok(snapshot) if matches!(snapshot.coverage, eidetic_search::query::Coverage::Complete))
        });
        mixed_lexical.selected = Some(vec![
            Member::Capture(admission.key.clone()),
            Member::Clip(clip.clone()),
        ]);
        let both_sources_fit = matches!(
            crate::research_search::snapshot(bridge, actor, &mixed_lexical),
            Ok(snapshot) if matches!(snapshot.coverage, eidetic_search::query::Coverage::Complete)
        );
        let record_fits = actor_read_with_budget(
            actor,
            *session.as_uuid(),
            generation,
            manifest,
            limits.budget(),
        )
        .is_ok();
        if each_source_fits && both_sources_fit && record_fits {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    assert!(lo > 0 && lo < 1_000_000);
    let mut limits = crate::research_storage::StorageLimits::default();
    limits.operation_read.max_visited_items = lo;
    let shared = limits.budget();
    assert!(
        crate::research_search::snapshot_with_budget(bridge, actor, &mixed_lexical, shared.clone())
            .is_ok()
    );
    let error = actor_read_with_budget(actor, *session.as_uuid(), generation, manifest, shared)
        .unwrap_err();
    assert!(
        matches!(error.storage_error(), Some(muniment::StoreError::LimitExceeded {
        kind: muniment::StoreLimitKind::VisitedItems, limit
    }) if *limit == lo)
    );
    bridge.set_storage_limits(Ok(limits.clone()));
    assert!(matches!(
        crate::research_records_worker::execute(
            bridge,
            actor,
            crate::research_records::Job::Query(mixed.clone())
        ),
        crate::research_records::JobResult::Query {
            result: Err(error), ..
        } if matches!(error.storage_error(), Some(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::VisitedItems, limit
        }) if *limit == lo)
    ));
    bridge.set_storage_limits(Ok(Default::default()));
    let crate::research_records::JobResult::Query {
        result: Ok(value), ..
    } = crate::research_records_worker::execute(
        bridge,
        actor,
        crate::research_records::Job::Query(mixed.clone()),
    )
    else {
        panic!("mixed query should fit with the larger budget")
    };
    use eidetic::TypedPayload;
    let result_id = eidetic::ManifestId::of_blob(&value.serialize_to_bytes().unwrap());
    let saved = record_actor::ResultRequest {
        nonce: uuid::Uuid::new_v4(),
        session: *session.as_uuid(),
        graph: *graph_id.as_uuid(),
        entity: mixed.candidates[0].binding.entity,
        generation,
        manifest: result_id,
    };
    assert_eq!(
        actor_call(actor, *session.as_uuid(), generation, |reply| {
            record_actor::Kind::SaveResult(saved.clone(), value.clone(), reply)
        })
        .unwrap(),
        result_id
    );
    let statement = "fixture-result-assertion".to_string();
    actor_call(actor, *session.as_uuid(), generation, |reply| {
        record_actor::Kind::MarkResultAttached(
            saved.clone(),
            statement.clone(),
            serde_json::json!({"statement_id": statement}),
            reply,
        )
    })
    .unwrap();
    actor_call(actor, *session.as_uuid(), generation, |reply| {
        record_actor::Kind::AcknowledgeResult(saved, statement, reply)
    })
    .unwrap();
    bridge.set_storage_limits(Ok(limits));
    let refused = crate::research_records_worker::execute(
        bridge,
        actor,
        crate::research_records::Job::Replay {
            id: uuid::Uuid::new_v4(),
            scope: mixed.scope.clone(),
            result: result_id,
        },
    );
    assert!(matches!(
        refused,
        crate::research_records::JobResult::Replay {
            result: Err(error), ..
        } if matches!(error.storage_error(), Some(muniment::StoreError::LimitExceeded {
            kind: muniment::StoreLimitKind::VisitedItems, limit
        }) if *limit == lo)
    ));
    bridge.set_storage_limits(Ok(Default::default()));
    let replay = crate::research_records_worker::execute(
        bridge,
        actor,
        crate::research_records::Job::Replay {
            id: uuid::Uuid::new_v4(),
            scope: mixed.scope,
            result: result_id,
        },
    );
    assert!(
        matches!(replay, crate::research_records::JobResult::Replay { result: Ok(replayed), .. } if replayed == value)
    );
}
