// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn native_mouse(
    pool: &mut Pool,
    buttons: &mut HashMap<u64, PointerButtons>,
    window: u64,
    appearance: Option<SurfaceId>,
    phase: PointerPhase,
    button: Option<inker::MouseButton>,
) {
    if let Some((_, effect)) = native_mouse_input(
        pool,
        buttons,
        window,
        appearance,
        PhysicalPosition { x: 7., y: 9. },
        phase,
        button,
        Default::default(),
    ) {
        assert!(effect.navigation.is_none());
        assert!(effect.submission.is_none());
    }
}
