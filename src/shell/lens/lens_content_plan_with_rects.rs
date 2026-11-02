// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn lens_content_plan_with_rects(
    app: &crate::app::App,
    contents: &super::super::NodeDocuments,
    ordinal: usize,
    w: u32,
    h: u32,
    frame_rects: Option<&[(PaneId, Rect)]>,
    frame_floats: &[(PaneId, Rect, Rect)],
) -> Vec<crate::surface::Surface> {
    let Some(Some(space)) = app.lenses.get(ordinal) else {
        return Vec::new();
    };
    let area = Rect::full(w.max(1), h.max(1));
    let workspace_area = crate::chrome_view::workspace_area_for_window(
        app.shell_chrome_config(),
        ordinal + 1,
        w.max(1),
        h.max(1),
    );
    let (pane_rects, divider_rects, float_rects): (
        Vec<(crate::panes::PaneId, Rect)>,
        Vec<(u32, Rect)>,
        Vec<(crate::panes::PaneId, Rect)>,
    ) = match app.blueprint_space(crate::action::SpaceRef::Lens(ordinal)) {
        Some(blueprint) => {
            let placements = crate::panes::place_space(blueprint, workspace_area, None);
            (
                placements
                    .panes
                    .into_iter()
                    .map(|pane| (pane.id, pane.rect))
                    .collect(),
                placements
                    .dividers
                    .into_iter()
                    .map(|divider| (divider.index, divider.rect))
                    .collect(),
                placements
                    .floats
                    .into_iter()
                    .map(|pane| (pane.id, pane.rect))
                    .collect(),
            )
        },
        None => (
            frame_rects.unwrap_or_default().to_vec(),
            Vec::new(),
            frame_floats
                .iter()
                .map(|(id, _, body)| (*id, *body))
                .collect(),
        ),
    };
    let mut base: Vec<(SurfaceKind, Rect)> = pane_rects
        .iter()
        .filter_map(|(id, rect)| {
            lens_pane_content(app, ordinal, *id).map(|content| (id, rect, content))
        })
        .map(|(id, rect, content)| {
            if matches!(content, PaneContent::Orrery) {
                (SurfaceKind::Graph(*id), *rect)
            } else if let PaneContent::Tile(m) = content
                && contents.contains_key(&m)
            {
                // A torn-out tile: the pinned pane composites its live
                // session as this window's content surface.
                (SurfaceKind::Content(m), *rect)
            } else {
                (SurfaceKind::Pane(*id), *rect)
            }
        })
        .collect();
    if app
        .blueprint_space(crate::action::SpaceRef::Lens(ordinal))
        .is_none()
    {
        base.insert(0, (SurfaceKind::Frame, area));
    }
    base.extend(
        divider_rects
            .iter()
            .map(|(index, rect)| (SurfaceKind::Divider(*index), *rect)),
    );
    // Workbench tiles in a LENS (rung-7 depth: content tiles follow the
    // pane): when the workbench pane tore out to this window, its cells'
    // live tiles compose as content surfaces at their body rects — the
    // same walk the primary plan does, at the lens pane's rect.
    let workbench_pane = pane_rects
        .iter()
        .find(|(id, _)| {
            matches!(
                lens_pane_content(app, ordinal, *id),
                Some(PaneContent::Workbench)
            )
        })
        .map(|(id, rect)| (*id, *rect));
    let tiles: Vec<(uuid::Uuid, Rect)> = workbench_pane
        .map(|(pane, rect)| {
            let geom = app
                .workbench_for_pane(pane)
                .and_then(|workbench| workbench.to_arrangement().1);
            crate::workbench_tiling::place_reading_workbench(app, pane, geom.as_ref(), rect)
                .cells
                .iter()
                .filter_map(|c| {
                    let m = c.active_member()?;
                    contents.contains_key(&m).then(|| (m, c.body()))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut surfaces = crate::surface::assemble(&base, &tiles, None, None);
    let mut appearance_roles = Vec::new();
    if let Some((pane, rect)) = workbench_pane {
        let geom = app
            .workbench_for_pane(pane)
            .and_then(|workbench| workbench.to_arrangement().1);
        for cell in
            crate::workbench_tiling::place_reading_workbench(app, pane, geom.as_ref(), rect).cells
        {
            if let Some(node) = cell.active_member()
                && contents.contains_key(&node)
            {
                appearance_roles.push((
                    node,
                    crate::surface::appearance::workbench_appearance_role(pane, node),
                    cell.body(),
                ));
            }
        }
    }
    appearance_roles.extend(pane_rects.iter().filter_map(|(pane, rect)| {
        match lens_pane_content(app, ordinal, *pane) {
            Some(PaneContent::Tile(node)) if contents.contains_key(&node) => {
                Some((node, 0x1000_0000_0000_0000 | pane.0, *rect))
            },
            _ => None,
        }
    }));
    appearance_roles.extend(
        float_rects.iter().filter_map(|(pane, rect)| {
            match lens_pane_content(app, ordinal, *pane) {
                Some(PaneContent::Tile(node)) if contents.contains_key(&node) => {
                    Some((node, 0x1000_0000_0000_0000 | pane.0, *rect))
                },
                _ => None,
            }
        }),
    );
    for (id, rect) in float_rects {
        let Some(content) = lens_pane_content(app, ordinal, id) else {
            continue;
        };
        if let Some((_, outer, _)) = frame_floats.iter().find(|(pane, _, _)| *pane == id) {
            let kind = SurfaceKind::FloatFrame(id);
            surfaces.push(crate::surface::Surface {
                id: crate::surface::SurfaceId::for_kind(kind),
                kind,
                rect: *outer,
            });
        }
        let kind = match &content {
            PaneContent::Orrery => SurfaceKind::Graph(id),
            PaneContent::Tile(member) if contents.contains_key(member) => {
                SurfaceKind::Content(*member)
            },
            _ => SurfaceKind::Pane(id),
        };
        surfaces.push(crate::surface::Surface {
            id: crate::surface::SurfaceId::for_kind(kind),
            kind,
            rect,
        });
        if matches!(content, PaneContent::Workbench) {
            let geom = app
                .workbench_for_pane(id)
                .and_then(|w| w.to_arrangement().1);
            for cell in
                crate::workbench_tiling::place_reading_workbench(app, id, geom.as_ref(), rect).cells
            {
                if let Some(node) = cell.active_member().filter(|n| contents.contains_key(n)) {
                    let body = cell.body();
                    appearance_roles.push((
                        node,
                        crate::surface::appearance::workbench_appearance_role(id, node),
                        body,
                    ));
                    surfaces.push(crate::surface::Surface {
                        id: crate::surface::SurfaceId::content(node),
                        kind: SurfaceKind::Content(node),
                        rect: body,
                    });
                }
            }
        }
    }
    crate::surface::appearance::assign_content_appearance_ids(
        &mut surfaces,
        ordinal as u32 + 1,
        &appearance_roles,
    );
    surfaces
}
