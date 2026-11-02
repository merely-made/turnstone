// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Canonicalize a persisted or requested viewer id after retiring Turnstone's
/// incumbent static HTML lane. The old id described the same product rung, so
/// preserving the pin means moving it to Livery rather than clearing it.
pub(super) fn canonical_viewer_override(viewer: Option<String>) -> Option<String> {
    viewer.map(|engine_id| {
        if engine_id == inker::routing::ENGINE_GENET_WEB {
            inker::routing::ENGINE_GENET_LIVERY.to_string()
        } else {
            engine_id
        }
    })
}
