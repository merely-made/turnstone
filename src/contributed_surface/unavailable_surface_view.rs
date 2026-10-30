// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn unavailable_surface_view(
    state: &UnavailableSurfaceState,
) -> impl View<UnavailableSurfaceState, (), GenetCtx, Element = GenetElement> + use<> {
    el::<_, UnavailableSurfaceState, ()>(
        "section",
        format!("{} unavailable: {:?}", state.label, state.reason),
    )
    .attr("class", "contributed-surface-unavailable")
    .attr("role", "status")
}
