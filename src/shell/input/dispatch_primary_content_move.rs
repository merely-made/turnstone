// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn dispatch_primary_content_move(
    host: &mut impl PrimaryPointerHost,
    x: f32,
    y: f32,
) -> bool {
    let Some(crate::surface::SurfaceKind::Content(node)) = *host.pointer_state().2 else {
        return false;
    };
    host.sync_pointer_placements();
    let target = {
        let (_, pool, _) = host.pointer_state();
        super::super::window_input::captured_mouse(pool).and_then(|id| {
            pool.placement(&id).map(|placement| {
                let scale = placement.scale_factor;
                (
                    id,
                    (x - placement.rect.x * scale, y - placement.rect.y * scale),
                )
            })
        })
    };
    if let Some((id, local)) = target {
        host.pointer_content_mouse(id, local, inker::PointerPhase::Move, None);
        host.pointer_content_hovered(node);
    }
    true
}
