// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The current app-owned refusal, independent of the retained DOM's last sync.
/// This read-only status uses pane bounds; Arrange controls remain unqualified.
pub(crate) fn physics_refusal_tree(
    app: &App,
    pane: crate::panes::PaneId,
    placement: crate::surface::Rect,
) -> Option<uxtree::UxTree> {
    let refusal = app.physics_refusal()?;
    let id = uxtree::node_id_for_path(&format!("turnstone/arrange/{}/physics-refusal", pane.0));
    let mut node = accesskit::Node::new(accesskit::Role::Status);
    node.set_label(format!("Physics refused: {refusal}"));
    node.set_live(accesskit::Live::Polite);
    node.set_bounds(accesskit::Rect::new(
        f64::from(placement.x),
        f64::from(placement.y),
        f64::from(placement.x + placement.w),
        f64::from(placement.y + placement.h),
    ));
    Some(uxtree::UxTree {
        root: id,
        nodes: vec![(id, node)],
    })
}
