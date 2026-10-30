// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use cambium::reactive_diagnostics::{
    DiagnosticDetails, DiagnosticKind, ProducerRef, REACTIVE_DIAGNOSTIC_SCHEMA_VERSION,
    ReactiveDiagnosticEvent, Subject,
};
use tracing_subscriber::prelude::*;

fn event_json() -> String {
    serde_json::to_string(&ReactiveDiagnosticEvent::Decision {
        schema_version: REACTIVE_DIAGNOSTIC_SCHEMA_VERSION,
        reference: ProducerRef {
            producer_id: 42,
            sequence: 7,
        },
        kind: DiagnosticKind::ComponentRebuildStarted,
        subject: Some(Subject {
            owner_id: 3,
            slot: 5,
            route_generation: 2,
            placement_generation: 9,
        }),
        details: DiagnosticDetails::default(),
    })
    .unwrap()
}

#[test]
fn typed_payload_is_redacted_strict_and_keeps_numeric_correlation() {
    let payload = validate_cambium_payload(event_json()).unwrap();
    let reference = producer_ref(&payload).unwrap();
    assert_eq!(reference.producer_id, 42);
    assert_eq!(reference.sequence, 7);

    let mut unknown: serde_json::Value = serde_json::from_str(&event_json()).unwrap();
    unknown["private_text"] = serde_json::json!("must-not-cross-the-bridge");
    assert!(validate_cambium_payload(serde_json::to_string(&unknown).unwrap()).is_none());
}

#[test]
fn explicit_layer_projects_events_and_numeric_span_context() {
    let run = apparatus_core::RunId::from("reactive-test-run");
    let boot = apparatus_core::SourceBootId::from("reactive-test-boot");
    let payload_json = event_json();
    let (layer, receiver) = channel(run.clone(), boot);
    let dispatch = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(dispatch, || {
        let span = cambium::reactive_diagnostics::diagnostic_span(
            Subject {
                owner_id: 3,
                slot: 5,
                route_generation: 2,
                placement_generation: 9,
            },
            Some(13),
            Some(ProducerRef {
                producer_id: 9,
                sequence: 4,
            }),
        );
        span.in_scope(|| {
            tracing::info!(
                target: CAMBIUM_TARGET,
                payload = payload_json.as_str(),
                "shared reactive decision"
            );
        });
    });
    let batch = receiver.try_drain(4).unwrap();
    assert_eq!(batch.packets.len(), 1);
    assert_eq!(
        batch.packets[0].correlation.as_ref().unwrap().run,
        "reactive-test-run"
    );
    assert_eq!(
        batch.packets[0].correlation.as_ref().unwrap().source,
        "turnstone.cambium-reactive"
    );
    assert_eq!(
        batch.packets[0].correlation.as_ref().unwrap().operation,
        "9:4"
    );
    let DiagnosticEvent::Event { fields, .. } = &batch.packets[0].event else {
        panic!("reactive payload should use the structured event variant");
    };
    let projected: ReactiveTracePayload = serde_json::from_str(&fields[0].value).unwrap();
    let span = projected.span_context.unwrap();
    assert_eq!(span.update_id, Some(13));
    assert_eq!(span.subject.unwrap().slot, 5);
    assert_eq!(span.operation.unwrap().sequence, 4);
    assert_eq!(projected.scope_cause.unwrap().producer_id, 9);
}

#[test]
fn actual_composition_queue_merge_keeps_notice_count_out_of_update_id() {
    use cambium::composition::ComponentKey;
    use cambium::composition_scheduler::CompositionInputs;
    use cambium::composition_scheduler::{CompositionInputQueue, DependencyStamp, InputVector};
    use std::collections::BTreeMap;

    let run = apparatus_core::RunId::from("composition-merge-run");
    let boot = apparatus_core::SourceBootId::from("composition-merge-boot");
    let (layer, receiver) = channel(run, boot);
    let queue = CompositionInputQueue::new().unwrap();
    let inputs = |revision| {
        CompositionInputs::new(BTreeMap::from([(
            ComponentKey::from("row"),
            InputVector::new(
                vec![DependencyStamp::new(7, 11, "appearance", revision).unwrap()],
                Vec::new(),
            )
            .unwrap(),
        )]))
        .unwrap()
    };
    let dispatch = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(dispatch, || {
        queue.publish(inputs(1), None).unwrap();
        queue.publish(inputs(2), None).unwrap();
    });

    let batch = receiver.try_drain(16).unwrap();
    let merged = batch.packets.iter().find_map(|packet| {
        let DiagnosticEvent::Event { fields, .. } = &packet.event else {
            return None;
        };
        let field = fields.iter().find(|field| field.name == "payload")?;
        let payload: ReactiveTracePayload = serde_json::from_str(&field.value).ok()?;
        match payload.event {
            ReactiveDiagnosticPayload::Cambium(ReactiveDiagnosticEvent::Decision {
                kind: DiagnosticKind::CompositionInputMerged,
                details,
                ..
            }) => Some(details),
            _ => None,
        }
    });
    let details = merged.expect("actual queue should emit the merged-input decision");
    assert_eq!(details.revision, Some(2));
    assert_eq!(details.notice_count, Some(2));
    assert_eq!(details.update_id, None);
}

