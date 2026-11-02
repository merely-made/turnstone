// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::saved_sources::Content;
use alembic::workshop::{InstallationStatus, Proposal, RunStatus, Usage, WorkshopView};
use mere::kernel::graph::apply::{GraphDeltaResult, apply_graph_delta};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestProtection(AtomicU64);

impl Protection for TestProtection {
    fn epoch(&self) -> Option<u64> {
        match self.0.load(Ordering::SeqCst) {
            0 => None,
            epoch => Some(epoch),
        }
    }
    fn seal(&self, _: &[u8], clear: &[u8]) -> Result<Vec<u8>, athanor::workshop::Error> {
        Ok(clear.to_vec())
    }
    fn open(&self, _: &[u8], sealed: &[u8]) -> Result<Vec<u8>, athanor::workshop::Error> {
        Ok(sealed.to_vec())
    }
}

fn sealed_entry(owner: &Owner, manifest: &str, text: &str) -> Entry {
    let annotation = eidetic::Hash::of(manifest.as_bytes()).to_string();
    let key = Key {
        session: crate::panes::SessionId(Uuid::parse_str(&owner.session).unwrap()),
        graph: crate::panes::GraphId::from_uuid(Uuid::parse_str(&owner.workspace).unwrap()),
        node: Uuid::new_v4(),
        annotation_manifest: format!("sealed-v1:{annotation}"),
    };
    let admission = Admission {
        key,
        occurrence: 0,
        title: "Saved title".into(),
        url: "https://private.test/source".into(),
        error: None,
    };
    Entry {
        admission,
        content: Some(Content {
            title: "Café retained copy".into(),
            origin: "https://private.test/source".into(),
            captured_at_ms: 4,
            text: text.into(),
            text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
            raw_manifest: format!("sealed-v1:{annotation}"),
        }),
        error: None,
    }
}

fn proposal_for_test(installation: [u8; 16], run: u64, sources: &[Passage]) -> Proposal {
    let mut djot = String::from("# Evidence brief\n\n");
    for (index, passage) in sources.iter().enumerate() {
        djot.push_str(&format!("## {}. {}\n\n", index + 1, passage.source.title));
        djot.push_str("> ```\n");
        for line in passage.text.split('\n') {
            djot.push_str(&format!("> {line}\n"));
        }
        djot.push_str("> ```\n\n");
        let source_bytes = serde_json::to_vec(&passage.source).unwrap();
        let source_digest = blake3::hash(&source_bytes);
        let source_hex = source_digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        djot.push_str(&format!(
            "[Open retained source {}](alembic-source:{source_hex})\n\n",
            index + 1
        ));
    }
    let mut proposal = Proposal {
        schema: 1,
        definition: athanor::workshop::definition(),
        installation,
        run,
        sources: sources.to_vec(),
        djot,
        digest: [0; 32],
    };
    proposal.digest = *blake3::hash(&serde_json::to_vec(&proposal).unwrap()).as_bytes();
    proposal
}

#[test]
fn changed_example_body_is_refused_by_fresh_public_source_gate() {
    let mut graph = Graph::new();
    for delta in example_seed_deltas(&graph) {
        assert!(!matches!(
            apply_graph_delta(&mut graph, delta),
            GraphDeltaResult::Refused(_)
        ));
    }
    let example = EXAMPLES[0];
    let owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    assert!(fixture_resource(
        &graph,
        example.id,
        graph
            .shown_resource_id(graph.get_node_by_id(example.id).unwrap().0)
            .unwrap(),
    ));

    assert!(!matches!(
        apply_graph_delta(
            &mut graph,
            GraphDelta::ReplaySetNodeBodyById {
                node_id: example.id,
                body: Some("private replacement bytes".into()),
            },
        ),
        GraphDeltaResult::Refused(_),
    ));
    let captured = athanor::workshop_destination::capture_graph_public_source(
        &graph,
        &owner,
        example.id,
        0,
        example.body.len() as u64,
        |surface, resource| fixture_resource(&graph, surface, resource),
    );
    assert!(
        captured.is_err(),
        "changed retained bytes cannot keep fixture-public status"
    );
}

