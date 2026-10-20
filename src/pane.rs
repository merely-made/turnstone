// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The pane tree: frisket's ratio tree walked into pixel rects. Rung 5 slice C.
//!
//! `frisket` owns the pane MODEL (the `PaneNode` split tree, `PaneContent`,
//! `FrisketLayout`) and is deliberately geometry-free: it emits fractions, never
//! rects, so a host decides the pixels. This module is that host-side geometry.
//! It walks a `FrisketLayout` into a flat list of placed panes, which the shell
//! turns into surfaces the same way slice A places the canvas and chrome.
//!
//! Pure: a function of the layout, the area, and which pane is maximized. No GPU,
//! no frisket mutation. The layout OPERATIONS (summon, close, set-divider) are
//! frisket's own; this only reads.
//!
//! The split GEOMETRY is cambium's (`Split::slots` / `divider_rect`) — the
//! catalog's split component, pulled 2026-07-17. turnstone composites surfaces,
//! so it consumes the component's state math rather than its view (the math is
//! the single geometry truth; the view exists for in-tree consumers like the
//! Workbench's platen tiling), and each seam becomes a divider surface that
//! takes the drag.

use crate::panes::{FrisketLayout, PaneContent, PaneId, PaneNode, SplitAxis, SplitChoice};

use crate::surface::Rect;

/// Paint a hairline; pointer admission uses a wider band around it.
pub const DIVIDER_WIDTH: f32 = 1.0;
pub const DIVIDER_GRAB_WIDTH: f32 = 12.0;

pub(crate) fn divider_grab_rect(rect: Rect, vertical: bool) -> Rect {
    if vertical {
        Rect::new(
            rect.x + (rect.w - DIVIDER_GRAB_WIDTH) / 2.0,
            rect.y,
            DIVIDER_GRAB_WIDTH,
            rect.h,
        )
    } else {
        Rect::new(
            rect.x,
            rect.y + (rect.h - DIVIDER_GRAB_WIDTH) / 2.0,
            rect.w,
            DIVIDER_GRAB_WIDTH,
        )
    }
}

/// One pane placed in the window: its identity, what it shows, the rect it
/// occupies, and the path to its leaf (so a divider or close op can name it).
#[derive(Clone, Debug, PartialEq)]
pub struct PanePlacement {
    pub id: PaneId,
    pub content: PaneContent,
    /// The graph authority this pane resolves, independent of its screen rect.
    /// Window composition never infers graph identity from pane kind.
    pub graph_id: crate::panes::GraphId,
    pub rect: Rect,
    /// Path from the root to this leaf. Empty = the whole tree is one leaf.
    pub path: Vec<SplitChoice>,
}

/// One divider band placed in the window: the seam between a split's slots.
/// Carries everything a drag needs — the split's own container rect (so
/// `Split::ratio_at` can turn a pointer point into a ratio) and the path to
/// the split node (so the ratio lands on the right divider).
#[derive(Clone, Debug, PartialEq)]
pub struct DividerPlacement {
    /// Walk-order index; the surface plan's `SurfaceKind::Divider` carries it.
    pub index: u32,
    /// The divider band itself, window-space.
    pub rect: Rect,
    /// The split's container rect, window-space.
    pub area: Rect,
    /// Path from the root to the SPLIT node (not a leaf).
    pub path: Vec<SplitChoice>,
    pub axis: SplitAxis,
    pub ratio: f32,
}

/// A walked layout: the panes and the seams between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaneTiling {
    pub panes: Vec<PanePlacement>,
    pub dividers: Vec<DividerPlacement>,
}

/// cambium's split state for a frisket split node — the one place the two
/// vocabularies meet. Same axis names, same meaning.
pub fn cambium_split(axis: SplitAxis, ratio: f32) -> cambium::Split {
    let cam_axis = match axis {
        SplitAxis::Horizontal => cambium::SplitAxis::Horizontal,
        SplitAxis::Vertical => cambium::SplitAxis::Vertical,
    };
    let mut split = cambium::Split::new(cam_axis, ratio);
    split.thickness = DIVIDER_WIDTH;
    split
}

