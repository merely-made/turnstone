// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Geometry projection for a [`super::SpaceBlueprint`].
//!
//! The blueprint stays the durable topology authority. This module only turns
//! its active tiled branch into pane and divider rectangles for Turnstone's
//! mixed-surface compositor; it does not construct a Cambium DOM or a
//! `TileTree`.

use crate::panes::{PaneId, SplitAxis};
use crate::surface::{Rect, Surface, SurfaceId, SurfaceKind};

use super::{
    FloatingPane, LayoutNode, LayoutPath, LayoutPathStep, PaneSource, SourceRef, SpaceBlueprint,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintPanePlacement {
    pub id: PaneId,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintDividerPlacement {
    pub index: u32,
    pub rect: Rect,
    pub area: Rect,
    pub path: LayoutPath,
    /// The divider follows this child in the N-ary split at `path`.
    pub after_child: usize,
    pub axis: SplitAxis,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlueprintFloatPlacement {
    pub id: PaneId,
    pub rect: Rect,
    pub z: u32,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlueprintTiling {
    pub panes: Vec<BlueprintPanePlacement>,
    pub dividers: Vec<BlueprintDividerPlacement>,
    /// Floats are in bottom-to-top order above tiled panes and dividers. Modal
    /// chrome remains a later shell layer, outside this blueprint projection.
    pub floats: Vec<BlueprintFloatPlacement>,
}

/// Place the active tiled portion of one space. Inactive tab children do not
/// receive a rectangle, which gives render, hit testing, accessibility, and
/// background pumping one common lifecycle gate.
pub fn place_space(
    space: &SpaceBlueprint,
    area: Rect,
    maximized: Option<PaneId>,
) -> BlueprintTiling {
    place_space_with_float_layer(space, area, maximized, true)
}

/// As [`place_space`], with the shell's transient float-layer toggle made
/// explicit. Pinned floats stay present even when the ordinary float layer is
/// hidden.
pub fn place_space_with_float_layer(
    space: &SpaceBlueprint,
    area: Rect,
    maximized: Option<PaneId>,
    float_layer_visible: bool,
) -> BlueprintTiling {
    let mut out = BlueprintTiling::default();
    if let Some(tree) = &space.tiled {
        walk(tree, area, &mut Vec::new(), &mut out);
    }
    if let Some(pane) = maximized
        && let Some(mut placement) = out
            .panes
            .iter()
            .find(|placement| placement.id == pane)
            .cloned()
    {
        placement.rect = area;
        out.panes = vec![placement];
        out.dividers.clear();
    }
    out.floats = space
        .visible_floating_panes(float_layer_visible)
        .into_iter()
        .map(|floating| BlueprintFloatPlacement {
            id: floating.pane,
            rect: resolve_float_rect(floating, area),
            z: floating.z,
            pinned: floating.pinned,
        })
        .collect();
    out
}

/// The outer compositor projection for one blueprint space. It deliberately
/// selects Turnstone surfaces directly instead of creating a `TileTree`: graph
/// canvases, live document sessions, and Cambium panes remain distinct surface
/// kinds while sharing the same serialized topology.
pub fn surface_plan_for_space(
    space: &SpaceBlueprint,
    area: Rect,
    maximized: Option<PaneId>,
) -> Vec<Surface> {
    surface_plan_for_space_with_float_layer(space, area, maximized, true)
}

/// Project tiled and visible floating stations into the compositor. Floats
/// append after the tiled root, so a hit test walks the same topmost order the
/// frame paints.
pub fn surface_plan_for_space_with_float_layer(
    space: &SpaceBlueprint,
    area: Rect,
    maximized: Option<PaneId>,
    float_layer_visible: bool,
) -> Vec<Surface> {
    let tiling = place_space_with_float_layer(space, area, maximized, float_layer_visible);
    let mut surfaces: Vec<_> = tiling
        .panes
        .into_iter()
        .filter_map(|placement| surface_for_pane(space, placement.id, placement.rect))
        .collect();
    surfaces.extend(tiling.dividers.into_iter().map(|divider| Surface {
        id: SurfaceId::divider(divider.index),
        kind: SurfaceKind::Divider(divider.index),
        rect: divider.rect,
    }));
    surfaces.extend(
        tiling
            .floats
            .into_iter()
            .filter_map(|floating| surface_for_pane(space, floating.id, floating.rect)),
    );
    surfaces
}

fn surface_for_pane(space: &SpaceBlueprint, pane: PaneId, rect: Rect) -> Option<Surface> {
    let spec = space.pane(pane)?;
    let kind = match spec.kind.as_str() {
        crate::panes::kind::GRAPH => SurfaceKind::Graph(pane),
        crate::panes::kind::TILE => match &spec.source {
            PaneSource::Fixed(SourceRef::Member { member, .. }) => SurfaceKind::Content(*member),
            _ => SurfaceKind::Pane(pane),
        },
        _ => SurfaceKind::Pane(pane),
    };
    Some(Surface {
        id: SurfaceId::for_kind(kind),
        kind,
        rect,
    })
}

fn resolve_float_rect(floating: &FloatingPane, area: Rect) -> Rect {
    let width = constrained_extent(
        area.w,
        floating.rect.width,
        floating.constraints.min_width,
        floating.constraints.max_width,
    );
    let height = constrained_extent(
        area.h,
        floating.rect.height,
        floating.constraints.min_height,
        floating.constraints.max_height,
    );
    let x = (area.x + finite(floating.rect.x) * area.w).clamp(area.x, area.x + area.w - width);
    let y = (area.y + finite(floating.rect.y) * area.h).clamp(area.y, area.y + area.h - height);
    Rect::new(x, y, width, height)
}

fn constrained_extent(available: f32, proportion: f32, min: f32, max: Option<f32>) -> f32 {
    let available = available.max(0.0);
    let min = finite(min).max(0.0).min(available);
    let max = max.map(finite).unwrap_or(available).max(min).min(available);
    (available * finite(proportion).max(0.0)).clamp(min, max)
}

fn finite(value: f32) -> f32 {
    value.is_finite().then_some(value).unwrap_or_default()
}

fn walk(node: &LayoutNode, area: Rect, path: &mut LayoutPath, out: &mut BlueprintTiling) {
    match node {
        LayoutNode::Pane(id) => out.panes.push(BlueprintPanePlacement {
            id: *id,
            rect: area,
        }),
        LayoutNode::Split { axis, children } => place_split(*axis, children, area, path, out),
        LayoutNode::Tabs { children, active } => {
            let active = (*active).min(children.len().saturating_sub(1));
            if let Some(child) = children.get(active) {
                path.push(LayoutPathStep::Tab(active));
                walk(child, area, path, out);
                path.pop();
            }
        }
        LayoutNode::Grid {
            children,
            columns,
            shares,
        } => place_grid(children, *columns, shares, area, path, out),
    }
}

fn place_split(
    axis: SplitAxis,
    children: &[super::LayoutBranch],
    area: Rect,
    path: &mut LayoutPath,
    out: &mut BlueprintTiling,
) {
    if children.len() == 1 {
        path.push(LayoutPathStep::Split(0));
        walk(&children[0].tree, area, path, out);
        path.pop();
        return;
    }
    let seam = seam_extent(axis, area);
    let available =
        (axis_extent(axis, area) - seam * (children.len().saturating_sub(1) as f32)).max(0.0);
    let total: f32 = children
        .iter()
        .map(|child| sane_fraction(child.fraction))
        .sum();
    let mut cursor = axis_origin(axis, area);
    let mut remaining = available;
    for (index, child) in children.iter().enumerate() {
        let extent = if index + 1 == children.len() {
            remaining
        } else {
            let share = available * sane_fraction(child.fraction) / total;
            remaining -= share;
            share
        };
        let child_area = with_axis_extent(axis, area, cursor, extent);
        path.push(LayoutPathStep::Split(index));
        walk(&child.tree, child_area, path, out);
        path.pop();
        cursor += extent;
        if index + 1 != children.len() {
            let divider = with_axis_extent(axis, area, cursor, seam);
            out.dividers.push(BlueprintDividerPlacement {
                index: out.dividers.len() as u32,
                rect: divider,
                area,
                path: path.clone(),
                after_child: index,
                axis,
            });
            cursor += seam;
        }
    }
}

fn place_grid(
    children: &[LayoutNode],
    columns: usize,
    shares: &super::GridShares,
    area: Rect,
    path: &mut LayoutPath,
    out: &mut BlueprintTiling,
) {
    let columns = columns.clamp(1, children.len().max(1));
    let rows = children.len().div_ceil(columns);
    let widths = distributed(area.w, &shares.columns, columns);
    let heights = distributed(area.h, &shares.rows, rows);
    let mut y = area.y;
    for row in 0..rows {
        let mut x = area.x;
        for column in 0..columns {
            let index = row * columns + column;
            let Some(child) = children.get(index) else {
                break;
            };
            path.push(LayoutPathStep::Grid(index));
            walk(
                child,
                Rect::new(x, y, widths[column], heights[row]),
                path,
                out,
            );
            path.pop();
            x += widths[column];
        }
        y += heights[row];
    }
}

fn seam_extent(axis: SplitAxis, area: Rect) -> f32 {
    let divider = crate::pane::cambium_split(axis, 0.5).divider_rect(area.w, area.h);
    match axis {
        SplitAxis::Horizontal => divider[2],
        SplitAxis::Vertical => divider[3],
    }
}

fn axis_extent(axis: SplitAxis, area: Rect) -> f32 {
    match axis {
        SplitAxis::Horizontal => area.w,
        SplitAxis::Vertical => area.h,
    }
}

fn axis_origin(axis: SplitAxis, area: Rect) -> f32 {
    match axis {
        SplitAxis::Horizontal => area.x,
        SplitAxis::Vertical => area.y,
    }
}

fn with_axis_extent(axis: SplitAxis, area: Rect, origin: f32, extent: f32) -> Rect {
    match axis {
        SplitAxis::Horizontal => Rect::new(origin, area.y, extent, area.h),
        SplitAxis::Vertical => Rect::new(area.x, origin, area.w, extent),
    }
}

fn distributed(total: f32, shares: &[f32], len: usize) -> Vec<f32> {
    let values: Vec<_> = (0..len)
        .map(|index| sane_fraction(*shares.get(index).unwrap_or(&1.0)))
        .collect();
    let sum: f32 = values.iter().sum();
    values
        .into_iter()
        .map(|value| total * value / sum)
        .collect()
}

fn sane_fraction(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}
