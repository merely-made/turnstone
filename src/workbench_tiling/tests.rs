// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use mere::platen::{TreeBranch, Workbench};

fn area() -> Rect {
    Rect::new(0.0, 0.0, 800.0, 600.0)
}

fn geom_of(wb: &Workbench) -> Option<TreeGeometry> {
    wb.to_arrangement().1
}

#[test]
fn an_empty_workbench_places_nothing() {
    let t = place_workbench(None, area());
    assert!(t.cells.is_empty() && t.dividers.is_empty());
}

#[test]
fn every_tab_edge_partitions_the_cell_without_covering_document_content() {
    use crate::chrome_layout::TabEdge;
    let mut wb = Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(Uuid::new_v4());
    for edge in [TabEdge::Top, TabEdge::Bottom, TabEdge::Left, TabEdge::Right] {
        let tiling = place_workbench_at(
            geom_of(&wb).as_ref(),
            Rect::new(20.0, 30.0, 410.0, 260.0),
            edge,
        );
        let cell = &tiling.cells[0];
        let body = cell.body();
        let tabs = cell.tab_rect();
        assert!((body.w * body.h + tabs.w * tabs.h - cell.rect.w * cell.rect.h).abs() < 0.01);
        assert!(!body.contains(tabs.x + tabs.w / 2.0, tabs.y + tabs.h / 2.0));
        assert!(
            cell.rect
                .contains(body.x + body.w / 2.0, body.y + body.h / 2.0)
        );
    }
}

#[test]
fn two_tiles_split_side_by_side_with_one_seam() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut wb = Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(a);
    wb.open_tile(b);
    let t = place_workbench(geom_of(&wb).as_ref(), area());
    assert_eq!(t.cells.len(), 2);
    assert_eq!(t.dividers.len(), 1);
    let (l, r) = (&t.cells[0], &t.cells[1]);
    assert_eq!(l.members, vec![a]);
    assert_eq!(r.members, vec![b]);
    // Cells + seam tile the width exactly: no gap, no overlap.
    assert_eq!(l.rect.x + l.rect.w, t.dividers[0].rect.x);
    assert_eq!(t.dividers[0].rect.x + WB_SEAM, r.rect.x);
    assert_eq!(r.rect.x + r.rect.w, 800.0);
    // Paths address platen's split_fractions vocabulary.
    assert_eq!(l.path, vec![0]);
    assert_eq!(r.path, vec![1]);
    assert!(t.dividers[0].path.is_empty(), "the root split's band");
    assert_eq!(t.dividers[0].pair, 0);
}

#[test]
fn hairline_dividers_keep_a_wide_grab_band_on_both_axes() {
    for axis in [Axis::Row, Axis::Column] {
        let rect = if axis == Axis::Row {
            Rect::new(400.0, 0.0, 1.0, 600.0)
        } else {
            Rect::new(0.0, 300.0, 800.0, 1.0)
        };
        let tiling = WorkbenchTiling {
            cells: vec![],
            dividers: vec![WbDivider {
                index: 0,
                rect,
                area: area(),
                path: vec![],
                pair: 0,
                axis,
                fractions: vec![0.5, 0.5],
            }],
        };
        for offset in [-5.0, 0.0, 5.0] {
            let (x, y) = if axis == Axis::Row {
                (400.5 + offset, 100.0)
            } else {
                (100.0, 300.5 + offset)
            };
            assert!(tiling.divider_at(x, y).is_some());
        }
        let (x, y) = if axis == Axis::Row {
            (407.0, 100.0)
        } else {
            (100.0, 307.0)
        };
        assert!(tiling.divider_at(x, y).is_none());
    }
}

#[test]
fn a_stack_is_one_cell_with_its_active_tab() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut wb = Workbench::new();
    wb.ensure_tiled();
    wb.open_stack(&[a, b]);
    wb.activate(b);
    let t = place_workbench(geom_of(&wb).as_ref(), area());
    assert_eq!(t.cells.len(), 1);
    assert!(t.dividers.is_empty());
    assert_eq!(t.cells[0].members, vec![a, b]);
    assert_eq!(t.cells[0].active, 1);
    assert_eq!(t.cells[0].active_member(), Some(b));
    // The body sits below the tab bar.
    assert_eq!(t.cells[0].body().y, crate::ui::TABLIST_HEIGHT);
}

/// The model's own gesture (stack a dragged tile onto a target) walks into
/// one fewer cell — the geometry follows the model, never its own state.
#[test]
fn stacking_via_the_model_collapses_a_cell() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut wb = Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(a);
    wb.open_tile(b);
    assert!(wb.move_to_slot_of(b, a));
    let t = place_workbench(geom_of(&wb).as_ref(), area());
    assert_eq!(t.cells.len(), 1);
    assert_eq!(t.cells[0].members, vec![a, b]);
    assert_eq!(t.cells[0].active, 1, "the dragged tab lands active");
}

#[test]
fn drag_fractions_reweight_the_pair_and_clamp() {
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let mut wb = Workbench::new();
    wb.ensure_tiled();
    wb.open_tile(a);
    wb.open_tile(b);
    let t = place_workbench(geom_of(&wb).as_ref(), area());
    let div = &t.dividers[0];
    // Pointer at 3/4 of the content width: the pair re-weights toward it.
    let fr = drag_fractions(div, 0.75 * (800.0 - WB_SEAM) + 0.0, 300.0);
    assert_eq!(fr.len(), 2);
    assert!((fr[0] - 0.75).abs() < 0.02, "{fr:?}");
    assert!((fr.iter().sum::<f32>() - 1.0).abs() < 1e-4);
    // Dragged past the edge: clamped so neither side collapses.
    let fr = drag_fractions(div, 5000.0, 300.0);
    assert!(fr[1] >= 0.09, "{fr:?}");
    // The model accepts the write through the divider's own path.
    wb.set_split_fractions(&div.path, &fr);
    assert_eq!(wb.split_fractions(&[]).unwrap().len(), 2);
}