#[test]
fn meristem_fixed_message_is_ignored_but_unknown_debug_fields_are_rejected() {
    let run = apparatus_core::RunId::from("meristem-fields-run");
    let boot = apparatus_core::SourceBootId::from("meristem-fields-boot");
    let (layer, receiver) = channel(run, boot);
    let dispatch = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(dispatch, || {
        tracing::trace!(
            target: MERISTEM_TARGET,
            schema_version = 1_u16,
            producer_id = 0_u64,
            sequence = 1_u64,
            kind = "any_view_reused",
            generation = u64::MAX,
            generation_present = false,
            path_depth = u16::MAX,
            path_depth_present = false,
            path_truncated = false,
            "meristem reactive decision"
        );
        tracing::trace!(
            target: MERISTEM_TARGET,
            schema_version = 1_u16,
            producer_id = 0_u64,
            sequence = 2_u64,
            kind = "any_view_reused",
            generation = u64::MAX,
            generation_present = false,
            path_depth = u16::MAX,
            path_depth_present = false,
            path_truncated = false,
            private = ?"must-not-cross-the-bridge",
            "meristem reactive decision"
        );
    });
    let batch = receiver.try_drain(4).unwrap();
    assert_eq!(batch.packets.len(), 2);
    let read_payload = |index: usize| {
        let DiagnosticEvent::Event { fields, .. } = &batch.packets[index].event else {
            panic!("reactive payload should use the structured event variant");
        };
        serde_json::from_str::<ReactiveTracePayload>(&fields[0].value).unwrap()
    };
    assert!(matches!(
        read_payload(0).event,
        ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::Decision {
            kind: MeristemKind::AnyViewReused,
            ..
        })
    ));
    let rejected = read_payload(1);
    assert!(matches!(
        rejected.event,
        ReactiveDiagnosticPayload::Rejected { .. }
    ));
    assert!(
        !serde_json::to_string(&rejected)
            .unwrap()
            .contains("must-not-cross-the-bridge")
    );
}

#[test]
fn disconnected_sink_is_nonblocking_and_does_not_panic() {
    let run = apparatus_core::RunId::from("failure-test-run");
    let boot = apparatus_core::SourceBootId::from("failure-test-boot");
    let payload_json = event_json();
    let (layer, receiver) = channel(run, boot);
    drop(receiver);
    let dispatch = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(dispatch, || {
        tracing::info!(
            target: CAMBIUM_TARGET,
            payload = payload_json.as_str(),
            "shared reactive decision"
        );
    });
}

#[test]
fn full_ingress_queue_records_loss_without_blocking_producers() {
    let run = apparatus_core::RunId::from("full-test-run");
    let boot = apparatus_core::SourceBootId::from("full-test-boot");
    let payload_json = event_json();
    let (layer, receiver) = channel(run, boot);
    let dispatch = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(dispatch, || {
        for _ in 0..4_500 {
            tracing::info!(
                target: CAMBIUM_TARGET,
                payload = payload_json.as_str(),
                "shared reactive decision"
            );
        }
    });
    assert!(receiver.try_stats().unwrap().loss.full > 0);
}

#[test]
fn no_subscriber_keeps_producer_decision_independent_of_tracing() {
    let producer = cambium::reactive_diagnostics::Producer::new().unwrap();
    let reference = producer
        .emit(
            DiagnosticKind::ComponentMemoSkipped,
            None,
            DiagnosticDetails::default(),
        )
        .unwrap();
    assert_eq!(reference.producer_id, producer.producer_id());
}
