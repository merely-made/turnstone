// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn primary_content_plan_with_rects(
    app: &App,
    contents: &NodeDocuments,
    width: u32,
    height: u32,
    frame_rects: Option<&[(crate::panes::PaneId, Rect)]>,
    frame_floats: &[(crate::panes::PaneId, Rect, Rect)],
) -> Vec<crate::surface::Surface> {
    let area = Rect::full(width.max(1), height.max(1));
    let workspace_area = crate::chrome_view::workspace_area(
        app.shell_chrome_config(),
        true,
        width.max(1),
        height.max(1),
    );
    // A5 promotes a space to its `SpaceBlueprint` only when it enters the
    // float layer. Its placements then drive this exact compositor path;
    // legacy Frisket still supplies the payload and retained renderer
    // lookup during the A8 persistence migration.
    let (pane_rects, divider_rects, float_rects): (
        Vec<(crate::panes::PaneId, Rect)>,
        Vec<(u32, Rect)>,
        Vec<(crate::panes::PaneId, Rect)>,
    ) = match app.blueprint_space(crate::action::SpaceRef::Primary) {
        Some(blueprint) => {
            let placements = crate::panes::place_space(blueprint, workspace_area, app.maximized);
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
    let mut graph_rects = Vec::new();
    let mut base: Vec<(SurfaceKind, Rect)> = pane_rects
        .iter()
        .filter_map(|(id, rect)| app.pane_content(*id).map(|content| (id, rect, content)))
        .map(|(id, rect, content)| {
            if matches!(content, PaneContent::Orrery) {
                graph_rects.push((*id, *rect));
                (SurfaceKind::Graph(*id), *rect)
            } else if let PaneContent::Tile(m) = content
                && contents.contains_key(&m)
            {
                // A pinned Tile pane with a live session IS a content
                // surface at the pane's rect — same keyed path as an
                // inset or workbench tile, so input routes for free.
                (SurfaceKind::Content(*m), *rect)
            } else {
                (SurfaceKind::Pane(*id), *rect)
            }
        })
        .collect();
    if app
        .blueprint_space(crate::action::SpaceRef::Primary)
        .is_none()
    {
        base.insert(0, (SurfaceKind::Frame, area));
    }
    // Each seam is its own thin surface, so it paints (an empty scene over
    // the seam clear colour) and takes the divider drag.
    base.extend(
        divider_rects
            .iter()
            .map(|(index, rect)| (SurfaceKind::Divider(*index), *rect)),
    );
    // Workbench tiles (rung 5 slice E): the Workbench pane's cells, walked
    // at the pane's WINDOW rect, compose each visible (active) tile with a
    // live session as its own content surface at the cell's body rect —
    // the same keyed path the focused inset uses, so tile input routing
    // (wheel, clicks, focus) arrives through the existing Content arms.
    let workbench_panes: Vec<_> = pane_rects
        .iter()
        .filter(|(id, _)| matches!(app.pane_content(*id), Some(PaneContent::Workbench)))
        .map(|(id, rect)| (*id, *rect))
        .collect();
    let tiles: Vec<(uuid::Uuid, Rect)> = workbench_panes
        .iter()
        .flat_map(|(pane, rect)| {
            let geom = app
                .workbench_for_pane(*pane)
                .and_then(|workbench| workbench.to_arrangement().1);
            crate::workbench_tiling::place_reading_workbench(app, *pane, geom.as_ref(), *rect)
                .cells
                .iter()
                .filter_map(|c| {
                    let m = c.active_member()?;
                    contents.contains_key(&m).then(|| (m, c.body()))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    // Content overlays the canvas pane (when it is shown); a live node's
    // document insets within the graph, not over a maximized pane. A node
    // showing as a workbench tile is not ALSO inset over the canvas: one
    // session, one surface, or the two frame at fighting sizes. That rule
    // holds ACROSS windows too — when the workbench pane tore out to a
    // lens, its tiles render THERE (the lens's plan walks them), so the
    // same membership excludes the inset here.
    let wb_in_lens = workbench_panes.is_empty()
        && app.lenses.iter().flatten().any(|space| {
            space
                .iter_leaves()
                .any(|(_, c, _)| matches!(c, PaneContent::Workbench))
        });
    let tiled_in_lens = |id: &uuid::Uuid| {
        wb_in_lens && {
            app.lenses
                .iter()
                .enumerate()
                .filter_map(|(ordinal, space)| {
                    space.as_ref().and_then(|space| {
                        space
                            .iter_leaves()
                            .find(|(_, content, _)| matches!(content, PaneContent::Workbench))
                            .and_then(|(pane, _, _)| app.workbench_for_pane(pane))
                            .and_then(|workbench| workbench.to_arrangement().1)
                            .map(|geometry| (ordinal, geometry))
                    })
                })
                .any(|(_, geom)| {
                    crate::workbench_tiling::place_workbench_at(
                        Some(&geom),
                        area,
                        app.shell_chrome_config().layout.document_tabs,
                    )
                    .cells
                    .iter()
                    .any(|c| c.active_member() == Some(*id))
                })
        }
    };
    // A pinned Tile pane claims its member wherever its space shows.
    let tile_paned = |id: &uuid::Uuid| {
        app.frisket
            .iter_leaves()
            .chain(app.lenses.iter().flatten().flat_map(|s| s.iter_leaves()))
            .any(|(_, c, _)| matches!(c, PaneContent::Tile(m) if *m == *id))
    };
    let focused_graph = app
        .focused_graph_pane()
        .or_else(|| graph_rects.first().map(|(pane, _)| *pane));
    let content = focused_graph
        .filter(|_| !app.map_navigation_active())
        .and_then(|pane| {
            let cr = graph_rects
                .iter()
                .find(|(candidate, _)| *candidate == pane)
                .map(|(_, rect)| *rect)?;
            app.graph_pane_focused_member(pane)
                .filter(|id| contents.contains_key(id))
                .filter(|id| {
                    !tiles.iter().any(|(member, _)| member == id)
                        && !tiled_in_lens(id)
                        && !tile_paned(id)
                })
                .map(|node| (node, crate::surface::content_rect(cr)))
        });
    // The application title row remains visible when browser chrome is hidden.
    let chrome = Some(area);
    let mut surfaces = crate::surface::assemble(&base, &tiles, content, None);
    let mut appearance_roles: Vec<(uuid::Uuid, u64, Rect)> = pane_rects
        .iter()
        .filter_map(|(pane, rect)| match app.pane_content(*pane) {
            Some(PaneContent::Tile(node)) if contents.contains_key(node) => {
                Some((*node, 0x1000_0000_0000_0000 | pane.0, *rect))
            },
            _ => None,
        })
        .collect();
    for (pane, rect) in &workbench_panes {
        let geom = app
            .workbench_for_pane(*pane)
            .and_then(|workbench| workbench.to_arrangement().1);
        for cell in
            crate::workbench_tiling::place_reading_workbench(app, *pane, geom.as_ref(), *rect).cells
        {
            if let Some(node) = cell.active_member()
                && contents.contains_key(&node)
            {
                appearance_roles.push((
                    node,
                    crate::surface::appearance::workbench_appearance_role(*pane, node),
                    cell.body(),
                ));
            }
        }
    }
    if let Some((node, rect)) = content {
        let pane = focused_graph.expect("content inset has a graph pane");
        appearance_roles.push((node, 0x3000_0000_0000_0000 | pane.0, rect));
    }
    appearance_roles.extend(float_rects.iter().filter_map(|(pane, rect)| {
        match app.pane_content(*pane) {
            Some(PaneContent::Tile(node)) if contents.contains_key(node) => {
                Some((*node, 0x1000_0000_0000_0000 | pane.0, *rect))
            },
            _ => None,
        }
    }));
    for (id, rect) in float_rects {
        let Some(content) = app.pane_content(id).cloned() else {
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
    if let Some(rect) = chrome {
        surfaces.push(crate::surface::Surface {
            id: crate::surface::SurfaceId::CHROME,
            kind: SurfaceKind::Chrome,
            rect,
        });
    }
    crate::surface::appearance::assign_content_appearance_ids(&mut surfaces, 0, &appearance_roles);
    surfaces
}