#[test]
fn citation_reads_exact_saved_proposal_passage_after_live_source_changes() {
    let mut graph = Graph::new();
    for delta in example_seed_deltas(&graph) {
        assert!(!matches!(
            apply_graph_delta(&mut graph, delta),
            GraphDeltaResult::Refused(_)
        ));
    }
    let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    let selected = [EXAMPLES[0].id.to_string(), EXAMPLES[1].id.to_string()];
    let passages = capture_public_examples(&graph, &test_owner, &selected).unwrap();
    let installation = [9; 16];
    let proposal = proposal_for_test(installation, 1, &passages);
    athanor::workshop::validate_proposal(&proposal).unwrap();
    let source = passages[0].source.clone();
    let view = WorkshopView {
        installation,
        run: Some(1),
        generation: 1,
        lifecycle: InstallationStatus::Active,
        phase: RunStatus::Completed,
        review: None,
        proposal: Some(proposal.clone()),
        usage: Usage::default(),
        limits: None,
        controls: Vec::new(),
    };

    let mut changed_graph = graph.clone();
    let mut changed_body = EXAMPLES[0].body.to_string();
    changed_body.replace_range(..1, "X");
    assert!(!matches!(
        apply_graph_delta(
            &mut changed_graph,
            GraphDelta::ReplaySetNodeBodyById {
                node_id: EXAMPLES[0].id,
                body: Some(changed_body),
            },
        ),
        GraphDeltaResult::Refused(_),
    ));
    let stale_capture = athanor::workshop_destination::capture_graph_public_source(
        &changed_graph,
        &test_owner,
        EXAMPLES[0].id,
        source.start,
        source.end,
        |surface, resource| fixture_resource(&changed_graph, surface, resource),
    );
    assert!(stale_capture.is_err());

    let reading = proposal_reading(&test_owner, &view, proposal.digest, &source, None).unwrap();
    assert_eq!(reading.text, passages[0].text);
    assert_eq!(reading.version, source.version);
    assert!(proposal_reading(&test_owner, &view, [8; 32], &source, None).is_err());

    let wrong_owner = owner("other-host", Uuid::new_v4(), Uuid::new_v4());
    assert!(proposal_reading(&wrong_owner, &view, proposal.digest, &source, None).is_err());
    let mut wrong_source = source;
    wrong_source.version = "999".into();
    assert!(proposal_reading(&test_owner, &view, proposal.digest, &wrong_source, None).is_err());
}

#[test]
fn public_source_freshness_is_bound_to_exact_session_and_graph() {
    let mut app = crate::app::App::test_stub();
    let graph_id = app.graph_runtimes.active_graph();
    app.graph_runtimes
        .get_mut(graph_id)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            for delta in example_seed_deltas(graph) {
                assert!(!matches!(
                    apply_graph_delta(graph, delta),
                    GraphDeltaResult::Refused(_)
                ));
            }
            true
        });
    let test_owner = owner(
        "turnstone-public-example",
        *graph_id.as_uuid(),
        *app.session_id.as_uuid(),
    );
    let graph = app.graph_runtimes.get(graph_id).unwrap().canvas.graph();
    let passage = athanor::workshop_destination::capture_graph_public_source(
        graph,
        &test_owner,
        EXAMPLES[0].id,
        0,
        EXAMPLES[0].body.len() as u64,
        |surface, resource| fixture_resource(graph, surface, resource),
    )
    .unwrap();
    assert!(passage_current(&app, &test_owner, &passage));

    let wrong_session = owner(&test_owner.host, *graph_id.as_uuid(), Uuid::new_v4());
    let mut wrong_session_passage = passage.clone();
    wrong_session_passage.source.owner = wrong_session.clone();
    assert!(!passage_current(
        &app,
        &wrong_session,
        &wrong_session_passage
    ));

    let wrong_graph = owner(&test_owner.host, Uuid::new_v4(), *app.session_id.as_uuid());
    let mut wrong_graph_passage = passage.clone();
    wrong_graph_passage.source.owner = wrong_graph.clone();
    assert!(!passage_current(&app, &wrong_graph, &wrong_graph_passage));
}

