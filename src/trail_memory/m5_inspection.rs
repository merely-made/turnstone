// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[path = "m5_inventory.rs"]
mod inventory;

#[path = "fault_helper.rs"]
mod missing_fault;

pub(super) fn inject_missing_capture_copy() {
    missing_fault::inject();
}

pub(super) fn verify_missing_capture_copy() {
    missing_fault::verify_preservation();
}

/// Inspect only an already complete modern graph. Never acquire a writer
/// lease or invoke migration, recovery, or image externalization.
fn strict_inspection_graph(root: &std::path::Path) -> Result<mere::kernel::graph::Graph, String> {
    let facets_path = pandect::node_facets_path(root);
    if !crate::session_persistence::require_complete_file(&facets_path)
        .map_err(|e| e.to_string())?
    {
        return Err("canonical facets missing".into());
    }
    let facets = pandect::load_node_facets(root)
        .map_err(|e| e.to_string())?
        .ok_or("canonical facets missing")?;
    let graph_path = root.join(pandect::session_graph_store::GRAPH_FILE);
    if !crate::session_persistence::require_complete_file(&graph_path).map_err(|e| e.to_string())? {
        return Err("session graph missing".into());
    }
    let mut input = crate::session_graph_file::load_profiled_snapshot(&graph_path)
        .map_err(|e| e.to_string())?
        .ok_or("session graph missing")?;
    if input.snapshot.legacy_node_facet_count() != 0
        || input.snapshot.legacy_image_count() != 0
        || !crate::surface_controls::extract_legacy(&mut input.snapshot).is_empty()
    {
        return Err("session graph requires migration".into());
    }
    let mut graph =
        pandect::graph_placement::materialize_snapshot(&input.snapshot, input.placement)?;
    graph.install_facets(facets).map_err(|e| e.to_string())?;
    Ok(graph)
}

