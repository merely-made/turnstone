// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn unavailable_surface_session(
    descriptor: SurfaceDescriptor,
    dom: DomHandle,
    reason: SurfaceUnavailableReason,
) -> Box<dyn RetainedSurfaceSession> {
    let state = UnavailableSurfaceState {
        label: descriptor.label.clone(),
        reason: reason.clone(),
    };
    let runner = GenetAppRunner::new(dom, unavailable_surface_view, state);
    Box::new(RunnerSurfaceSession::new(
        descriptor,
        runner,
        move |_state: &UnavailableSurfaceState| SurfaceAvailability::Unavailable(reason.clone()),
        |_state, _viewport| {},
        |_action: ()| Vec::new(),
    ))
}
