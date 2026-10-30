// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// One message as both converged peers see it. Sorted by the operation id,
/// which is content-derived, so two peers that hold the same messages
/// serialize the same bytes whatever order they arrived in.
pub(super) fn canonical_chat(
    projection: &commons::chat::ChatProjection,
) -> Vec<(String, String, String, u64)> {
    let mut rows: Vec<_> = projection
        .messages
        .iter()
        .map(|authored| {
            (
                crate::place::hex32(&authored.operation),
                crate::place::hex32(&authored.author),
                authored.message.body.clone(),
                authored.message.sent_at_ms,
            )
        })
        .collect();
    rows.sort();
    rows
}