#[test]
fn sealed_saved_source_capture_keeps_exact_utf8_version_and_selection_order() {
    let mut graph = Graph::new();
    for delta in example_seed_deltas(&graph) {
        assert!(!matches!(
            apply_graph_delta(&mut graph, delta),
            GraphDeltaResult::Refused(_)
        ));
    }
    let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    let private_text = "Café 🌱 — exact retained bytes.";
    let entry = sealed_entry(&test_owner, "manifest-one", private_text);
    let admission = entry.admission.clone();
    let private_key = saved_source_choice_key(&entry).unwrap();
    let public_key = EXAMPLES[0].id.to_string();
    let protection = TestProtection(AtomicU64::new(1));
    let passages = capture_selected_sources(
        &graph,
        &test_owner,
        &[private_key.clone(), public_key.clone()],
        std::slice::from_ref(&entry),
        std::slice::from_ref(&admission),
        &protection,
    )
    .unwrap();
    assert_eq!(passages.len(), 2);
    assert!(!passages[0].public);
    assert_eq!(passages[0].text, private_text);
    assert_eq!(passages[0].source.start, 0);
    assert_eq!(passages[0].source.end, private_text.len() as u64);
    assert_eq!(
        passages[0].source.version,
        eidetic::Hash::of(private_text.as_bytes()).to_string()
    );
    assert!(passages[1].public);
    assert_eq!(passages[1].source.title, EXAMPLES[0].title);

    assert!(
        capture_selected_sources(
            &graph,
            &test_owner,
            &[private_key.clone(), private_key.clone()],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&admission),
            &protection,
        )
        .is_err()
    );
    assert!(
        capture_selected_sources(
            &graph,
            &test_owner,
            &[private_key.clone(), public_key.clone()],
            std::slice::from_ref(&entry),
            &[],
            &protection,
        )
        .is_err()
    );
    let wrong_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    assert!(
        capture_selected_sources(
            &graph,
            &wrong_owner,
            &[private_key, public_key],
            std::slice::from_ref(&entry),
            std::slice::from_ref(&admission),
            &protection,
        )
        .is_err()
    );
}

#[test]
fn private_source_choices_refuse_legacy_tampered_and_oversized_entries() {
    let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    let good = sealed_entry(&test_owner, "good", "Short café passage.");
    let legacy = {
        let mut entry = sealed_entry(&test_owner, "legacy", "Legacy text.");
        entry.admission.key.annotation_manifest = eidetic::Hash::of(b"legacy").to_string();
        entry
    };
    let mut tampered = sealed_entry(&test_owner, "tampered", "Original text.");
    tampered.content.as_mut().unwrap().text = "Altered text.".into();
    let oversized = sealed_entry(&test_owner, "oversized", &"x".repeat(2049));
    let mut malformed_hash = sealed_entry(&test_owner, "malformed", "Hash text.");
    malformed_hash.content.as_mut().unwrap().text_hash = "ééééééééééééé".into();
    let entries = vec![good.clone(), legacy, tampered, oversized, malformed_hash];
    let admissions: Vec<_> = entries
        .iter()
        .map(|entry| entry.admission.clone())
        .collect();
    let choices = private_source_choices(&test_owner, &entries, &admissions, true);
    assert_eq!(choices.len(), 5);
    assert!(choices[0].available);
    assert!(!choices[1].available);
    assert!(
        choices[1]
            .refusal
            .as_deref()
            .is_some_and(|reason| reason.contains("Legacy unsealed"))
    );
    assert!(!choices[2].available);
    assert_eq!(choices[2].title, "Private saved source");
    assert!(!choices[3].available);
    assert_eq!(choices[3].title, "Private saved source");
    assert!(
        choices[3]
            .refusal
            .as_deref()
            .is_some_and(|reason| reason.contains("shorter retained passage"))
    );
    assert!(!choices[4].available);
    let mut different_version = good.clone();
    let content = different_version.content.as_mut().unwrap();
    content.text = "A different version.".into();
    content.text_hash = eidetic::Hash::of(content.text.as_bytes()).to_string();
    assert_eq!(
        saved_source_choice_key(&good) == saved_source_choice_key(&different_version),
        false,
        "different immutable text versions have distinct source identities"
    );
}

