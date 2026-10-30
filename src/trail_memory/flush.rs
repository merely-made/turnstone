// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Flush open segments; a failure warns and keeps them buffered (they retry
/// on the next lifecycle edge).
pub(super) fn flush(store: &mut FjallStore, memory: &mut BrowsingMemory) {
    if let Err(err) = pollster::block_on(memory.flush(store, now_ms())) {
        tracing::warn!(%err, "trail memory: flush failed; open segments retained");
    }
}
