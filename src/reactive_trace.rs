// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Explicit, bounded tracing bridge for Cambium and Meristem reactive events.

use std::time::Duration;

use registry::diagnostics::{
    DiagnosticCorrelation, DiagnosticEvent, DiagnosticReceiver, DiagnosticSender, IngressLimits,
    diagnostic_channel,
};
use serde::{Deserialize, Serialize};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
    span::Attributes,
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

mod meristem_fields;
use meristem_fields::MeristemFieldsVisitor;

const CAMBIUM_TARGET: &str = "cambium::reactive";
const MERISTEM_TARGET: &str = "meristem::reactive";
const MAX_EVENT_JSON_BYTES: usize = 2_048;
pub const REACTIVE_TRACE_SCHEMA_VERSION: u16 = 1;

pub struct ReactiveTraceReceiver {
    receiver: DiagnosticReceiver,
    sender: DiagnosticSender,
    boot_id: apparatus_core::SourceBootId,
    run_id: apparatus_core::RunId,
}

impl ReactiveTraceReceiver {
    pub fn boot_id(&self) -> &apparatus_core::SourceBootId {
        &self.boot_id
    }

    pub(crate) fn run_id(&self) -> &apparatus_core::RunId {
        &self.run_id
    }

    pub(crate) fn try_drain(
        &self,
        max_records: usize,
    ) -> Result<registry::diagnostics::IngressBatch, registry::diagnostics::IngressAccessError>
    {
        self.receiver.try_drain(max_records)
    }

    pub(crate) fn try_stats(
        &self,
    ) -> Result<registry::diagnostics::IngressStats, registry::diagnostics::IngressAccessError>
    {
        self.sender.try_stats()
    }
}

/// Creates a tracing layer and one matching bounded receiver for a single run.
pub fn channel(
    run_id: apparatus_core::RunId,
    boot_id: apparatus_core::SourceBootId,
) -> (ReactiveTraceLayer, ReactiveTraceReceiver) {
    let (sender, receiver) = diagnostic_channel(IngressLimits {
        max_records: 4_096,
        max_bytes: 2 * 1024 * 1024,
        max_age: Duration::from_secs(300),
    });
    (
        ReactiveTraceLayer {
            sender: sender.clone(),
            run_id: run_id.clone(),
        },
        ReactiveTraceReceiver {
            receiver,
            sender,
            boot_id,
            run_id,
        },
    )
}

#[derive(Clone)]
pub struct ReactiveTraceLayer {
    sender: DiagnosticSender,
    run_id: apparatus_core::RunId,
}

