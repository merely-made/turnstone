// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DiskHeader {
    pub(super) fn to_header(&self, id: RunId) -> Result<RunHeader, String> {
        Ok(RunHeader {
            id,
            started_at_ms: self.started_at_ms,
            limits: self.limits.into(),
            ticket: RunTicket {
                binding: servitor::ResidentBinding {
                    id: servitor::ResidentId(decode::<16>(&self.resident)?),
                    subject: servitor::Subject::new(decode::<32>(&self.subject)?),
                    revision: servitor::BodyRevision(decode::<32>(&self.revision)?),
                    generation: self.generation,
                    lifecycle: parse_lifecycle(&self.lifecycle)?,
                },
                trigger: self.trigger.to_trigger()?,
                required: self
                    .required
                    .iter()
                    .map(|x| {
                        Ok((
                            Cap::parse(&x.cap).map_err(|_| "invalid stored capability")?,
                            parse_mode(&x.mode)?,
                        ))
                    })
                    .collect::<Result<_, String>>()?,
            },
        })
    }
}
