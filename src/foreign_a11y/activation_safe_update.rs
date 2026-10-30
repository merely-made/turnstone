// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Check activation at the platform-serialized application boundary. Call this
/// inside each `update_if_active` closure, never while preparing the batch.
/// A replacement consumer receives safe ROOT; the caller must abort the old
/// batch, reset publication and schedule complete parent-first replay.
pub(crate) fn activation_safe_update(
    replay: &std::sync::atomic::AtomicBool,
    update: TreeUpdate,
    safe_host: &TreeUpdate,
) -> (TreeUpdate, bool) {
    if replay.swap(false, std::sync::atomic::Ordering::AcqRel) {
        (safe_host.clone(), true)
    } else {
        (update, false)
    }
}
