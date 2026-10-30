// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, VecDeque};

use apparatus_core::{
    Admission, ObservationMetadata, ObservationStore, RecordRef, RunId, SourceBootId, SubjectRef,
};
use registry::diagnostics::{DiagnosticEvent, IngressStats};
use serde::{Deserialize, Serialize};

use super::EventCategory;
use crate::reactive_trace::{
    MeristemReactiveEvent, ReactiveDiagnosticPayload, ReactiveTracePayload, ReactiveTraceReceiver,
    producer_ref,
};
mod facts;
use facts::payload_facts;

const REACTIVE_SOURCE: &str = "turnstone.cambium-reactive";
const MAX_JOIN_REFS: usize = 4_096;
const MAX_PENDING_EDGES: usize = 256;
const MAX_DRAIN: usize = 4_096;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactiveObservationStats {
    pub attached: bool,
    pub ingress_retained: usize,
    pub ingress_bytes: usize,
    pub ingress_full_loss: u64,
    pub ingress_disabled_loss: u64,
    pub ingress_other_loss: u64,
    pub malformed_rejected: u64,
    pub projected: u64,
    pub store_rejected: u64,
    pub join_refs: usize,
    pub join_evicted: u64,
    pub pending_edges: usize,
    pub pending_edge_dropped: u64,
    pub resolved_edges: u64,
    pub causes_omitted: u64,
    pub producer_gap_candidates: u64,
    pub producer_gap_late_fills: u64,
    pub producer_tracker_evicted: u64,
    pub producer_missing_ranges: Vec<ProducerMissingRange>,
    pub renderer_scope: RendererCoverageScope,
    pub renderer_coverage_gaps: u64,
}

