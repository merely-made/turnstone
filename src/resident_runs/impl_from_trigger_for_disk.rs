// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<&Trigger> for DiskTrigger {
    fn from(t: &Trigger) -> Self {
        match t {
            Trigger::Manual => Self::Manual,
            Trigger::Journal {
                source,
                first,
                last,
            } => Self::Journal {
                source: source.clone(),
                first: *first,
                last: *last,
            },
            Trigger::Clock { at_ms } => Self::Clock { at_ms: *at_ms },
        }
    }
}