pub(super) fn inspect_exited_structured_copy() {
    use eidetic::records::LifecycleState;
    use std::collections::HashMap;
    let root = PathBuf::from(std::env::var_os("TURNSTONE_M5_INSPECTION_COPY").expect("copy root"));
    assert_eq!(
        std::fs::read(root.join("m5-inspection-copy-marker")).unwrap(),
        b"m5-isolated-copy/v1\n"
    );
    let endpoint = std::env::var("GRAPHSHELL_APP_ENDPOINT").expect("isolated fixture endpoint");
    let fixture_root =
        PathBuf::from(std::env::var_os("TURNSTONE_M5_FIXTURE_ROOT").expect("fixture root"));
    assert_eq!(
        std::fs::read(fixture_root.join("fixture-marker")).unwrap(),
        b"eidetic-retention-20261019\n"
    );
    let socket = PathBuf::from(&endpoint);
    assert!(socket.is_absolute(), "fixture endpoint must be absolute");
    let fixture_root = fixture_root.canonicalize().expect("fixture root");
    let socket = socket.canonicalize().expect("fixture endpoint");
    assert!(
        socket.starts_with(&fixture_root),
        "endpoint outside fixture root"
    );
    let sealer = crate::identity::RootIdentity::Custody(
        djinn::door::custody_identity::CustodyIdentity::connect_at(&endpoint, "turnstone")
            .expect("fixture custody"),
    );
    let observation: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("TURNSTONE_M5_STRUCTURED_OBSERVATION").expect("observation"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(observation["schema"], "turnstone.structured-observation/v1");
    let session: uuid::Uuid = serde_json::from_value(observation["session"].clone()).unwrap();
    let seed_observation: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("TURNSTONE_M5_SEED_OBSERVATION").expect("seed observation"),
        )
        .expect("seed observation unavailable"),
    )
    .expect("seed observation invalid");
    assert_eq!(
        seed_observation["schema"],
        "turnstone.structured-observation/v1"
    );
    assert_eq!(seed_observation["session"], observation["session"]);
    assert_eq!(seed_observation["historical"], false);
    assert_eq!(observation["historical"], true);
    assert_eq!(
        observation["source_selection"]["mode"],
        "unknown-historical"
    );
    assert!(matches!(
        seed_observation["source_selection"]["mode"].as_str(),
        Some("all" | "chosen")
    ));
    assert_eq!(
        seed_observation["content_manifest"],
        observation["content_manifest"]
    );
    assert_eq!(seed_observation["result"], observation["result"]);
    let manifest: eidetic::ManifestId =
        serde_json::from_value(observation["content_manifest"].clone()).unwrap();
    let memory = root
        .join("sessions")
        .join(session.to_string())
        .join("memory");
    let sealed = super::sealed_memory_dir(&memory);
    assert!(sealed.is_dir(), "sealed session owner store missing");
    let mut store = FjallStore::open(sealed).unwrap();
    let graph_revisions = observation["graph_revisions"]
        .as_array()
        .expect("graph observations");
    assert_eq!(
        graph_revisions.len(),
        1,
        "M5 inspector requires one session graph"
    );
    let graph_id: uuid::Uuid = serde_json::from_value(graph_revisions[0]["graph"].clone()).unwrap();
    let session_dir = root.join("sessions").join(session.to_string());
    let graph = strict_inspection_graph(&session_dir)
        .expect("copied graph unavailable or requires migration");
    let bindings =
        crate::research_record_graph::bindings(&graph, true).expect("graph record bindings");
    let result_bindings =
        crate::research_record_graph::result_bindings(&graph).expect("graph result bindings");
    assert!(
        result_bindings
            .iter()
            .any(|binding| binding.manifest == manifest),
        "result is not bound to copied graph"
    );
    let held = super::sealed_records::load_result(&mut store, &sealer, manifest)
        .expect("saved result unavailable or corrupt");
    assert_eq!(
        crate::research_records::observation_result(&held),
        observation["result"],
        "persisted result projection differs from UI observation"
    );
    let mut records = HashMap::new();
    for lifecycle in &held.snapshot.lifecycle {
        assert_eq!(lifecycle.state, LifecycleState::Active);
        let (owner_session, graph, statement, id) =
            crate::research_records_worker::parse_lifecycle(&lifecycle.lifecycle_ref).unwrap();
        assert_eq!(owner_session, session);
        assert_eq!(graph, graph_id, "record belongs to another graph");
        let (record, definition) = super::sealed_records::load_record(&mut store, &sealer, id)
            .expect("record unavailable or corrupt");
        assert_eq!(record.id, lifecycle.record_id);
        let matching: Vec<_> = bindings
            .iter()
            .filter(|binding| {
                binding.manifest == id
                    && binding.statement_id == statement
                    && binding.entity.to_string() == record.entity
                    && binding.author == record.author
            })
            .collect();
        assert_eq!(
            matching.len(),
            1,
            "record graph binding unavailable or ambiguous"
        );
        let withdrawn = matching[0].withdrawn;
        assert!(
            !withdrawn || observation["historical"] == true,
            "withdrawn record included outside historical scope"
        );
        assert!(held.snapshot.definitions.contains(&definition));
        assert!(records.insert(record.id.clone(), serde_json::json!({
            "record":crate::research_records::observation_record(&record),"definition":definition,"graph":graph,"statement":statement,"manifest":id,"withdrawn":withdrawn
        })).is_none(), "duplicate record ID");
    }
    assert_eq!(records.len(), held.snapshot.records.len());
    for record in &held.snapshot.records {
        assert_eq!(
            records.get(&record.id).unwrap()["record"],
            crate::research_records::observation_record(record)
        );
    }
    if let Some(nonce) = observation["request_id"].as_str() {
        let nonce = uuid::Uuid::parse_str(nonce).unwrap();
        let receipt =
            super::sealed_result_receipts::read_result_receipt(&mut store, &sealer, nonce)
                .expect("result receipt corrupt")
                .expect("result receipt missing");
        assert!(receipt.saved && receipt.deposited && !receipt.abandoned);
        assert_eq!(receipt.request.session, session);
        assert_eq!(receipt.request.manifest, manifest);
    }
    // Reopen exact admitted capture IDs, including their authenticated raw
    // response and canonical text, from the same sealed owner copy.
    let sources: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var_os("TURNSTONE_M5_SOURCE_OBSERVATION").expect("source observation"),
        )
        .unwrap(),
    )
    .unwrap();
    let source_session: uuid::Uuid = serde_json::from_value(sources["session"].clone()).unwrap();
    assert_eq!(source_session, session);
    let admissions: Vec<crate::saved_sources::Admission> =
        serde_json::from_value(sources["admissions"].clone()).unwrap();
    assert!(!admissions.is_empty(), "no admitted capture exercised");
    let clips: Vec<crate::research::ClipAdmission> =
        serde_json::from_value(sources["retained_admissions"].clone()).unwrap();
    let members = inventory::graph_members(&graph, session, graph_id, &admissions, &clips)
        .expect("graph source enumeration differs");
    let selected = inventory::selected_members(&seed_observation["source_selection"], &members)
        .expect("seed source selection invalid");
    let mut input_sources = Vec::new();
    let mut captures = Vec::new();
    for admission in &admissions {
        assert_eq!(*admission.key.session.as_uuid(), session);
        assert_eq!(*admission.key.graph.as_uuid(), graph_id);
        let (_, node) = graph
            .get_node_by_id(admission.key.node)
            .expect("admitted node missing");
        let refs = graph
            .facets()
            .get(
                &node.id,
                &chartulary::FacetId::new(crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
            )
            .and_then(|value| value.as_array())
            .expect("capture facet missing");
        assert_eq!(
            refs.get(admission.occurrence)
                .and_then(|value| value.as_str()),
            Some(admission.key.annotation_manifest.as_str())
        );
        assert!(
            admission.key.is_sealed(),
            "M5 capture must use sealed owner"
        );
        let content = super::sealed_capture::content(&mut store, &sealer, admission)
            .expect("capture bytes unavailable or corrupt");
        let source_selected = selected.contains(&crate::research_search::Member::Capture(
            admission.key.clone(),
        ));
        if source_selected {
            // The current research worker reads the sealed owner directly;
            // the legacy saved-source cache need not contain an entry. The
            // full owner-derived inventory is checked against the saved result
            // below, including versions and record evidence hashes.
            let target = crate::research::Target::Capture(crate::saved_sources::Target {
                key: admission.key.clone(),
                text_hash: content.text_hash.clone(),
                selector: None,
            });
            input_sources.push((
                eidetic::records::SourceRecord {
                    source: crate::research::source_id(&target).expect("capture source identity"),
                    version: content.text_hash.clone(),
                    producer: "capture".into(),
                    captured_at: i64::try_from(content.captured_at_ms)
                        .ok()
                        .map(|ms| ms.div_euclid(1000)),
                    tags: vec![],
                },
                content.text.clone(),
            ));
        }
        captures.push(inventory::capture_evidence(
            admission,
            &content,
            source_selected,
        ));
    }
    let mut selections = Vec::new();
    let mut notes = Vec::new();
    for admission in &clips {
        assert_eq!(*admission.session.as_uuid(), session);
        assert_eq!(*admission.graph.as_uuid(), graph_id);
        let (_, node) = graph
            .get_node_by_id(admission.node)
            .expect("clip node missing");
        assert_eq!(
            node.url(),
            crate::retained_clip::address(admission.manifest)
        );
        let held = crate::retained_clip::read_sealed(
            &root,
            admission.manifest,
            crate::retained_clip::limits().unwrap(),
            &sealer,
        )
        .expect("retained source unavailable or corrupt");
        let producer = crate::research_search::producer(&held);
        if selected.contains(&crate::research_search::Member::Clip(admission.clone())) {
            let crate::retained_clip::ClipBody::Text(text) = &held.body else {
                panic!("non-text retained source")
            };
            let hash = eidetic::Hash::of(text.as_bytes()).to_string();
            let target = crate::research::Target::Clip(crate::research::ClipTarget {
                clip: admission.clone(),
                text_hash: hash.clone(),
                selector: None,
            });
            let producer_name = match producer {
                eidetic_search::query::ProducerKind::Selection => "selection",
                eidetic_search::query::ProducerKind::Note => "note",
                _ => panic!("unexpected retained producer in M5 scope"),
            };
            input_sources.push((
                eidetic::records::SourceRecord {
                    source: crate::research::source_id(&target).expect("clip source identity"),
                    version: hash,
                    producer: producer_name.into(),
                    captured_at: None,
                    tags: held
                        .clip
                        .tags
                        .iter()
                        .map(|tag| eidetic::records::TagIdentity {
                            owner: tag.owner.clone(),
                            concept: tag.concept.clone(),
                        })
                        .collect(),
                },
                text.clone(),
            ));
        }
        match producer {
            eidetic_search::query::ProducerKind::Selection => selections.push((admission, held)),
            eidetic_search::query::ProducerKind::Note => notes.push((admission, held)),
            _ => panic!("unexpected retained producer in M5 scope"),
        }
    }
    // Enumerate graph-owned assertions independently, including the single
    // later withdrawal in this fixed M5 corpus. A source outside the recorded
    // selected scope is filtered by check_snapshot after it is authenticated.
    let mut input_records = Vec::new();
    for binding in &bindings {
        let (record, definition) =
            super::sealed_records::load_record(&mut store, &sealer, binding.manifest)
                .expect("graph-owned record unavailable or corrupt");
        assert_eq!(record.entity, binding.entity.to_string());
        assert_eq!(record.author, binding.author);
        assert_eq!(
            record.method,
            "https://merely.computer/ns/research#manual-property-v1"
        );
        input_records.push((record, definition));
    }
    inventory::check_snapshot(&held, input_sources, input_records)
        .expect("complete graph/owner inputs differ from saved snapshot");
    assert_eq!(
        selections.len(),
        1,
        "expected one actual selected-text admission"
    );
    assert_eq!(
        notes.len(),
        1,
        "expected one actual authored note admission"
    );
    let (selection_admission, view) = selections.pop().unwrap();
    let expected_note = std::env::var("TURNSTONE_M5_NOTE_TEXT").expect("authored note text");
    let (note_admission, note) = notes.pop().unwrap();
    let crate::retained_clip::ClipBody::Text(note_text) = &note.body else {
        panic!("note is not text")
    };
    assert_eq!(note_text, &expected_note);
    let quote = std::env::var("TURNSTONE_M5_SELECTED_QUOTE").expect("selected quote");
    let crate::retained_clip::ClipBody::Text(text) = &view.body else {
        panic!("selected clip is not text")
    };
    assert_eq!(text, &quote);
    let package_bytes =
        std::fs::read(std::env::var_os("TURNSTONE_M5_SELECTED_PACKAGE").expect("selected package"))
            .unwrap();
    let package: fleece::portable_clip::ClipPackageV1 =
        serde_json::from_slice(&package_bytes).expect("invalid package");
    assert_eq!(package.clip.clip_id, view.clip.clip_id);
    assert_eq!(package.clip.anchors, view.clip.anchors);
    assert_eq!(package.payloads.len(), 1);
    assert_eq!(package.payloads[0].bytes, quote.as_bytes());
    assert_eq!(
        package.payloads[0].id,
        eidetic::ManifestId::of_blob(quote.as_bytes())
    );
    let original = PathBuf::from(
        std::env::var_os("TURNSTONE_M5_SOURCE_PROFILE").expect("source profile path"),
    );
    let address = url::Url::parse(&view.clip.source.entity).unwrap();
    let path = address
        .to_file_path()
        .expect("Knot source must be a file address");
    let rel = path
        .strip_prefix(&original)
        .expect("Knot source outside isolated profile");
    assert!(
        rel.components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    );
    let source = std::fs::read(root.join(rel)).expect("copied Knot source missing");
    // Verify the private backing itself, not just the live source file. The
    // selected-only export must contain none of this undisclosed part.
    let mut clip_store = FjallStore::open(root.join("retained-clips")).unwrap();
    let retained = pollster::block_on(eidetic::load_typed_sealed::<
        eidetic::clip_profile::RetainedClipV1,
    >(
        &mut clip_store,
        &mut eidetic::NoFetcher,
        Some(&sealer),
        selection_admission.manifest,
    ))
    .expect("retained clip read")
    .expect("retained clip missing");
    assert_eq!(
        retained.private_backing.len(),
        1,
        "expected exact private backing"
    );
    let backing = &retained.private_backing[0];
    assert_eq!(backing.role, "source-backing");
    let backing_bytes =
        pollster::block_on(eidetic::load_typed_sealed::<eidetic::models::OpaqueBlob>(
            &mut clip_store,
            &mut eidetic::NoFetcher,
            Some(&sealer),
            backing.manifest,
        ))
        .expect("private backing read")
        .expect("private backing missing");
    assert_eq!(
        backing_bytes.0, source,
        "private backing differs from actual Knot source"
    );
    assert!(
        !package
            .payloads
            .iter()
            .any(|payload| payload.id == backing.manifest)
    );
    assert_eq!(view.clip.anchors.len(), 1);
    let anchor = &view.clip.anchors[0];
    assert_eq!(anchor.units, "utf8-bytes");
    let start = anchor.selector["byte_span"]["start"].as_u64().unwrap() as usize;
    let end = anchor.selector["byte_span"]["end"].as_u64().unwrap() as usize;
    assert_eq!(source.get(start..end), Some(quote.as_bytes()));
    assert_eq!(anchor.selector["exact_quote"], quote);
    assert_eq!(
        anchor.selector["source_hash"],
        blake3::hash(&source).to_hex().to_string()
    );
    for canary in ["M5 private preface zebra", "M5 private suffix otter"] {
        assert!(
            source
                .windows(canary.len())
                .any(|bytes| bytes == canary.as_bytes())
        );
        assert!(
            !package_bytes
                .windows(canary.len())
                .any(|bytes| bytes == canary.as_bytes())
        );
        assert!(!package.payloads.iter().any(|payload| {
            payload
                .bytes
                .windows(canary.len())
                .any(|bytes| bytes == canary.as_bytes())
        }));
    }
    let receipt_inventory = super::sealed_capture::receipts(&mut store, &sealer)
        .expect("sealed capture receipt inventory unavailable");
    let receipt_bytes = serde_json::to_vec(&receipt_inventory).expect("receipt inventory encoding");
    let selection_selected = selected.contains(&crate::research_search::Member::Clip(
        selection_admission.clone(),
    ));
    let note_selected = selected.contains(&crate::research_search::Member::Clip(
        note_admission.clone(),
    ));
    let selection_output = inventory::selection_evidence(
        &serde_json::to_value(selection_admission).unwrap(),
        serde_json::json!({"selected":true, "admission":selection_admission, "clip": view.clip, "quote": text,
            "source_relative_path": rel.to_str().expect("source path is not UTF-8"),
            "source_hash": blake3::hash(&source).to_hex().to_string(), "byte_start":start,"byte_end":end,
            "scalar_start": std::str::from_utf8(&source[..start]).unwrap().chars().count(),
            "scalar_end": std::str::from_utf8(&source[..end]).unwrap().chars().count(), "package": package}),
        &eidetic::Hash::of(text.as_bytes()).to_string(),
        selection_selected,
    );
    let note_output = inventory::note_evidence(
        &serde_json::to_value(note_admission).unwrap(),
        &serde_json::to_value(&note.clip).unwrap(),
        note_text,
        note_selected,
    );
    let output = inventory::output(
        session,
        graph_id,
        manifest,
        crate::research_records::observation_result(&held),
        serde_json::to_value(records).unwrap(),
        captures,
        note_output,
        selection_output,
        receipt_inventory.len(),
        &blake3::hash(&receipt_bytes).to_hex().to_string(),
    );
    std::fs::write(
        std::env::var_os("TURNSTONE_M5_INSPECTION_OUTPUT").expect("output"),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
}

#[cfg(test)]
mod strict_graph_tests {
    use super::*;
    #[test]
    fn model_free_inspection_graph_refuses_missing_corrupt_and_migration_without_writes() {
        let root = tempfile::tempdir().unwrap();
        assert!(strict_inspection_graph(root.path()).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        let graph_path = root.path().join(pandect::session_graph_store::GRAPH_FILE);
        std::fs::write(&graph_path, b"{bad graph").unwrap();
        assert!(strict_inspection_graph(root.path()).is_err());
        assert_eq!(std::fs::read(&graph_path).unwrap(), b"{bad graph");
        let mut graph = mere::kernel::graph::Graph::new();
        mere::kernel::graph::apply::add_node(
            &mut graph,
            None,
            "https://fixture.test".into(),
            Default::default(),
        );
        let mut snapshot = graph.to_snapshot();
        snapshot.nodes[0].tags.push("keep".into());
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        std::fs::write(&graph_path, &bytes).unwrap();
        pandect::save_node_facets(root.path(), graph.facets()).unwrap();
        let facets = std::fs::read(pandect::node_facets_path(root.path())).unwrap();
        assert!(strict_inspection_graph(root.path()).is_err());
        assert_eq!(std::fs::read(&graph_path).unwrap(), bytes);
        assert_eq!(
            std::fs::read(pandect::node_facets_path(root.path())).unwrap(),
            facets
        );
        crate::session_graph_file::save_profiled(&graph_path, &graph, None).unwrap();
        let modern = std::fs::read(&graph_path).unwrap();
        let before: std::collections::BTreeMap<_, _> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_owned(),
                    std::fs::read(&path).unwrap(),
                )
            })
            .collect();
        let inspected = strict_inspection_graph(root.path()).unwrap();
        assert_eq!(inspected.nodes().count(), graph.nodes().count());
        assert_eq!(std::fs::read(&graph_path).unwrap(), modern);
        let after: std::collections::BTreeMap<_, _> = std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_owned(),
                    std::fs::read(&path).unwrap(),
                )
            })
            .collect();
        assert_eq!(before, after);
    }
}

#[cfg(test)]
#[path = "m5_store_integrity_tests.rs"]
mod sealed_store_integrity_tests;
