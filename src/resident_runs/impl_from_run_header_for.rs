// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<&RunHeader> for DiskHeader {
    fn from(h: &RunHeader) -> Self {
        Self {
            started_at_ms: h.started_at_ms,
            resident: hex(&h.ticket.binding.id.0),
            subject: h.ticket.binding.subject.to_hex(),
            revision: hex(&h.ticket.binding.revision.0),
            generation: h.ticket.binding.generation,
            lifecycle: format!("{:?}", h.ticket.binding.lifecycle),
            trigger: (&h.ticket.trigger).into(),
            required: h
                .ticket
                .required
                .iter()
                .map(|(c, m)| DiskRequirement {
                    cap: c.to_wire(),
                    mode: format!("{m:?}"),
                })
                .collect(),
            limits: h.limits.into(),
        }
    }
}
