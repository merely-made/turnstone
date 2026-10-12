// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Painted input ownership and paging for the retained Graphshell presenter.

/// A painted UI rectangle: left, top, width, height.
pub type Rect = (f32, f32, f32, f32);

// These classes belong to retained panels which keep their host input even
// when they overlap the graph. Read painted rectangles rather than layout or
// browser mirror bounds; inspectors can have multiple visible instances.
const OVERLAY_CLASSES: &[&str] = &[
    "tools-overlay",
    "tree-product",
    "tree-projection",
    "tree-command-menu",
    "tree-inspection",
    "tree-rules",
];

/// Collect every painted instance of each retained input owner.
pub fn painted_overlays<I: IntoIterator<Item = Rect>>(
    mut painted: impl FnMut(&str) -> I,
) -> Vec<Rect> {
    OVERLAY_CLASSES
        .iter()
        .flat_map(|class| painted(class))
        .collect()
}

fn inside((left, top, width, height): Rect, (x, y): (f32, f32)) -> bool {
    x >= left && y >= top && x < left + width && y < top + height
}

/// Return graph-local coordinates only where the graph owns the point.
pub fn graph_point(graph: Option<Rect>, overlays: &[Rect], at: (f32, f32)) -> Option<(f32, f32)> {
    let graph = graph?;
    if !inside(graph, at) || overlays.iter().any(|rect| inside(*rect, at)) {
        return None;
    }
    Some((at.0 - graph.0, at.1 - graph.1))
}

/// Keep a selected page within the newly computed matrix on both axes.
pub fn clamp_comparison_page(
    (column, row): (usize, usize),
    columns: usize,
    rows: usize,
) -> (usize, usize) {
    (
        column.min(columns.saturating_sub(1)),
        row.min(rows.saturating_sub(1)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAPH: Rect = (20., 30., 600., 400.);

    #[test]
    fn projection_and_command_menu_keep_wheel_and_touch_from_graph() {
        for owner in ["tree-projection", "tree-command-menu"] {
            let overlays = painted_overlays(|class| {
                if class == owner {
                    vec![(100., 100., 80., 60.)]
                } else {
                    vec![]
                }
            });
            assert_eq!(
                graph_point(Some(GRAPH), &overlays, (110., 110.)),
                None,
                "{owner}"
            );
            assert_eq!(
                graph_point(Some(GRAPH), &overlays, (300., 200.)),
                Some((280., 170.))
            );
        }
    }

    #[test]
    fn inspector_and_rule_panels_keep_their_painted_input() {
        for owner in ["tree-inspection", "tree-rules"] {
            let overlays = painted_overlays(|class| {
                if class == owner {
                    vec![(200., 150., 120., 80.)]
                } else {
                    vec![]
                }
            });
            assert_eq!(
                graph_point(Some(GRAPH), &overlays, (220., 160.)),
                None,
                "{owner}"
            );
        }
    }

    #[test]
    fn every_painted_instance_of_an_owner_is_excluded() {
        let overlays = painted_overlays(|class| {
            if class == "tree-product" {
                vec![(40., 50., 40., 40.), (140., 150., 40., 40.)]
            } else {
                vec![]
            }
        });
        assert_eq!(graph_point(Some(GRAPH), &overlays, (150., 160.)), None);
    }

    #[test]
    fn graph_and_page_points_keep_existing_half_open_bounds() {
        let overlays = painted_overlays(|class| {
            if class == "tools-overlay" {
                vec![(40., 50., 40., 40.)]
            } else {
                vec![]
            }
        });
        assert_eq!(
            graph_point(Some(GRAPH), &overlays, (20., 30.)),
            Some((0., 0.))
        );
        assert_eq!(graph_point(Some(GRAPH), &overlays, (45., 55.)), None);
        assert_eq!(
            graph_point(Some(GRAPH), &overlays, (80., 90.)),
            Some((60., 60.))
        );
        assert_eq!(graph_point(Some(GRAPH), &overlays, (620., 100.)), None);
        assert_eq!(graph_point(Some(GRAPH), &overlays, (0., 0.)), None);
        assert_eq!(graph_point(None, &[], (110., 110.)), None);
    }

    #[test]
    fn later_dynamics_page_clamps_to_smaller_arrangement_matrix() {
        assert_eq!(clamp_comparison_page((4, 5), 3, 1), (2, 0));
        assert_eq!(clamp_comparison_page((2, 0), 2, 1), (1, 0));
    }

    #[test]
    fn empty_axes_cannot_underflow_or_leave_stale_pages() {
        assert_eq!(
            clamp_comparison_page((usize::MAX, usize::MAX), 0, 0),
            (0, 0)
        );
        assert_eq!(clamp_comparison_page((1, 5), 2, 1), (1, 0));
    }

    #[test]
    fn valid_matrix_pages_are_preserved() {
        assert_eq!(clamp_comparison_page((1, 2), 4, 8), (1, 2));
        assert_eq!(clamp_comparison_page((0, 0), 1, 1), (0, 0));
    }
}
