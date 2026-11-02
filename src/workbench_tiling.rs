// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Workbench geometry: platen's `TreeGeometry` walked into pixel rects within
//! the Workbench pane (rung 5 slice E, option (b): the cambium path).
//!
//! `platen::Workbench` owns the tiling MODEL (the split tree of tab-stacks,
//! the active tab per stack, every mutator) and is deliberately geometry-free:
//! fractions, never rects. This module is the host-side geometry, the same
//! role `pane.rs` plays for frisket: walk the canonical `TreeGeometry` (the
//! persistence pair's layout half, derived from the live tree by
//! `to_arrangement`) into placed cells and the divider bands between split
//! children.
//!
//! Pure: a function of the geometry and the pane's area. No platen mutation —
//! the model's ops are platen's own; this only reads. Splits here are N-ary
//! (platen's `children: Vec<TreeBranch>`), unlike frisket's binary tree, so a
//! divider band sits between each adjacent pair and a drag re-weights exactly
//! that pair (the others keep their shares).

use mere::platen::{Axis, TreeGeometry};
use uuid::Uuid;

use crate::surface::Rect;

/// The painted seam shares the outer frame's hairline width.
pub const WB_SEAM: f32 = crate::pane::DIVIDER_WIDTH;

/// One placed cell: a tab-stack at its rect. The tab bar occupies the top
/// `ui::TABLIST_HEIGHT` of `rect`; [`CellPlacement::body`] is what remains.
#[derive(Clone, Debug, PartialEq)]
pub struct CellPlacement {
    pub chrome_scale: f32,
    pub reading_toolbar_height: f32,
    pub tab_edge: crate::chrome_layout::TabEdge,
    pub tab_policy: cambium::TabStripPolicy,
    /// The stack's members in tab order.
    pub members: Vec<Uuid>,
    /// Index of the active (visible) tab.
    pub active: usize,
    /// The whole cell, tab bar included, pane-local.
    pub rect: Rect,
    /// Path from the root to this leaf (the child index taken at each split
    /// level — platen's `split_fractions` path vocabulary).
    pub path: Vec<usize>,
}

impl CellPlacement {
    /// The active member, when the stack is non-empty.
    pub fn active_member(&self) -> Option<Uuid> {
        self.members
            .get(self.active.min(self.members.len().saturating_sub(1)))
            .copied()
    }

    /// The cell's body rect (below the tab bar), pane-local.
    pub fn toolbar_rect(&self) -> Rect {
        let rect = self.stack_body();
        Rect::new(
            rect.x,
            rect.y,
            rect.w,
            self.reading_toolbar_height.min(rect.h),
        )
    }
    pub fn body(&self) -> Rect {
        let rect = self.stack_body();
        let toolbar = self.toolbar_rect();
        Rect::new(
            rect.x,
            rect.y + toolbar.h,
            rect.w,
            (rect.h - toolbar.h).max(0.0),
        )
    }
    fn stack_body(&self) -> Rect {
        use crate::chrome_layout::TabEdge;
        let tabs = self.tab_rect();
        match self.tab_edge {
            TabEdge::Top => Rect::new(
                self.rect.x,
                self.rect.y + tabs.h,
                self.rect.w,
                (self.rect.h - tabs.h).max(0.0),
            ),
            TabEdge::Bottom => Rect::new(
                self.rect.x,
                self.rect.y,
                self.rect.w,
                (self.rect.h - tabs.h).max(0.0),
            ),
            TabEdge::Left => Rect::new(
                self.rect.x + tabs.w,
                self.rect.y,
                (self.rect.w - tabs.w).max(0.0),
                self.rect.h,
            ),
            TabEdge::Right => Rect::new(
                self.rect.x,
                self.rect.y,
                (self.rect.w - tabs.w).max(0.0),
                self.rect.h,
            ),
        }
    }

    pub fn tab_rect(&self) -> Rect {
        use crate::chrome_layout::TabEdge;
        if !self.tab_policy.visible(self.members.len()) {
            return Rect::new(self.rect.x, self.rect.y, 0.0, 0.0);
        }
        let height = (crate::ui::TABLIST_HEIGHT * self.chrome_scale).min(self.rect.h);
        let width = (cambium::ShellMetrics::DOCUMENT_RAIL_MAX_WIDTH * self.chrome_scale)
            .min(self.rect.w * 0.30);
        match self.tab_edge {
            TabEdge::Top => Rect::new(self.rect.x, self.rect.y, self.rect.w, height),
            TabEdge::Bottom => Rect::new(
                self.rect.x,
                self.rect.y + self.rect.h - height,
                self.rect.w,
                height,
            ),
            TabEdge::Left => Rect::new(self.rect.x, self.rect.y, width, self.rect.h),
            TabEdge::Right => Rect::new(
                self.rect.x + self.rect.w - width,
                self.rect.y,
                width,
                self.rect.h,
            ),
        }
    }
}