/// The drop zones: on a DIFFERENT cell, the tab bar and body centre
/// stack and the edge bands split beside; on the OWN cell the edges split
/// out and everything else is a click (`None`). The gesture's whole
/// decision table, headless.
#[test]
fn drop_zones_resolve_stack_and_edge_splits() {
    use crate::action::{Action, WbAxis};
    let (d, t) = (Uuid::new_v4(), Uuid::new_v4());
    let other_cell = CellPlacement {
        chrome_scale: 1.0,
        reading_toolbar_height: 0.0,
        tab_edge: Default::default(),
        tab_policy: Default::default(),
        members: vec![t],
        active: 0,
        rect: Rect::new(100.0, 50.0, 400.0, 430.0),
        path: vec![],
    };
    let body = other_cell.body();
    let stack = |lx: f32, ly: f32| {
        matches!(
            wb_drop_action(d, t, &other_cell, lx, ly),
            Some(Action::WorkbenchStackOnto { .. })
        )
    };
    // Tab bar and body centre: stack.
    assert!(stack(300.0, other_cell.rect.y + 10.0));
    assert!(stack(body.x + body.w / 2.0, body.y + body.h / 2.0));
    // Each edge band: split beside on that side.
    let split_at = |lx: f32, ly: f32| match wb_drop_action(d, t, &other_cell, lx, ly) {
        Some(Action::WorkbenchSplitBeside { axis, after, .. }) => Some((axis, after)),
        _ => None,
    };
    assert_eq!(
        split_at(body.x + body.w * 0.1, body.y + body.h * 0.5),
        Some((WbAxis::Row, false)),
        "left band"
    );
    assert_eq!(
        split_at(body.x + body.w * 0.9, body.y + body.h * 0.5),
        Some((WbAxis::Row, true)),
        "right band"
    );
    assert_eq!(
        split_at(body.x + body.w * 0.5, body.y + body.h * 0.1),
        Some((WbAxis::Column, false)),
        "top band"
    );
    assert_eq!(
        split_at(body.x + body.w * 0.5, body.y + body.h * 0.9),
        Some((WbAxis::Column, true)),
        "bottom band"
    );
    // The OWN cell (a stack holding the dragged tab): edges split OUT,
    // the centre and tab bar are a click.
    let own_cell = CellPlacement {
        chrome_scale: 1.0,
        reading_toolbar_height: 0.0,
        tab_edge: Default::default(),
        tab_policy: Default::default(),
        members: vec![d, t],
        active: 1,
        rect: other_cell.rect,
        path: vec![],
    };
    assert!(matches!(
        wb_drop_action(
            d,
            t,
            &own_cell,
            body.x + body.w * 0.5,
            body.y + body.h * 0.9
        ),
        Some(Action::WorkbenchSplitOut {
            axis: WbAxis::Column,
            after: true,
            ..
        })
    ));
    assert!(
        wb_drop_action(
            d,
            t,
            &own_cell,
            body.x + body.w / 2.0,
            body.y + body.h / 2.0
        )
        .is_none(),
        "own-cell centre is a click, not a drop"
    );
    assert!(
        wb_drop_action(d, t, &own_cell, 300.0, own_cell.rect.y + 10.0).is_none(),
        "own-cell tab bar is a click"
    );
}

/// A nested split walks into nested rects with the inner divider carrying
/// the inner path. The geometry is hand-built (a Row of [leaf, Column of
/// two leaves]) — the canonical serde tree platen persists, so a restored
/// nested layout walks the same way a gestured one does.
#[test]
fn nested_splits_carry_nested_paths() {
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let geom = TreeGeometry::Split {
        axis: Axis::Row,
        children: vec![
            TreeBranch {
                fraction: 0.5,
                node: TreeGeometry::leaf(a),
            },
            TreeBranch {
                fraction: 0.5,
                node: TreeGeometry::Split {
                    axis: Axis::Column,
                    children: vec![
                        TreeBranch {
                            fraction: 0.5,
                            node: TreeGeometry::leaf(b),
                        },
                        TreeBranch {
                            fraction: 0.5,
                            node: TreeGeometry::leaf(c),
                        },
                    ],
                },
            },
        ],
    };
    let t = place_workbench(Some(&geom), area());
    assert_eq!(t.cells.len(), 3);
    assert_eq!(t.dividers.len(), 2);
    let inner = t
        .dividers
        .iter()
        .find(|d| !d.path.is_empty())
        .expect("the nested split has its own band");
    assert_eq!(
        inner.path,
        vec![1],
        "the Column split is the root's child 1"
    );
    assert!(matches!(inner.axis, Axis::Column));
    // The two Column cells share the right half, stacked vertically.
    let right_cells: Vec<_> = t
        .cells
        .iter()
        .filter(|cl| cl.path.starts_with(&[1]))
        .collect();
    assert_eq!(right_cells.len(), 2);
    assert_eq!(right_cells[0].rect.x, right_cells[1].rect.x);
    assert!(right_cells[0].rect.y < right_cells[1].rect.y);
}
