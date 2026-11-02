// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Admit a Workbench seam before document dispatch, including the invisible
/// part of its grab band. Match appearance custody so another pane or float
/// covering the same coordinates keeps its input.
pub(super) fn workbench_resize_target(
    app: &crate::app::App,
    plan: &[crate::surface::Surface],
    hit: Option<crate::surface::HitResult>,
    x: f32,
    y: f32,
) -> Option<(
    crate::panes::PaneId,
    crate::workbench_tiling::WbDivider,
    (f32, f32),
)> {
    use crate::surface::{SurfaceId, SurfaceKind};
    let hit = hit?;
    plan.iter().rev().find_map(|surface| {
        let SurfaceKind::Pane(pane) = surface.kind else {
            return None;
        };
        if !matches!(
            app.pane_content(pane),
            Some(crate::panes::PaneContent::Workbench)
        ) || !surface.rect.contains(x, y)
        {
            return None;
        }
        let owned = match hit.kind {
            SurfaceKind::Pane(id) => id == pane,
            SurfaceKind::Content(node) => {
                hit.id
                    == SurfaceId::content_appearance(
                        node,
                        crate::surface::appearance::workbench_appearance_role(pane, node),
                    )
            },
            _ => false,
        };
        if !owned {
            return None;
        }
        let geom = app.workbench_for_pane(pane)?.to_arrangement().1;
        let tiling = crate::workbench_tiling::place_workbench_at(
            geom.as_ref(),
            Rect::new(0.0, 0.0, surface.rect.w, surface.rect.h),
            app.shell_chrome_config().layout.document_tabs,
        );
        let divider = tiling
            .divider_at(x - surface.rect.x, y - surface.rect.y)?
            .clone();
        Some((pane, divider, (surface.rect.x, surface.rect.y)))
    })
}
