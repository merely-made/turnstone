// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use apparatus_core::SourceId;
use cambium::reactive_diagnostics::{
    CauseRelation, DiagnosticDetails, DiagnosticKind, ProducerRef,
    REACTIVE_DIAGNOSTIC_SCHEMA_VERSION, ReactiveDiagnosticEvent, Subject,
};

fn limits(max_records: usize) -> apparatus_core::RetentionLimits {
    apparatus_core::RetentionLimits {
        max_records,
        max_bytes: 2_000_000,
        max_age: std::time::Duration::from_secs(60),
    }
}

fn decision(reference: ProducerRef) -> ReactiveTracePayload {
    ReactiveTracePayload {
        schema_version: crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION,
        event: ReactiveDiagnosticPayload::Cambium(ReactiveDiagnosticEvent::Decision {
            schema_version: REACTIVE_DIAGNOSTIC_SCHEMA_VERSION,
            reference,
            kind: DiagnosticKind::ComponentRebuildStarted,
            subject: None,
            details: DiagnosticDetails::default(),
        }),
        span_context: None,
        scope_cause: None,
    }
}

fn store(max_records: usize) -> ObservationStore<EventCategory> {
    ObservationStore::new(
        RunId::from("reactive-projection-test"),
        SourceId::from("turnstone.app-event-fanout"),
        limits(max_records),
    )
}

fn cause_edge(edge: u64, child: u64, parent: u64) -> ReactiveTracePayload {
    ReactiveTracePayload {
        schema_version: crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION,
        event: ReactiveDiagnosticPayload::Cambium(ReactiveDiagnosticEvent::CauseEdge {
            schema_version: REACTIVE_DIAGNOSTIC_SCHEMA_VERSION,
            reference: ProducerRef {
                producer_id: 9,
                sequence: edge,
            },
            child: ProducerRef {
                producer_id: 9,
                sequence: child,
            },
            parent: ProducerRef {
                producer_id: 9,
                sequence: parent,
            },
            relation: CauseRelation::TriggeredBy,
        }),
        span_context: None,
        scope_cause: None,
    }
}

#[test]
fn reordered_edge_resolves_as_new_observation_with_exact_parent_record() {
    let mut observations = store(32);
    let mut ingress = ReactiveIngress {
        boot_id: Some(SourceBootId::from("boot")),
        ..ReactiveIngress::default()
    };
    let now = std::time::Duration::from_secs(1);
    ingress
        .project(&mut observations, now, cause_edge(3, 2, 1))
        .unwrap();
    ingress
        .project(
            &mut observations,
            now,
            decision(ProducerRef {
                producer_id: 9,
                sequence: 2,
            }),
        )
        .unwrap();
    // Edge waits until both child and parent records exist.
    assert_eq!(ingress.pending.len(), 1);
    ingress
        .project(
            &mut observations,
            now,
            decision(ProducerRef {
                producer_id: 9,
                sequence: 1,
            }),
        )
        .unwrap();

    let parent = ingress.refs[&ProducerRef {
        producer_id: 9,
        sequence: 1,
    }]
        .clone();
    let mut cursor = observations.cursor();
    let batch = observations
        .read(&mut cursor, std::time::Duration::from_secs(2), 32)
        .unwrap();
    let resolution = batch
        .records
        .iter()
        .find(|record| {
            matches!(
                record
                    .payload
                    .reactive
                    .as_ref()
                    .map(|payload| &payload.event),
                Some(ReactiveDiagnosticPayload::CauseResolution { .. })
            )
        })
        .unwrap();
    assert_eq!(resolution.envelope.metadata.cause.as_ref(), Some(&parent));
    assert_eq!(ingress.stats.resolved_edges, 1);
}

#[test]
fn absent_parent_remains_explicitly_pending_and_store_eviction_expires_join_refs() {
    let mut observations = store(2);
    let mut ingress = ReactiveIngress {
        boot_id: Some(SourceBootId::from("boot")),
        ..ReactiveIngress::default()
    };
    let now = std::time::Duration::from_secs(1);
    ingress
        .project(&mut observations, now, cause_edge(3, 2, 1))
        .unwrap();
    ingress
        .project(
            &mut observations,
            now,
            decision(ProducerRef {
                producer_id: 9,
                sequence: 2,
            }),
        )
        .unwrap();
    assert_eq!(ingress.pending.len(), 1);
    assert!(!ingress.refs.contains_key(&ProducerRef {
        producer_id: 9,
        sequence: 1
    }));

    for sequence in 0..4 {
        let category = EventCategory {
            kind: "app-event",
            reactive: None,
        };
        let bytes = serde_json::to_vec(&category).unwrap().len();
        let _ = observations.record(category, bytes, ObservationMetadata::default(), now);
    }
    ingress.reconcile_store_floor(&observations);
    assert!(ingress.refs.is_empty());
    assert_eq!(ingress.pending.len(), 1);
}

#[test]
fn pending_edge_capacity_and_store_rejection_are_counted() {
    let mut observations = store(1_000);
    let mut ingress = ReactiveIngress {
        boot_id: Some(SourceBootId::from("boot")),
        ..ReactiveIngress::default()
    };
    let now = std::time::Duration::from_secs(1);
    for id in 0..=MAX_PENDING_EDGES as u64 {
        ingress
            .project(
                &mut observations,
                now,
                cause_edge(id + 1, id + 1000, id + 2000),
            )
            .unwrap();
    }
    assert_eq!(ingress.pending.len(), MAX_PENDING_EDGES);
    assert_eq!(ingress.stats.pending_edge_dropped, 1);

    let mut disabled = ObservationStore::new(
        RunId::from("disabled"),
        SourceId::from("turnstone.app-event-fanout"),
        limits(0),
    );
    let mut disabled_ingress = ReactiveIngress::default();
    disabled_ingress
        .project(
            &mut disabled,
            now,
            decision(ProducerRef {
                producer_id: 3,
                sequence: 1,
            }),
        )
        .unwrap();
    assert_eq!(disabled_ingress.stats.store_rejected, 1);
}