/// Walk a layout into placed panes within `area`. When `maximized` names a pane
/// present in the tree, that pane takes the whole area and the rest are dropped
/// (frisket has no maximize op; it is a host view state).
pub fn place_panes(layout: &FrisketLayout, area: Rect, maximized: Option<PaneId>) -> PaneTiling {
    let mut out = PaneTiling::default();
    walk(&layout.root, area, &mut Vec::new(), &mut out);
    if let Some(mid) = maximized
        && let Some(mut placed) = out.panes.iter().find(|p| p.id == mid).cloned()
    {
        // A maximized pane has no visible seams, so no dividers either.
        placed.rect = area;
        return PaneTiling {
            panes: vec![placed],
            dividers: Vec::new(),
        };
    }
    out
}

/// The path from the root to the leaf carrying `id`, or `None` if no leaf does.
/// Empty path = the whole tree is that leaf. Used to aim a summon/close/divider
/// op (frisket's ops take a `SplitPath`).
pub fn path_of(layout: &FrisketLayout, id: PaneId) -> Option<Vec<SplitChoice>> {
    crate::panes::path_for_pane_id(&layout.root, id)
}

fn walk(node: &PaneNode, area: Rect, path: &mut Vec<SplitChoice>, out: &mut PaneTiling) {
    match node {
        PaneNode::Workspace(frame) => {
            walk_workspace(frame, frame.workspace.tiled(), area, path, out)
        },
        PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } => out.panes.push(PanePlacement {
            id: *pane_id,
            content: content.clone(),
            graph_id: *graph_id,
            rect: area,
            path: path.clone(),
        }),
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            // cambium's split math is the geometry truth: slots + divider band,
            // offset from the split's own origin into window space.
            let split = cambium_split(*axis, *ratio);
            let (a, b) = split.slots(area.w, area.h);
            let d = split.divider_rect(area.w, area.h);
            let at = |r: [f32; 4]| Rect::new(area.x + r[0], area.y + r[1], r[2], r[3]);
            out.dividers.push(DividerPlacement {
                index: out.dividers.len() as u32,
                rect: at(d),
                area,
                path: path.clone(),
                axis: *axis,
                ratio: *ratio,
            });
            path.push(SplitChoice::First);
            walk(first, at(a), path, out);
            path.pop();
            path.push(SplitChoice::Second);
            walk(second, at(b), path, out);
            path.pop();
        },
    }
}

