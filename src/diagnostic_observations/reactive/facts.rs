// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::PendingEdge;
use crate::reactive_trace::{MeristemReactiveEvent, ReactiveDiagnosticPayload};

pub(super) fn payload_facts(
    payload: &ReactiveDiagnosticPayload,
) -> (
    Option<cambium::reactive_diagnostics::ProducerRef>,
    Option<cambium::reactive_diagnostics::Subject>,
    Option<u64>,
    Option<PendingEdge>,
) {
    use cambium::reactive_diagnostics::ReactiveDiagnosticEvent as Event;
    match payload {
        ReactiveDiagnosticPayload::Cambium(Event::Decision {
            reference,
            subject,
            kind,
            details,
            ..
        }) => (
            Some(*reference),
            *subject,
            (*kind == cambium::reactive_diagnostics::DiagnosticKind::RevisionAdvanced)
                .then_some(details.revision)
                .flatten(),
            None,
        ),
        ReactiveDiagnosticPayload::Cambium(Event::CauseEdge {
            reference,
            child,
            parent,
            relation,
            ..
        }) => (
            Some(*reference),
            None,
            None,
            Some(PendingEdge {
                edge: *reference,
                child: *child,
                parent: *parent,
                relation: *relation,
            }),
        ),
        ReactiveDiagnosticPayload::Cambium(Event::CausesOmitted { reference, .. }) => {
            (Some(*reference), None, None, None)
        },
        ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::Decision {
            producer_id,
            sequence,
            ..
        }) => (
            Some(cambium::reactive_diagnostics::ProducerRef {
                producer_id: *producer_id,
                sequence: *sequence,
            }),
            None,
            None,
            None,
        ),
        ReactiveDiagnosticPayload::Meristem(MeristemReactiveEvent::CoverageGap { .. })
        | ReactiveDiagnosticPayload::Cambium(
            Event::SequenceExhausted { .. } | Event::ProducerIdUnavailable { .. },
        )
        | ReactiveDiagnosticPayload::CauseResolution { .. }
        | ReactiveDiagnosticPayload::Coverage { .. }
        | ReactiveDiagnosticPayload::Rejected { .. } => (None, None, None, None),
    }
}