/// One divider band between two adjacent children of an N-ary split. Carries
/// what a drag needs: the split's path (to aim `set_split_fractions`), which
/// pair it re-weights, the pair's fractions, and the split's own rect (to turn
/// a pointer point into new fractions).
#[derive(Clone, Debug, PartialEq)]
pub struct WbDivider {
    /// Walk-order index (the surface/input key for this band).
    pub index: u32,
    /// The band itself, pane-local.
    pub rect: Rect,
    /// The split's container rect, pane-local.
    pub area: Rect,
    /// Path from the root to the SPLIT node.
    pub path: Vec<usize>,
    /// The band sits between children `pair` and `pair + 1`.
    pub pair: usize,
    pub axis: Axis,
    /// ALL the split's fractions (a drag rewrites the pair, keeps the rest).
    pub fractions: Vec<f32>,
}

/// A walked workbench: the cells and the seams between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbenchTiling {
    pub cells: Vec<CellPlacement>,
    pub dividers: Vec<WbDivider>,
}

impl WorkbenchTiling {
    /// The cell containing pane-local `(x, y)`, if any.
    pub fn cell_at(&self, x: f32, y: f32) -> Option<&CellPlacement> {
        self.cells.iter().find(|c| c.rect.contains(x, y))
    }

    /// The generous resize target around the painted divider, pane-local.
    pub fn divider_at(&self, x: f32, y: f32) -> Option<&WbDivider> {
        self.dividers.iter().rev().find(|d| {
            d.area.contains(x, y)
                && crate::pane::divider_grab_rect(d.rect, d.axis == Axis::Row).contains(x, y)
        })
    }
}

/// Walk `geom` into placed cells within `area` (pane-local coordinates).
/// `None` (an empty workbench) yields an empty tiling.
pub fn place_workbench(geom: Option<&TreeGeometry>, area: Rect) -> WorkbenchTiling {
    place_workbench_at(geom, area, crate::chrome_layout::TabEdge::Top)
}

pub fn place_workbench_at(
    geom: Option<&TreeGeometry>,
    area: Rect,
    edge: crate::chrome_layout::TabEdge,
) -> WorkbenchTiling {
    let mut out = WorkbenchTiling::default();
    if let Some(geom) = geom {
        walk(geom, area, &mut Vec::new(), &mut out);
    }
    for cell in &mut out.cells {
        cell.tab_edge = edge;
    }
    out
}

/// One geometry path for painted controls, document holes and input planning.
pub(crate) fn place_reading_workbench(
    app: &crate::app::App,
    pane: crate::panes::PaneId,
    geom: Option<&TreeGeometry>,
    area: Rect,
) -> WorkbenchTiling {
    let mut tiling = place_workbench_at(geom, area, app.shell_chrome_config().layout.document_tabs);
    let scale = app.pane_layout_scale(pane);
    for cell in &mut tiling.cells {
        cell.chrome_scale = scale;
        cell.tab_policy = cambium::TabStripPolicy::Multiple;
        let ui = cell.active_member().and_then(|member| {
            app.reading
                .find
                .get(&crate::reading_view::ReadTarget { pane, member })
        });
        let width = cell.stack_body().w / scale;
        let header = if width >= 240.0 { 32.0 } else { 64.0 };
        let appearance = if ui.is_some_and(|ui| ui.appearance_open) {
            if width >= 460.0 {
                32.0
            } else if width >= 180.0 {
                64.0
            } else {
                96.0
            }
        } else {
            0.0
        };
        cell.reading_toolbar_height = (header
            + appearance
            + if ui.is_some_and(|ui| ui.open) {
                32.0
            } else {
                0.0
            })
            * scale;
    }
    tiling
}