#[test]
fn attachment_rejects_wrong_run_or_boot() {
    let run = RunId::from("correct-run");
    let boot = SourceBootId::from("correct-boot");
    let (_layer, receiver) = crate::reactive_trace::channel(run.clone(), boot.clone());
    let mut ingress = ReactiveIngress::default();
    assert_eq!(
        ingress.attach(receiver, &boot, &RunId::from("other-run")),
        Err(ReactiveAttachError::WrongRun)
    );

    let (_layer, receiver) =
        crate::reactive_trace::channel(run.clone(), SourceBootId::from("other-boot"));
    assert_eq!(
        ingress.attach(receiver, &boot, &run),
        Err(ReactiveAttachError::WrongBoot)
    );
}

#[test]
fn reordered_producer_sequence_is_resolved_not_reported_as_confirmed_loss() {
    let mut ingress = ReactiveIngress::default();
    let event = |sequence| {
        decision(ProducerRef {
            producer_id: 22,
            sequence,
        })
    };
    ingress.observe_sequence(&event(1));
    ingress.observe_sequence(&event(3));
    assert_eq!(ingress.stats.producer_gap_candidates, 1);
    ingress.observe_sequence(&event(2));
    assert_eq!(ingress.stats.producer_gap_late_fills, 1);
    assert_eq!(ingress.missing.get(&22).map(VecDeque::len), Some(0));
}

#[test]
fn span_operation_becomes_scope_contains_cause_without_replacing_event_ref() {
    let mut observations = store(16);
    let mut ingress = ReactiveIngress {
        boot_id: Some(SourceBootId::from("boot")),
        ..ReactiveIngress::default()
    };
    let parent = ProducerRef {
        producer_id: 5,
        sequence: 1,
    };
    let child = ProducerRef {
        producer_id: 5,
        sequence: 2,
    };
    let mut child_payload = decision(child);
    child_payload.scope_cause = Some(parent);
    let now = std::time::Duration::from_secs(1);
    ingress
        .project(&mut observations, now, child_payload)
        .unwrap();
    ingress
        .project(&mut observations, now, decision(parent))
        .unwrap();
    let mut cursor = observations.cursor();
    let batch = observations
        .read(&mut cursor, std::time::Duration::from_secs(2), 16)
        .unwrap();
    let resolution =
        batch
            .records
            .iter()
            .find_map(|record| match &record.payload.reactive.as_ref()?.event {
                ReactiveDiagnosticPayload::CauseResolution { relation, .. } => Some(*relation),
                _ => None,
            });
    assert_eq!(resolution, Some(CauseRelation::ScopeContains));
    assert_eq!(ingress.refs.get(&child).unwrap().sequence, 1);
    assert_eq!(ingress.stats.resolved_edges, 1);
}

#[test]
fn event_subject_wins_and_scheduler_generations_are_not_semantic_revisions() {
    let mut observations = store(16);
    let mut ingress = ReactiveIngress::default();
    let payload = ReactiveTracePayload {
        schema_version: crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION,
        event: ReactiveDiagnosticPayload::Cambium(ReactiveDiagnosticEvent::Decision {
            schema_version: REACTIVE_DIAGNOSTIC_SCHEMA_VERSION,
            reference: ProducerRef {
                producer_id: 50,
                sequence: 1,
            },
            kind: DiagnosticKind::ComponentInputEvaluated,
            subject: Some(Subject {
                owner_id: 3,
                slot: 5,
                route_generation: 2,
                placement_generation: 9,
            }),
            details: DiagnosticDetails {
                revision: Some(999),
                ..Default::default()
            },
        }),
        span_context: Some(crate::reactive_trace::ReactiveSpanContext {
            subject: Some(Subject {
                owner_id: 4,
                slot: 6,
                route_generation: 8,
                placement_generation: 10,
            }),
            update_id: Some(77),
            operation: None,
        }),
        scope_cause: None,
    };
    ingress
        .project(
            &mut observations,
            std::time::Duration::from_secs(1),
            payload,
        )
        .unwrap();
    let mut cursor = observations.cursor();
    let batch = observations
        .read(&mut cursor, std::time::Duration::from_secs(2), 8)
        .unwrap();
    let metadata = &batch.records[0].envelope.metadata;
    assert_eq!(metadata.semantic_revision, None);
    assert_eq!(metadata.subject.as_ref().unwrap().id, "3:5:2:9");
    let span = batch.records[0]
        .payload
        .reactive
        .as_ref()
        .unwrap()
        .span_context
        .unwrap();
    assert_eq!(span.update_id, Some(77));
}

#[test]
fn renderer_coverage_gaps_are_relative_to_receiver_attach() {
    let run = RunId::from("renderer-run");
    let boot = SourceBootId::from("renderer-boot");
    let (_layer, receiver) = crate::reactive_trace::channel(run.clone(), boot.clone());
    crate::reactive_render::mark_input_gap();
    let mut ingress = ReactiveIngress::default();
    ingress.attach(receiver, &boot, &run).unwrap();
    crate::reactive_render::mark_input_gap();
    assert_eq!(ingress.stats(&store(32)).renderer_coverage_gaps, 1);
}