impl<S> Layer<S> for ReactiveTraceLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let target = event.metadata().target();
        let level = match *event.metadata().level() {
            tracing::Level::TRACE => "TRACE",
            tracing::Level::DEBUG => "DEBUG",
            tracing::Level::INFO => "INFO",
            tracing::Level::WARN => "WARN",
            tracing::Level::ERROR => "ERROR",
        };
        if (target == CAMBIUM_TARGET && !matches!(level, "INFO" | "WARN"))
            || (target == MERISTEM_TARGET && !matches!(level, "TRACE" | "WARN"))
        {
            return;
        }
        let projected = match target {
            CAMBIUM_TARGET => {
                let mut visitor = CambiumPayloadVisitor::default();
                event.record(&mut visitor);
                if visitor.invalid {
                    None
                } else {
                    visitor.payload.and_then(validate_cambium_payload)
                }
            },
            MERISTEM_TARGET => {
                let mut visitor = MeristemFieldsVisitor::default();
                event.record(&mut visitor);
                visitor.to_payload()
            },
            _ => return,
        };
        let event_payload = projected.unwrap_or(ReactiveDiagnosticPayload::Rejected {
            reason: ReactiveRejectReason::InvalidOrOversizedPayload,
        });
        let span_context = event_span_context(event, _ctx);
        let event_reference = producer_ref(&event_payload);
        let scope_cause = span_context
            .and_then(|context| context.operation)
            .filter(|operation| Some(*operation) != event_reference);
        let payload = ReactiveTracePayload {
            schema_version: REACTIVE_TRACE_SCHEMA_VERSION,
            event: event_payload,
            span_context,
            scope_cause,
        };
        let (payload, json) = match serde_json::to_string(&payload) {
            Ok(json) if json.len() <= MAX_EVENT_JSON_BYTES => (payload, json),
            _ => {
                let rejected = ReactiveTracePayload {
                    schema_version: REACTIVE_TRACE_SCHEMA_VERSION,
                    event: ReactiveDiagnosticPayload::Rejected {
                        reason: ReactiveRejectReason::InvalidOrOversizedPayload,
                    },
                    span_context: None,
                    scope_cause: None,
                };
                let Ok(json) = serde_json::to_string(&rejected) else {
                    return;
                };
                (rejected, json)
            },
        };
        let operation_ref = payload
            .span_context
            .and_then(|context| context.operation)
            .or_else(|| producer_ref(&payload.event));
        let event = DiagnosticEvent::Event {
            target: if target == CAMBIUM_TARGET {
                CAMBIUM_TARGET
            } else {
                MERISTEM_TARGET
            },
            level,
            message: "shared reactive observation".to_owned(),
            fields: vec![registry::diagnostics::StructuredPayloadField {
                name: "payload",
                value: json,
            }],
        };
        let correlation = operation_ref.map(|reference| DiagnosticCorrelation {
            run: self.run_id.0.clone(),
            source: "turnstone.cambium-reactive".to_owned(),
            operation: format!("{}:{}", reference.producer_id, reference.sequence),
        });
        if let Some(correlation) = correlation {
            let _ = self.sender.try_send_correlated(event, correlation);
        } else {
            let _ = self.sender.try_send(event);
        }
    }

    fn on_new_span(&self, attrs: &Attributes<'_>, id: &tracing::Id, ctx: Context<'_, S>) {
        if attrs.metadata().target() != CAMBIUM_TARGET {
            return;
        }
        let mut fields = ReactiveSpanFields::default();
        attrs.record(&mut fields);
        if !fields.invalid {
            if let Some(span) = ctx.span(id) {
                let mut extensions = span.extensions_mut();
                if let Some(current) = extensions.get_mut::<ReactiveSpanFields>() {
                    current.merge(fields);
                } else {
                    extensions.insert(fields);
                }
            }
        }
    }

    fn on_record(&self, id: &tracing::Id, values: &tracing::span::Record<'_>, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if span.metadata().target() != CAMBIUM_TARGET {
            return;
        }
        let mut fields = ReactiveSpanFields::default();
        values.record(&mut fields);
        if fields.invalid {
            return;
        }
        let mut extensions = span.extensions_mut();
        if let Some(current) = extensions.get_mut::<ReactiveSpanFields>() {
            current.merge(fields);
        } else {
            extensions.insert(fields);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactiveSpanContext {
    pub subject: Option<cambium::reactive_diagnostics::Subject>,
    pub update_id: Option<u64>,
    pub operation: Option<cambium::reactive_diagnostics::ProducerRef>,
}

impl ReactiveSpanContext {
    fn merge(&mut self, next: Self) {
        if next.subject.is_some() {
            self.subject = next.subject;
        }
        if next.update_id.is_some() {
            self.update_id = next.update_id;
        }
        if next.operation.is_some() {
            self.operation = next.operation;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ReactiveSpanFields {
    owner_id: Option<u64>,
    slot: Option<u64>,
    route_generation: Option<u64>,
    placement_generation: Option<u64>,
    update_id: Option<u64>,
    operation_producer_id: Option<u64>,
    operation_sequence: Option<u64>,
    invalid: bool,
}

impl ReactiveSpanFields {
    fn into_context(self) -> ReactiveSpanContext {
        let subject = match (
            self.owner_id,
            self.slot,
            self.route_generation,
            self.placement_generation,
        ) {
            (Some(owner_id), Some(slot), Some(route_generation), Some(placement_generation)) => {
                Some(cambium::reactive_diagnostics::Subject {
                    owner_id,
                    slot,
                    route_generation,
                    placement_generation,
                })
            },
            _ => None,
        };
        let operation = match (self.operation_producer_id, self.operation_sequence) {
            (Some(producer_id), Some(sequence)) => {
                Some(cambium::reactive_diagnostics::ProducerRef {
                    producer_id,
                    sequence,
                })
            },
            _ => None,
        };
        ReactiveSpanContext {
            subject,
            update_id: self.update_id,
            operation,
        }
    }

    fn merge(&mut self, next: Self) {
        macro_rules! merge_some {
            ($($field:ident),+ $(,)?) => {
                $(if next.$field.is_some() { self.$field = next.$field; })+
            };
        }
        merge_some!(
            owner_id,
            slot,
            route_generation,
            placement_generation,
            update_id,
            operation_producer_id,
            operation_sequence,
        );
    }
}

impl Visit for ReactiveSpanFields {
    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {
        self.invalid = true;
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "owner_id" => self.owner_id = Some(value),
            "slot" => self.slot = Some(value),
            "route_generation" => self.route_generation = Some(value),
            "placement_generation" => self.placement_generation = Some(value),
            "update_id" => self.update_id = Some(value),
            "operation_producer_id" => self.operation_producer_id = Some(value),
            "operation_sequence" => self.operation_sequence = Some(value),
            _ => {},
        }
    }
}

fn event_span_context<S>(event: &Event<'_>, ctx: Context<'_, S>) -> Option<ReactiveSpanContext>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    let scope = ctx.event_scope(event)?;
    let mut merged = ReactiveSpanContext::default();
    let mut seen = false;
    for span in scope.from_root() {
        if let Some(fields) = span.extensions().get::<ReactiveSpanFields>() {
            merged.merge(fields.into_context());
            seen = true;
        }
    }
    seen.then_some(merged)
}

pub(crate) fn producer_ref(
    payload: &ReactiveDiagnosticPayload,
) -> Option<cambium::reactive_diagnostics::ProducerRef> {
    use cambium::reactive_diagnostics::ReactiveDiagnosticEvent as Event;
    match payload {
        ReactiveDiagnosticPayload::Cambium(Event::Decision { reference, .. })
        | ReactiveDiagnosticPayload::Cambium(Event::CauseEdge { reference, .. })
        | ReactiveDiagnosticPayload::Cambium(Event::CausesOmitted { reference, .. }) => {
            Some(*reference)
        },
        ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::Decision {
            producer_id,
            sequence,
            ..
        }) => Some(cambium::reactive_diagnostics::ProducerRef {
            producer_id: *producer_id,
            sequence: *sequence,
        }),
        ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::CoverageGap { .. })
        | ReactiveDiagnosticPayload::Cambium(
            cambium::reactive_diagnostics::ReactiveDiagnosticEvent::SequenceExhausted { .. }
            | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::ProducerIdUnavailable {
                ..
            },
        )
        | ReactiveDiagnosticPayload::CauseResolution { .. }
        | ReactiveDiagnosticPayload::Coverage { .. }
        | ReactiveDiagnosticPayload::Rejected { .. } => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    content = "event",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ReactiveDiagnosticPayload {
    Cambium(cambium::reactive_diagnostics::ReactiveDiagnosticEvent),
    Meristem(MeristemReactiveEvent),
    CauseResolution {
        edge: cambium::reactive_diagnostics::ProducerRef,
        child: cambium::reactive_diagnostics::ProducerRef,
        parent: cambium::reactive_diagnostics::ProducerRef,
        relation: cambium::reactive_diagnostics::CauseRelation,
        child_record: apparatus_core::RecordRef,
        parent_record: apparatus_core::RecordRef,
    },
    Coverage {
        counters: crate::diagnostic_observations::ReactiveObservationStats,
    },
    Rejected {
        reason: ReactiveRejectReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactiveRejectReason {
    InvalidOrOversizedPayload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactiveTracePayload {
    pub schema_version: u16,
    pub event: ReactiveDiagnosticPayload,
    pub span_context: Option<ReactiveSpanContext>,
    pub scope_cause: Option<cambium::reactive_diagnostics::ProducerRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeristemKind {
    AnyViewReused,
    AnyViewReplaced,
    AnyViewTornDown,
    GenerationMatched,
    GenerationRefused,
    MessagePathAdvanced,
    MessageConsumed,
    MessageDeclined,
    SequenceChildAdded,
    SequenceChildRemoved,
    SequenceMessageMatched,
    SequenceMessageRefused,
    CoverageUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeristemReason {
    SequenceExhausted,
    TargetLacksAtomicU64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case", deny_unknown_fields)]
pub enum MeristemReactiveEvent {
    Decision {
        schema_version: u16,
        producer_id: u64,
        sequence: u64,
        kind: MeristemKind,
        generation: Option<u64>,
        path_depth: Option<u16>,
        path_truncated: bool,
    },
    CoverageGap {
        schema_version: u16,
        producer_id: u64,
        kind: MeristemKind,
        reason: MeristemReason,
        last_sequence: Option<u64>,
    },
}

#[derive(Default)]
struct CambiumPayloadVisitor {
    payload: Option<String>,
    invalid: bool,
}

impl Visit for CambiumPayloadVisitor {
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        if field.name() != "message" {
            self.invalid = true;
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "payload" if value.len() <= MAX_EVENT_JSON_BYTES && self.payload.is_none() => {
                self.payload = Some(value.to_owned());
            },
            "message" if value == "shared reactive decision" => {},
            _ => self.invalid = true,
        }
    }
    fn record_u64(&mut self, _field: &Field, _value: u64) {
        self.invalid = true;
    }
    fn record_bool(&mut self, _field: &Field, _value: bool) {
        self.invalid = true;
    }
}

fn validate_cambium_payload(json: String) -> Option<ReactiveDiagnosticPayload> {
    if json.len() > MAX_EVENT_JSON_BYTES {
        return None;
    }
    let event =
        serde_json::from_str::<cambium::reactive_diagnostics::ReactiveDiagnosticEvent>(&json)
            .ok()?;
    let schema_version = match &event {
        cambium::reactive_diagnostics::ReactiveDiagnosticEvent::Decision {
            schema_version, ..
        }
        | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::CauseEdge {
            schema_version,
            ..
        }
        | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::CausesOmitted {
            schema_version,
            ..
        }
        | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::SequenceExhausted {
            schema_version,
            ..
        }
        | cambium::reactive_diagnostics::ReactiveDiagnosticEvent::ProducerIdUnavailable {
            schema_version,
        } => *schema_version,
    };
    (schema_version == cambium::reactive_diagnostics::REACTIVE_DIAGNOSTIC_SCHEMA_VERSION)
        .then_some(ReactiveDiagnosticPayload::Cambium(event))
}

#[cfg(test)]
#[path = "reactive_trace/tests.rs"]
mod tests;