fn walk(node: &TreeGeometry, area: Rect, path: &mut Vec<usize>, out: &mut WorkbenchTiling) {
    match node {
        TreeGeometry::Stack { members, active } => out.cells.push(CellPlacement {
            chrome_scale: 1.0,
            reading_toolbar_height: 0.0,
            tab_edge: Default::default(),
            tab_policy: Default::default(),
            members: members.clone(),
            active: *active,
            rect: area,
            path: path.clone(),
        }),
        TreeGeometry::Split { axis, children } => {
            if children.is_empty() {
                return;
            }
            // Normalize the fractions defensively (sanitize() promises a sum of
            // 1, but a walked frame must never divide by drift).
            let sum: f32 = children.iter().map(|b| b.fraction.max(0.0)).sum();
            let fractions: Vec<f32> = if sum > 0.0 {
                children.iter().map(|b| b.fraction.max(0.0) / sum).collect()
            } else {
                vec![1.0 / children.len() as f32; children.len()]
            };
            let seams = WB_SEAM * (children.len() - 1) as f32;
            let along = match axis {
                Axis::Row => (area.w - seams).max(0.0),
                Axis::Column => (area.h - seams).max(0.0),
            };
            let mut cursor = match axis {
                Axis::Row => area.x,
                Axis::Column => area.y,
            };
            for (i, (branch, frac)) in children.iter().zip(&fractions).enumerate() {
                // Round each child's span so cells and seams tile exactly.
                let end = match axis {
                    Axis::Row => area.x + area.w,
                    Axis::Column => area.y + area.h,
                };
                let span = if i + 1 == children.len() {
                    (end - cursor).max(0.0)
                } else {
                    (along * frac).round()
                };
                let child_rect = match axis {
                    Axis::Row => Rect::new(cursor, area.y, span, area.h),
                    Axis::Column => Rect::new(area.x, cursor, area.w, span),
                };
                path.push(i);
                walk(&branch.node, child_rect, path, out);
                path.pop();
                cursor += span;
                if i + 1 < children.len() {
                    let band = match axis {
                        Axis::Row => Rect::new(cursor, area.y, WB_SEAM, area.h),
                        Axis::Column => Rect::new(area.x, cursor, area.w, WB_SEAM),
                    };
                    out.dividers.push(WbDivider {
                        index: out.dividers.len() as u32,
                        rect: band,
                        area,
                        path: path.clone(),
                        pair: i,
                        axis: *axis,
                        fractions: fractions.clone(),
                    });
                    cursor += WB_SEAM;
                }
            }
        },
    }
}

/// The action a workbench tab drop resolves to, from WHERE in the target cell
/// it released. A body edge band (the outer quarter on each side) splits: out
/// of its OWN cell when the drop cell already holds the dragged tab
/// (platen's `split_out`), beside the target otherwise (`split_beside_axis`).
/// The tab bar or the body's centre stacks into a DIFFERENT cell; on the own
/// cell it is no drop at all (`None` — the shell treats it as a click). Pure,
/// so the zones are testable; the shell calls it at the tab gesture's release.
pub fn wb_drop_action(
    dragged: Uuid,
    target: Uuid,
    cell: &CellPlacement,
    lx: f32,
    ly: f32,
) -> Option<crate::action::Action> {
    use crate::action::{Action, WbAxis};
    let same_cell = cell.members.contains(&dragged);
    let body = cell.body();
    let stack = || (!same_cell).then_some(Action::WorkbenchStackOnto { dragged, target });
    if !body.contains(lx, ly) {
        return stack();
    }
    const EDGE: f32 = 0.25;
    let fx = (lx - body.x) / body.w.max(1.0);
    let fy = (ly - body.y) / body.h.max(1.0);
    let (axis, after) = if fx < EDGE {
        (Some(WbAxis::Row), false)
    } else if fx > 1.0 - EDGE {
        (Some(WbAxis::Row), true)
    } else if fy < EDGE {
        (Some(WbAxis::Column), false)
    } else if fy > 1.0 - EDGE {
        (Some(WbAxis::Column), true)
    } else {
        (None, false)
    };
    match axis {
        Some(axis) if same_cell => Some(Action::WorkbenchSplitOut {
            dragged,
            axis,
            after,
        }),
        Some(axis) => Some(Action::WorkbenchSplitBeside {
            dragged,
            target,
            axis,
            after,
        }),
        None => stack(),
    }
}

/// New fractions for a divider drag: the pointer at pane-local `(x, y)` moves
/// `div`'s pair boundary; the pair re-weights (each side clamped to a minimum
/// share so neither collapses), every other child keeps its fraction.
pub fn drag_fractions(div: &WbDivider, x: f32, y: f32) -> Vec<f32> {
    let n = div.fractions.len();
    let seams = WB_SEAM * (n.saturating_sub(1)) as f32;
    let (along, origin) = match div.axis {
        Axis::Row => ((div.area.w - seams).max(1.0), div.area.x),
        Axis::Column => ((div.area.h - seams).max(1.0), div.area.y),
    };
    // The boundary's position along the axis, in content (seam-less) space:
    // the spans of the children before the pair, plus the pointer's offset
    // into the pair's combined span.
    let before: f32 = div.fractions[..div.pair].iter().sum();
    let pair_total = div.fractions[div.pair] + div.fractions[div.pair + 1];
    let pointer = match div.axis {
        Axis::Row => x,
        Axis::Column => y,
    };
    // Subtract the seams before this boundary to land in content space.
    let content_pos = (pointer - origin) - WB_SEAM * div.pair as f32;
    let mut first = content_pos / along - before;
    // Clamp: neither side of the pair below 10% of the pair's own share.
    let min = pair_total * 0.1;
    first = first.clamp(min, pair_total - min);
    let mut out = div.fractions.clone();
    out[div.pair] = first;
    out[div.pair + 1] = pair_total - first;
    out
}

#[cfg(test)]
#[path = "workbench_tiling/tests.rs"]
mod tests;