#[test]
fn private_proposal_citation_is_exact_and_requires_live_custody() {
    let test_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    let entry = sealed_entry(&test_owner, "cited", "Café citation text.");
    let protection = TestProtection(AtomicU64::new(1));
    let source_key = saved_source_choice_key(&entry).unwrap();
    let mut graph = Graph::new();
    for delta in example_seed_deltas(&graph) {
        assert!(!matches!(
            apply_graph_delta(&mut graph, delta),
            GraphDeltaResult::Refused(_)
        ));
    }
    let passages = capture_selected_sources(
        &graph,
        &test_owner,
        &[source_key, EXAMPLES[0].id.to_string()],
        std::slice::from_ref(&entry),
        std::slice::from_ref(&entry.admission),
        &protection,
    )
    .unwrap();
    let private_passage = passages[0].clone();
    let public_passage = passages[1].clone();
    let proposal = proposal_for_test([9; 16], 1, &passages);
    athanor::workshop::validate_private_proposal(&proposal).unwrap();
    let view = WorkshopView {
        installation: proposal.installation,
        run: Some(proposal.run),
        generation: 1,
        lifecycle: InstallationStatus::Active,
        phase: RunStatus::Completed,
        review: None,
        proposal: Some(proposal.clone()),
        usage: Usage::default(),
        limits: None,
        controls: Vec::new(),
    };
    let reading = proposal_reading(
        &test_owner,
        &view,
        proposal.digest,
        &private_passage.source,
        Some(&protection),
    )
    .unwrap();
    assert_eq!(reading.text, private_passage.text);
    assert!(
        proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            None,
        )
        .is_err()
    );
    let public_reading = proposal_reading(
        &test_owner,
        &view,
        proposal.digest,
        &public_passage.source,
        None,
    )
    .unwrap();
    assert_eq!(public_reading.text, public_passage.text);
    let wrong_owner = owner("turnstone-public-example", Uuid::new_v4(), Uuid::new_v4());
    assert!(
        proposal_reading(
            &wrong_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            Some(&protection),
        )
        .is_err()
    );
    protection.0.store(0, Ordering::SeqCst);
    assert!(
        proposal_reading(
            &test_owner,
            &view,
            proposal.digest,
            &private_passage.source,
            Some(&protection),
        )
        .is_err()
    );
}

#[test]
fn private_passage_freshness_requires_the_current_saved_source_admission() {
    let mut app = crate::app::App::test_stub();
    let graph_id = app.graph_runtimes.active_graph();
    let member = app
        .graph_runtimes
        .visit("https://private.test/retained-source");
    let node = app.graph_runtimes.graph().get_node(member).unwrap().id;
    let mut entry = sealed_entry(
        &owner(
            "turnstone-public-example",
            *graph_id.as_uuid(),
            *app.session_id.as_uuid(),
        ),
        "attached",
        "Private café text.",
    );
    entry.admission.key.session = app.session_id;
    entry.admission.key.graph = graph_id;
    entry.admission.key.node = node;
    let manifest = entry.admission.key.annotation_manifest.clone();
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas,
        |facets| crate::source_capture::append_reference(facets, node, manifest),
    )
    .unwrap()
    .unwrap();
    let graph_node = app.graph_runtimes.graph().get_node(member).unwrap();
    entry.admission = Admission {
        key: entry.admission.key.clone(),
        occurrence: 0,
        title: graph_node.title.clone(),
        url: graph_node.url().to_owned(),
        error: None,
    };
    app.saved_sources.entries = vec![entry.clone()];
    let current_admissions = app.saved_source_admissions();
    assert_eq!(current_admissions, vec![entry.admission.clone()]);
    let test_owner = owner(
        "turnstone-public-example",
        *graph_id.as_uuid(),
        *app.session_id.as_uuid(),
    );
    let protection = TestProtection(AtomicU64::new(1));
    let passage =
        capture_private_saved_source(&test_owner, &entry, &current_admissions, &protection)
            .unwrap();
    assert!(private_passage_current(
        &app,
        &test_owner,
        &protection,
        &passage
    ));

    let wrong_graph = owner(
        "turnstone-public-example",
        Uuid::new_v4(),
        *app.session_id.as_uuid(),
    );
    assert!(!private_passage_current(
        &app,
        &wrong_graph,
        &protection,
        &passage
    ));
    let wrong_session = owner(
        "turnstone-public-example",
        *graph_id.as_uuid(),
        Uuid::new_v4(),
    );
    assert!(!private_passage_current(
        &app,
        &wrong_session,
        &protection,
        &passage
    ));

    app.saved_sources.entries[0].content.as_mut().unwrap().text = "Changed text.".into();
    assert!(!private_passage_current(
        &app,
        &test_owner,
        &protection,
        &passage
    ));
    app.saved_sources.entries = vec![entry];
    crate::graph_facets::edit_canvas_facets(
        &mut app.graph_runtimes.get_mut(graph_id).unwrap().canvas,
        |facets| {
            facets.remove(
                &node,
                &chartulary::FacetId::new(crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
            );
            Ok::<(), String>(())
        },
    )
    .unwrap()
    .unwrap();
    assert!(!private_passage_current(
        &app,
        &test_owner,
        &protection,
        &passage
    ));
    protection.0.store(0, Ordering::SeqCst);
    assert!(!private_passage_current(
        &app,
        &test_owner,
        &protection,
        &passage
    ));
}
