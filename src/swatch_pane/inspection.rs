// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Structural accessibility tree for the swatch's read-only inspection rows.

use apparatus_core::Inspection;

/// Project inspection values without adding focus, bounds, or actions.
///
/// The swatch supplies retained DOM bounds and the pane-scoped root identity.
pub(super) fn project_inspection(inspection: &Inspection) -> uxtree::UxTree {
    use accesskit::{Node, Role};
    use uxtree::node_id_for_path;

    let root_id = node_id_for_path("apparatus/inspection");
    let mut nodes = Vec::with_capacity(inspection.lines.len() + 1);
    let mut children = Vec::with_capacity(inspection.lines.len());
    for line in &inspection.lines {
        let id = node_id_for_path(&line.id);
        let mut node = Node::new(Role::Label);
        node.set_label(line.text.clone());
        children.push(id);
        nodes.push((id, node));
    }
    let mut root = Node::new(Role::Group);
    root.set_label("Diagnostics");
    root.set_children(children);
    nodes.push((root_id, root));
    uxtree::UxTree {
        root: root_id,
        nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apparatus_core::InspectionLine;

    #[test]
    fn structural_projection_keeps_all_readings_without_actions() {
        let inspection = Inspection {
            lines: vec![
                InspectionLine {
                    id: "record/1".into(),
                    text: "Cause: unavailable".into(),
                },
                InspectionLine {
                    id: "record/2".into(),
                    text: "Source: retained".into(),
                },
            ],
        };
        let tree = project_inspection(&inspection);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| { node.label() == Some("Cause: unavailable") })
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| { node.label() == Some("Source: retained") })
        );
        assert!(tree.nodes.iter().all(|(_, node)| {
            !node.supports_action(accesskit::Action::Click)
                && !node.supports_action(accesskit::Action::Focus)
        }));
    }
}
