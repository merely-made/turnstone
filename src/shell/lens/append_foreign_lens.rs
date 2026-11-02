// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(in super::super) fn append_foreign_lens(
    frame: &mut super::super::document_a11y::Projected,
    forest: &crate::foreign_a11y::ForeignA11y,
    placements: Vec<(uuid::Uuid, accesskit::Rect)>,
    focus: crate::surface::FocusTarget,
) -> Vec<uuid::Uuid> {
    let mut visible = Vec::new();
    for (node, rect) in placements {
        let Some(mut graft) = forest.graft(node, rect) else {
            continue;
        };
        graft.set_label("browser page");
        let graft_id = uxtree::node_id_for_path(&format!("turnstone/doc/{node}"));
        // Replace any structural outline, retaining its existing pane parent.
        let mut pending = frame
            .tree
            .nodes
            .iter()
            .find(|(id, _)| *id == graft_id)
            .map(|(_, n)| n.children().to_vec())
            .unwrap_or_default();
        let mut retired = std::collections::HashSet::new();
        while let Some(child) = pending.pop() {
            if retired.insert(child) {
                if let Some((_, n)) = frame.tree.nodes.iter().find(|(id, _)| *id == child) {
                    pending.extend(n.children().iter().copied());
                }
            }
        }
        frame.tree.nodes.retain(|(id, _)| !retired.contains(id));
        frame.routes.retain(|id, _| !retired.contains(id));
        if let Some((_, old)) = frame.tree.nodes.iter_mut().find(|(id, _)| *id == graft_id) {
            *old = graft;
        } else {
            if let Some((_, root)) = frame
                .tree
                .nodes
                .iter_mut()
                .find(|(id, _)| *id == frame.tree.root)
            {
                let mut children = root.children().to_vec();
                children.push(graft_id);
                root.set_children(children);
            }
            frame.tree.nodes.push((graft_id, graft));
        }
        if matches!(focus, crate::surface::FocusTarget::Content { node: current, .. } if current == node)
        {
            frame.focus = Some(graft_id);
        }
        visible.push(node);
    }
    visible
}
