// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn lens_fresh_pane(f: &CallerCaptureFixture) -> PhysicalPosition {
    let plan = f.plan();
    let focus = FocusTarget::Graph(f.app.default_graph_pane());
    for surface in &plan {
        if !matches!(surface.kind, SurfaceKind::Pane(_)) {
            continue;
        }
        for dy in [1., 8., surface.rect.h / 2., surface.rect.h - 1.] {
            for dx in [1., 8., surface.rect.w / 2., surface.rect.w - 1.] {
                let pos = PhysicalPosition {
                    x: surface.rect.x + dx,
                    y: surface.rect.y + dy,
                };
                if crate::surface::hit_test(&plan, focus, pos.x, pos.y)
                    .is_some_and(|h| h.id == surface.id)
                {
                    return pos;
                }
            }
        }
    }
    panic!("actual Lens plan exposes a Pane before any content routing");
}
