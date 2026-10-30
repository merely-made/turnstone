// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn modal_touch_delivery(
    f: &mut CallerCaptureFixture,
    touch: &mut TouchInput,
    window: u64,
    host_id: u64,
    phase: PointerPhase,
    index: usize,
) -> bool {
    if touch.reject_modal(
        &mut f.contents.pool,
        window,
        f.app.user_agent_decision.is_open(),
    ) {
        return false;
    }
    // Placement construction is already held by the real-plan fixture; the
    // rejected branch must return before any geometry, focus or page delivery.
    let plan = f.plan();
    let position = f.position(index);
    touch.deliver(
        &mut f.contents.pool,
        &plan,
        f.app.focus,
        window,
        host_id,
        phase,
        position,
        Some(0.6),
        Some(0.4),
        Default::default(),
    )
}
