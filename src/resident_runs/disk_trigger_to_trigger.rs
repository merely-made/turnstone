// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DiskTrigger {
    pub(super) fn to_trigger(&self) -> Result<Trigger, String> {
        Ok(match self {
            Self::Manual => Trigger::Manual,
            Self::Journal {
                source,
                first,
                last,
            } if !source.trim().is_empty() && first <= last => Trigger::Journal {
                source: source.clone(),
                first: *first,
                last: *last,
            },
            Self::Clock { at_ms } => Trigger::Clock { at_ms: *at_ms },
            _ => return Err("invalid stored trigger".into()),
        })
    }
}
