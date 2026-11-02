// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_binding(
    retained: &mut RetainedEndpointSession,
    session: &ProjectionSession,
    address: &str,
) -> Result<DocumentBinding, String> {
    retained
        .resolve_all(session)?
        .into_iter()
        .find_map(|(target, presentation)| binding_from_presentation(target, presentation, address))
        .ok_or_else(|| format!("Knot did not disclose writable text for {address}"))
}
