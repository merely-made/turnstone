// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Mirror one engine's page-zoom answer into app truth. Transient by
/// construction: it belongs to the live session that computed it, and the
/// sidecar keeps only what the node requested.
pub(in super::super) fn app_page_zoom_facts(
    state: inker::DocumentZoomState,
) -> crate::content::PageZoomFacts {
    crate::content::PageZoomFacts {
        requested: state.requested,
        applied: state.applied,
        min: state.min,
        max: state.max,
    }
}