fn walk_workspace(
    frame: &crate::panes::workspace_bridge::OuterWorkspace,
    tree: &workbench::TileTree,
    area: Rect,
    path: &mut Vec<SplitChoice>,
    out: &mut PaneTiling,
) {
    match tree {
        workbench::TileTree::Stack(stack) => {
            if let Some(tile) = stack.tabs.get(stack.active) {
                if let Some((content, graph_id)) = frame.panes.get(&tile.id.0) {
                    out.panes.push(PanePlacement {
                        id: PaneId(tile.id.0),
                        content: content.clone(),
                        graph_id: *graph_id,
                        rect: area,
                        path: path.clone(),
                    });
                }
            }
        },
        workbench::TileTree::Split { axis, children } => {
            let row = *axis == workbench::SplitAxis::Row;
            let span = if row { area.w } else { area.h };
            let available =
                (span - DIVIDER_WIDTH * children.len().saturating_sub(1) as f32).max(0.0);
            let total: f32 = children.iter().map(|child| child.fraction).sum();
            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                let size = available * child.fraction / total;
                let rect = if row {
                    Rect::new(area.x + offset, area.y, size, area.h)
                } else {
                    Rect::new(area.x, area.y + offset, area.w, size)
                };
                path.push(match i {
                    0 => SplitChoice::First,
                    1 => SplitChoice::Second,
                    i => SplitChoice::Child(i),
                });
                walk_workspace(frame, &child.tree, rect, path, out);
                path.pop();
                offset += size;
                if i + 1 < children.len() {
                    // Legacy ratio commands are retained for binary splits;
                    // the shared retained frame handles N-ary divider input.
                    if children.len() == 2 {
                        out.dividers.push(DividerPlacement {
                            index: out.dividers.len() as u32,
                            rect: if row {
                                Rect::new(area.x + offset, area.y, DIVIDER_WIDTH, area.h)
                            } else {
                                Rect::new(area.x, area.y + offset, area.w, DIVIDER_WIDTH)
                            },
                            area,
                            path: path.clone(),
                            axis: if row {
                                SplitAxis::Horizontal
                            } else {
                                SplitAxis::Vertical
                            },
                            ratio: children[0].fraction / total,
                        });
                    }
                    offset += DIVIDER_WIDTH;
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::{InsertSide, PaneContent};

    fn leaf(id: u64, content: PaneContent) -> PaneNode {
        PaneNode::Leaf {
            pane_id: PaneId(id),
            content,
            graph_id: crate::panes::GraphId::nil(),
        }
    }

    fn area() -> Rect {
        Rect::new(0.0, 0.0, 1000.0, 800.0)
    }

    #[test]
    fn a_single_leaf_takes_the_whole_area() {
        let layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: leaf(1, PaneContent::Orrery),
        };
        let tiling = place_panes(&layout, area(), None);
        assert_eq!(tiling.panes.len(), 1);
        assert!(tiling.dividers.is_empty(), "one leaf has no seams");
        assert_eq!(tiling.panes[0].rect, area());
        assert_eq!(tiling.panes[0].content, PaneContent::Orrery);
        assert!(tiling.panes[0].path.is_empty());
    }

    #[test]
    fn placement_carries_the_leaf_graph_binding() {
        let graph = crate::panes::GraphId::from_uuid(uuid::Uuid::from_u128(42));
        let layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: PaneNode::Leaf {
                pane_id: PaneId(8),
                content: PaneContent::Orrery,
                graph_id: graph,
            },
        };

        let tiling = place_panes(&layout, area(), None);
        assert_eq!(tiling.panes[0].id, PaneId(8));
        assert_eq!(tiling.panes[0].graph_id, graph);
    }

    #[test]
    fn a_horizontal_split_halves_the_width_by_ratio() {
        let mut layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: leaf(1, PaneContent::Orrery),
        };
        // Summon a Roster to the right of the Orrery: a horizontal split.
        assert!(layout.summon_leaf(&[], InsertSide::Right, leaf(2, PaneContent::Roster)));
        assert!(
            matches!(&layout.root, PaneNode::Workspace(_)),
            "summon publishes shared Workspace"
        );
        let tiling = place_panes(&layout, area(), None);
        assert_eq!(tiling.panes.len(), 2);
        assert_eq!(tiling.dividers.len(), 1, "one split, one seam");
        // Shared Workspace divides the usable width around a one-pixel seam.
        let orrery = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Orrery)
            .unwrap();
        let roster = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Roster)
            .unwrap();
        let seam = &tiling.dividers[0];
        assert_eq!(orrery.rect, Rect::new(0.0, 0.0, 499.5, 800.0));
        assert_eq!(seam.rect, Rect::new(499.5, 0.0, 1.0, 800.0));
        assert_eq!(roster.rect, Rect::new(500.5, 0.0, 499.5, 800.0));
        // Panes + seam tile the area exactly: no gap, no overlap.
        assert_eq!(orrery.rect.x + orrery.rect.w, seam.rect.x);
        assert_eq!(seam.rect.x + seam.rect.w, roster.rect.x);
        assert_eq!(roster.rect.x + roster.rect.w, 1000.0);
        assert!(
            seam.path.is_empty(),
            "the root split's seam has the root path"
        );
    }

    #[test]
    fn a_divider_ratio_moves_the_seam() {
        let mut layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: leaf(1, PaneContent::Orrery),
        };
        layout.summon_leaf(&[], InsertSide::Right, leaf(2, PaneContent::Roster));
        assert!(layout.set_split_ratio(&[], 0.7));
        assert!(
            matches!(&layout.root, PaneNode::Workspace(_)),
            "ratio operation keeps shared Workspace"
        );
        let tiling = place_panes(&layout, area(), None);
        let orrery = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Orrery)
            .unwrap();
        // Fractions allocate 0.7 of the 999 usable pixels.
        assert_eq!(orrery.rect.w, 699.3);
        assert_eq!(tiling.dividers[0].rect, Rect::new(699.3, 0.0, 1.0, 800.0));
        let roster = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Roster)
            .unwrap();
        assert_eq!(roster.rect, Rect::new(700.3, 0.0, 299.7, 800.0));
        assert_eq!(roster.rect.x + roster.rect.w, area().w);
        assert_eq!(tiling.dividers[0].ratio, 0.7);
    }

    #[test]
    fn maximize_gives_one_pane_the_whole_area() {
        let mut layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: leaf(1, PaneContent::Orrery),
        };
        layout.summon_leaf(&[], InsertSide::Right, leaf(2, PaneContent::Roster));
        let tiling = place_panes(&layout, area(), Some(PaneId(2)));
        assert_eq!(tiling.panes.len(), 1);
        assert!(
            tiling.dividers.is_empty(),
            "a maximized pane hides the seams"
        );
        assert_eq!(tiling.panes[0].id, PaneId(2));
        assert_eq!(tiling.panes[0].rect, area());
    }

    #[test]
    fn a_vertical_split_halves_the_height() {
        let mut layout = FrisketLayout {
            id: crate::panes::FrisketId::new("t"),
            label: "t".into(),
            root: leaf(1, PaneContent::Orrery),
        };
        layout.summon_leaf(&[], InsertSide::Below, leaf(2, PaneContent::Trail));
        assert!(
            matches!(&layout.root, PaneNode::Workspace(_)),
            "vertical summon publishes shared Workspace"
        );
        let tiling = place_panes(&layout, area(), None);
        let top = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Orrery)
            .unwrap();
        let bottom = tiling
            .panes
            .iter()
            .find(|p| p.content == PaneContent::Trail)
            .unwrap();
        // The one-pixel seam leaves 399.5 pixels per branch.
        assert_eq!(top.rect, Rect::new(0.0, 0.0, 1000.0, 399.5));
        assert_eq!(bottom.rect, Rect::new(0.0, 400.5, 1000.0, 399.5));
        assert_eq!(tiling.dividers[0].rect, Rect::new(0.0, 399.5, 1000.0, 1.0));
    }
    #[test]
    fn legacy_binary_splits_use_the_same_hairline_geometry() {
        for (axis, ratio, first, seam, second) in [
            (
                SplitAxis::Horizontal,
                0.5,
                Rect::new(0.0, 0.0, 500.0, 800.0),
                Rect::new(500.0, 0.0, 1.0, 800.0),
                Rect::new(501.0, 0.0, 499.0, 800.0),
            ),
            (
                SplitAxis::Vertical,
                0.5,
                Rect::new(0.0, 0.0, 1000.0, 400.0),
                Rect::new(0.0, 400.0, 1000.0, 1.0),
                Rect::new(0.0, 401.0, 1000.0, 399.0),
            ),
            (
                SplitAxis::Horizontal,
                0.7,
                Rect::new(0.0, 0.0, 699.0, 800.0),
                Rect::new(699.0, 0.0, 1.0, 800.0),
                Rect::new(700.0, 0.0, 300.0, 800.0),
            ),
        ] {
            // A persisted legacy Split is not the new summon-operation fixture.
            let layout = FrisketLayout {
                id: crate::panes::FrisketId::new("legacy"),
                label: "legacy".into(),
                root: PaneNode::Split {
                    axis,
                    ratio,
                    first: Box::new(leaf(1, PaneContent::Orrery)),
                    second: Box::new(leaf(2, PaneContent::Roster)),
                },
            };
            assert!(matches!(&layout.root, PaneNode::Split { .. }));
            let placed = place_panes(&layout, area(), None);
            assert_eq!(placed.panes.len(), 2);
            assert_eq!(placed.dividers.len(), 1);
            assert_eq!(placed.panes[0].rect, first);
            assert_eq!(placed.dividers[0].rect, seam);
            assert_eq!(placed.panes[1].rect, second);
            assert_eq!(placed.panes[0].path, vec![SplitChoice::First]);
            assert_eq!(placed.panes[1].path, vec![SplitChoice::Second]);
            assert_eq!(placed.dividers[0].ratio, ratio);
            if axis == SplitAxis::Horizontal {
                assert_eq!(first.x + first.w, seam.x);
                assert_eq!(seam.x + seam.w, second.x);
                assert_eq!(second.x + second.w, area().w);
            } else {
                assert_eq!(first.y + first.h, seam.y);
                assert_eq!(seam.y + seam.h, second.y);
                assert_eq!(second.y + second.h, area().h);
            }
        }
    }
}
