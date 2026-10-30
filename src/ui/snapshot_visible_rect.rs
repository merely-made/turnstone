// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Same inner-border clip used by the owned Cambium paint geometry. Turnstone
/// retains its own fragments, so this reads those fragments rather than creating
/// another OwnedLayout with a separate cascade or font ledger.
pub(super) fn snapshot_visible_rect<D>(
    dom: &D,
    snapshot: &LiverySnapshot<D::NodeId>,
    node: D::NodeId,
    element_scroll: &HashMap<D::NodeId, (f32, f32)>,
    viewport_scroll: (f32, f32),
    viewport: (u32, u32),
) -> Option<NodeRect>
where
    D: LayoutDom,
    D::NodeId: Copy + Eq + Hash,
{
    if snapshot
        .styles
        .computed_style(node, "visibility")
        .is_some_and(|value| matches!(value.as_str(), "hidden" | "collapse"))
    {
        return None;
    }
    let mut rect = snapshot_node_rect(dom, snapshot, node, element_scroll, viewport_scroll)?;
    let mut ancestor = dom.parent(node);
    while let Some(parent) = ancestor {
        // Genet's paint policy clips the inner border box on both axes when either
        // overflow axis clips. Keep semantic visibility aligned with that policy.
        let clips = |property| {
            snapshot
                .styles
                .computed_style(parent, property)
                .is_some_and(|value| value != "visible")
        };
        if clips("overflow-x") || clips("overflow-y") {
            let (x, y, width, height) =
                snapshot_node_rect(dom, snapshot, parent, element_scroll, viewport_scroll)?;
            let px = |property| {
                snapshot
                    .styles
                    .computed_style(parent, property)
                    .and_then(|value| value.strip_suffix("px").and_then(|v| v.parse::<f32>().ok()))
                    .unwrap_or_default()
            };
            let (left, right) = (px("border-left-width"), px("border-right-width"));
            let (top, bottom) = (px("border-top-width"), px("border-bottom-width"));
            rect = intersect_rect(
                rect,
                (
                    x + left,
                    y + top,
                    (width - left - right).max(0.0),
                    (height - top - bottom).max(0.0),
                ),
            )?;
        }
        ancestor = dom.parent(parent);
    }
    intersect_rect(rect, (0.0, 0.0, viewport.0 as f32, viewport.1 as f32))
}
