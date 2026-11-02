// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl AuthorityClock {
    pub(crate) fn now_ms(self) -> u64 {
        match self {
            // A clock behind the epoch yields 0, under which no grant has
            // opened yet, so every retained fact reads as pending rather than
            // effective. Unreadable time withholds content, it does not admit it.
            Self::SystemTime => std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis() as u64),
            Self::Fixed(at_ms) => at_ms,
        }
    }
}