/// Fixed scope of actual renderer calls represented by the stage events.
/// These events cover Settings pane scenes and outer-frame resources only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RendererCoverageScope {
    #[default]
    SettingsPaneAndOuterFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerMissingRange {
    pub producer_id: u64,
    pub first_missing: u64,
    pub last_missing: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactiveAttachError {
    WrongBoot,
    AlreadyAttached,
    WrongRun,
}

#[derive(Default)]
pub(super) struct ReactiveIngress {
    receiver: Option<ReactiveTraceReceiver>,
    boot_id: Option<SourceBootId>,
    refs: BTreeMap<cambium::reactive_diagnostics::ProducerRef, RecordRef>,
    ref_order: VecDeque<cambium::reactive_diagnostics::ProducerRef>,
    pending: VecDeque<PendingEdge>,
    sequence_max: BTreeMap<u64, u64>,
    missing: BTreeMap<u64, VecDeque<(u64, u64)>>,
    stats: ReactiveObservationStats,
    ingress_stats: Option<IngressStats>,
    render_gap_base: u64,
}

#[derive(Clone)]
struct PendingEdge {
    edge: cambium::reactive_diagnostics::ProducerRef,
    child: cambium::reactive_diagnostics::ProducerRef,
    parent: cambium::reactive_diagnostics::ProducerRef,
    relation: cambium::reactive_diagnostics::CauseRelation,
}

impl ReactiveIngress {
    pub(super) fn attach(
        &mut self,
        receiver: ReactiveTraceReceiver,
        expected_boot: &SourceBootId,
        run: &RunId,
    ) -> Result<(), ReactiveAttachError> {
        if self.receiver.is_some() {
            return Err(ReactiveAttachError::AlreadyAttached);
        }
        if receiver.boot_id() != expected_boot {
            return Err(ReactiveAttachError::WrongBoot);
        }
        if receiver.run_id() != run {
            return Err(ReactiveAttachError::WrongRun);
        }
        // Exclude gaps accumulated by earlier runs on this UI thread.
        self.render_gap_base = crate::reactive_render::coverage_gaps();
        self.boot_id = Some(expected_boot.clone());
        self.receiver = Some(receiver);
        self.stats.attached = true;
        Ok(())
    }

    pub(super) fn stats<P>(&mut self, store: &ObservationStore<P>) -> ReactiveObservationStats {
        self.prune_refs(store);
        if let Some(receiver) = &self.receiver {
            match receiver.try_stats() {
                Ok(ingress) => self.ingress_stats = Some(ingress),
                Err(_) => {
                    self.stats.ingress_other_loss = self.stats.ingress_other_loss.saturating_add(1)
                },
            }
        }
        if let Some(ingress) = self.ingress_stats {
            self.stats.ingress_retained = ingress.retained_records;
            self.stats.ingress_bytes = ingress.retained_bytes;
            self.stats.ingress_full_loss = ingress.loss.full;
            self.stats.ingress_disabled_loss = ingress.loss.disabled;
            self.stats.ingress_other_loss = ingress
                .loss
                .oversized
                .saturating_add(ingress.loss.accounting_overflow)
                .saturating_add(ingress.loss.disconnected)
                .saturating_add(ingress.loss.contended)
                .saturating_add(ingress.loss.poisoned)
                .saturating_add(ingress.loss.sequence_exhausted)
                .saturating_add(ingress.loss.expired)
                .saturating_add(ingress.loss.evicted)
                .saturating_add(ingress.loss.discarded_on_disconnect)
                .saturating_add(ingress.loss.no_sink);
        }
        self.stats.join_refs = self.refs.len();
        self.stats.pending_edges = self.pending.len();
        self.stats.producer_missing_ranges = self
            .missing
            .iter()
            .flat_map(|(producer_id, ranges)| {
                ranges
                    .iter()
                    .map(|(first_missing, last_missing)| ProducerMissingRange {
                        producer_id: *producer_id,
                        first_missing: *first_missing,
                        last_missing: *last_missing,
                    })
            })
            .take(64)
            .collect();
        self.stats.renderer_coverage_gaps =
            crate::reactive_render::coverage_gaps().saturating_sub(self.render_gap_base);
        self.stats.clone()
    }

    pub(super) fn record_coverage(
        &mut self,
        store: &mut ObservationStore<EventCategory>,
        now: std::time::Duration,
    ) -> Result<(), ()> {
        let counters = self.stats(store);
        let payload = ReactiveTracePayload {
            schema_version: crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION,
            event: ReactiveDiagnosticPayload::Coverage { counters },
            span_context: None,
            scope_cause: None,
        };
        let category = EventCategory {
            kind: "reactive-coverage",
            reactive: Some(payload),
        };
        let bytes = serde_json::to_vec(&category).map_err(|_| ())?.len();
        let metadata = ObservationMetadata {
            source_boot: self.boot_id.clone(),
            ..ObservationMetadata::default()
        };
        store
            .record(category, bytes, metadata, now)
            .map_err(|_| ())?;
        self.prune_refs(store);
        Ok(())
    }

    pub(super) fn reconcile_store_floor<P>(&mut self, store: &ObservationStore<P>) {
        self.prune_refs(store);
    }

    pub(super) fn drain(
        &mut self,
        store: &mut ObservationStore<EventCategory>,
        now: std::time::Duration,
        max_records: usize,
    ) -> Result<(), ()> {
        self.prune_refs(store);
        let Some(receiver) = &self.receiver else {
            return Ok(());
        };
        let run_id = receiver.run_id().0.clone();
        let batch = receiver
            .try_drain(max_records.min(MAX_DRAIN))
            .map_err(|_| ())?;
        self.ingress_stats = Some(batch.stats);
        for packet in batch.packets {
            let packet_correlation = packet.correlation;
            let DiagnosticEvent::Event {
                target,
                level,
                message,
                fields,
            } = packet.event
            else {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            };
            let expected_target = match target {
                "cambium::reactive" => true,
                "meristem::reactive" => true,
                _ => false,
            };
            if !expected_target
                || !matches!(level, "TRACE" | "INFO" | "WARN")
                || message != "shared reactive observation"
            {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            let mut payload_field = None;
            let mut invalid = false;
            for field in fields {
                if field.name != "payload" || payload_field.is_some() || field.value.len() > 2_048 {
                    invalid = true;
                    break;
                }
                payload_field = Some(field.value);
            }
            if invalid {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            let Some(json) = payload_field else {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            };
            let Ok(payload) = serde_json::from_str::<ReactiveTracePayload>(&json) else {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            };
            if payload.schema_version != crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            let target_matches_payload = matches!(
                (target, &payload.event),
                ("cambium::reactive", ReactiveDiagnosticPayload::Cambium(_))
                    | ("meristem::reactive", ReactiveDiagnosticPayload::Meristem(_))
                    | (_, ReactiveDiagnosticPayload::Rejected { .. })
            );
            if !target_matches_payload {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            if matches!(payload.event, ReactiveDiagnosticPayload::Rejected { .. }) {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            if let ReactiveDiagnosticPayload::Cambium(
                cambium::reactive_diagnostics::ReactiveDiagnosticEvent::CausesOmitted {
                    omitted_count,
                    ..
                },
            ) = &payload.event
            {
                self.stats.causes_omitted = self
                    .stats
                    .causes_omitted
                    .saturating_add(u64::from(*omitted_count));
            }
            let expected_correlation = payload
                .span_context
                .and_then(|context| context.operation)
                .or_else(|| producer_ref(&payload.event))
                .map(|reference| registry::diagnostics::DiagnosticCorrelation {
                    run: run_id.clone(),
                    source: REACTIVE_SOURCE.to_owned(),
                    operation: format!("{}:{}", reference.producer_id, reference.sequence),
                });
            if packet_correlation != expected_correlation {
                self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
                continue;
            }
            self.observe_sequence(&payload);
            self.project(store, now, payload)?;
        }
        Ok(())
    }

    fn project(
        &mut self,
        store: &mut ObservationStore<EventCategory>,
        now: std::time::Duration,
        payload: ReactiveTracePayload,
    ) -> Result<(), ()> {
        let (producer_ref, subject, revision, edge) = payload_facts(&payload.event);
        let scope_edge = payload.scope_cause.and_then(|parent| {
            producer_ref.map(|child| PendingEdge {
                edge: child,
                child,
                parent,
                relation: cambium::reactive_diagnostics::CauseRelation::ScopeContains,
            })
        });
        let cause = edge
            .as_ref()
            .and_then(|edge| self.refs.get(&edge.parent).cloned());
        let cause = cause.or_else(|| {
            scope_edge
                .as_ref()
                .and_then(|edge| self.refs.get(&edge.parent).cloned())
        });
        let metadata = ObservationMetadata {
            source_boot: self.boot_id.clone(),
            operation: payload
                .span_context
                .and_then(|span| span.operation)
                .or(producer_ref)
                .map(|reference| {
                    format!("{}:{}", reference.producer_id, reference.sequence).into()
                }),
            cause,
            subject: subject
                .or_else(|| payload.span_context.and_then(|span| span.subject))
                .map(|subject| SubjectRef {
                    namespace: "cambium.component".to_owned(),
                    id: format!(
                        "{}:{}:{}:{}",
                        subject.owner_id,
                        subject.slot,
                        subject.route_generation,
                        subject.placement_generation
                    ),
                }),
            semantic_revision: revision,
            ..ObservationMetadata::default()
        };
        let category = EventCategory {
            kind: "reactive-event",
            reactive: Some(payload.clone()),
        };
        let bytes = serde_json::to_vec(&category).map_err(|_| ())?.len();
        let admission = store
            .record(category, bytes, metadata, now)
            .map_err(|_| ())?;
        let Admission::Retained(record_ref) = admission else {
            self.stats.store_rejected = self.stats.store_rejected.saturating_add(1);
            return Ok(());
        };
        self.stats.projected = self.stats.projected.saturating_add(1);
        if let Some(reference) = producer_ref {
            self.insert_ref(reference, record_ref.clone(), store);
        }
        if let Some(edge) = edge {
            if self.pending.len() >= MAX_PENDING_EDGES {
                self.pending.pop_front();
                self.stats.pending_edge_dropped = self.stats.pending_edge_dropped.saturating_add(1);
            }
            self.pending.push_back(edge);
        }
        if let Some(edge) = scope_edge {
            if self.pending.len() >= MAX_PENDING_EDGES {
                self.pending.pop_front();
                self.stats.pending_edge_dropped = self.stats.pending_edge_dropped.saturating_add(1);
            }
            self.pending.push_back(edge);
        }
        self.resolve_pending(store, now)?;
        Ok(())
    }

    fn insert_ref<P>(
        &mut self,
        reference: cambium::reactive_diagnostics::ProducerRef,
        record: RecordRef,
        store: &ObservationStore<P>,
    ) {
        if self.refs.contains_key(&reference) {
            self.stats.malformed_rejected = self.stats.malformed_rejected.saturating_add(1);
            return;
        }
        self.refs.insert(reference, record);
        self.ref_order.push_back(reference);
        while self.refs.len() > MAX_JOIN_REFS {
            if let Some(old) = self.ref_order.pop_front() {
                if self.refs.remove(&old).is_some() {
                    self.stats.join_evicted = self.stats.join_evicted.saturating_add(1);
                }
            }
        }
        self.prune_refs(store);
    }

    fn prune_refs<P>(&mut self, store: &ObservationStore<P>) {
        let floor = store.stats().first_retained_sequence;
        while self.ref_order.front().is_some_and(|reference| {
            self.refs
                .get(reference)
                .is_none_or(|record| floor.is_none_or(|floor| record.sequence < floor))
        }) {
            if let Some(old) = self.ref_order.pop_front() {
                if self.refs.remove(&old).is_some() {
                    self.stats.join_evicted = self.stats.join_evicted.saturating_add(1);
                }
            }
        }
    }

    fn resolve_pending(
        &mut self,
        store: &mut ObservationStore<EventCategory>,
        now: std::time::Duration,
    ) -> Result<(), ()> {
        let mut remaining = VecDeque::new();
        while let Some(edge) = self.pending.pop_front() {
            let child = self.refs.get(&edge.child).cloned();
            let parent = self.refs.get(&edge.parent).cloned();
            if let (Some(child_record), Some(parent_record)) = (child, parent) {
                let payload = ReactiveTracePayload {
                    schema_version: crate::reactive_trace::REACTIVE_TRACE_SCHEMA_VERSION,
                    event: ReactiveDiagnosticPayload::CauseResolution {
                        edge: edge.edge,
                        child: edge.child,
                        parent: edge.parent,
                        relation: edge.relation,
                        child_record: child_record.clone(),
                        parent_record: parent_record.clone(),
                    },
                    span_context: None,
                    scope_cause: None,
                };
                let category = EventCategory {
                    kind: "reactive-cause-resolution",
                    reactive: Some(payload.clone()),
                };
                let bytes = serde_json::to_vec(&category).map_err(|_| ())?.len();
                let metadata = ObservationMetadata {
                    source_boot: self.boot_id.clone(),
                    cause: Some(parent_record),
                    ..ObservationMetadata::default()
                };
                match store
                    .record(category, bytes, metadata, now)
                    .map_err(|_| ())?
                {
                    Admission::Retained(_) => {
                        self.stats.resolved_edges = self.stats.resolved_edges.saturating_add(1);
                    },
                    Admission::Rejected { .. } => {
                        self.stats.store_rejected = self.stats.store_rejected.saturating_add(1);
                    },
                }
            } else {
                remaining.push_back(edge);
            }
        }
        self.pending = remaining;
        Ok(())
    }

    fn observe_sequence(&mut self, payload: &ReactiveTracePayload) {
        let reference = match &payload.event {
            ReactiveDiagnosticPayload::Cambium(
                cambium::reactive_diagnostics::ReactiveDiagnosticEvent::Decision {
                    reference, ..
                },
            )
            | ReactiveDiagnosticPayload::Cambium(
                cambium::reactive_diagnostics::ReactiveDiagnosticEvent::CauseEdge {
                    reference, ..
                },
            )
            | ReactiveDiagnosticPayload::Cambium(
                cambium::reactive_diagnostics::ReactiveDiagnosticEvent::CausesOmitted {
                    reference,
                    ..
                },
            ) => Some(*reference),
            ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::Decision {
                producer_id,
                sequence,
                ..
            }) => Some(cambium::reactive_diagnostics::ProducerRef {
                producer_id: *producer_id,
                sequence: *sequence,
            }),
            ReactiveDiagnosticPayload::Cambium(
                cambium::reactive_diagnostics::ReactiveDiagnosticEvent::SequenceExhausted {
                    ..
                }
                | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::ProducerIdUnavailable {
                    ..
                },
            )
            | ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::CoverageGap { .. })
            | ReactiveDiagnosticPayload::CauseResolution { .. }
            | ReactiveDiagnosticPayload::Coverage { .. }
            | ReactiveDiagnosticPayload::Rejected { .. } => None,
        };
        if let Some(reference) = reference {
            if self.sequence_max.len() >= 256
                && !self.sequence_max.contains_key(&reference.producer_id)
            {
                if let Some(oldest) = self.sequence_max.keys().next().copied() {
                    self.sequence_max.remove(&oldest);
                    self.missing.remove(&oldest);
                    self.stats.producer_tracker_evicted =
                        self.stats.producer_tracker_evicted.saturating_add(1);
                }
            }
            let max = self.sequence_max.entry(reference.producer_id).or_default();
            if reference.sequence > max.saturating_add(1) {
                let first = *max + 1;
                let last = reference.sequence - 1;
                if self.missing.len() >= 256 && !self.missing.contains_key(&reference.producer_id) {
                    if let Some(oldest) = self.missing.keys().next().copied() {
                        self.missing.remove(&oldest);
                    }
                }
                let range_count = self.missing.values().map(VecDeque::len).sum::<usize>();
                if range_count >= 256 {
                    if let Some(oldest) = self.missing.keys().next().copied() {
                        let remove = if let Some(ranges) = self.missing.get_mut(&oldest) {
                            ranges.pop_front();
                            ranges.is_empty()
                        } else {
                            false
                        };
                        if remove {
                            self.missing.remove(&oldest);
                        }
                        self.stats.producer_tracker_evicted =
                            self.stats.producer_tracker_evicted.saturating_add(1);
                    }
                }
                let ranges = self.missing.entry(reference.producer_id).or_default();
                ranges.push_back((first, last));
                self.stats.producer_gap_candidates = self
                    .stats
                    .producer_gap_candidates
                    .saturating_add(last - first + 1);
            }
            if let Some(ranges) = self.missing.get_mut(&reference.producer_id) {
                let mut retained = VecDeque::new();
                while let Some((first, last)) = ranges.pop_front() {
                    if (first..=last).contains(&reference.sequence) {
                        self.stats.producer_gap_late_fills =
                            self.stats.producer_gap_late_fills.saturating_add(1);
                        if first < reference.sequence {
                            retained.push_back((first, reference.sequence - 1));
                        }
                        if reference.sequence < last {
                            retained.push_back((reference.sequence + 1, last));
                        }
                    } else {
                        retained.push_back((first, last));
                    }
                }
                *ranges = retained;
            }
            *max = (*max).max(reference.sequence);
        }
    }
}

#[cfg(test)]
#[path = "reactive/tests.rs"]
mod tests;
