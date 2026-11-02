// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn touch(
    adapter: &mut TouchInput,
    pool: &mut Pool,
    plan: &[Surface],
    id: u64,
    phase: PointerPhase,
    x: f32,
) {
    adapter.deliver(
        pool,
        plan,
        FocusTarget::Graph(crate::panes::PaneId(99)),
        0,
        id,
        phase,
        PhysicalPosition { x, y: 9. },
        Some(0.6),
        Some(0.7),
        KeyboardModifiers::default(),
    );
}
