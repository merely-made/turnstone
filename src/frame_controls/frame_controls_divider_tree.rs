// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl FrameControls {
    pub(super) fn divider_tree(
        &self,
        control: &Control,
    ) -> Option<(workbench::SplitAxis, &[workbench::TileBranch])> {
        let Control::Divider {
            path,
            index,
            left,
            right,
        } = control
        else {
            return None;
        };
        let TileTree::Split { axis, children } =
            tree_at(self.runner.state().workspace.tiled(), path)?
        else {
            return None;
        };
        let ids = |t: &TileTree| t.tiles().into_iter().map(|t| t.id).collect::<Vec<_>>();
        if ids(&children.get(*index)?.tree) != *left
            || ids(&children.get(index + 1)?.tree) != *right
        {
            return None;
        }
        Some((*axis, children))
    }

    pub(super) fn divider(&self, path: &TilePath, index: usize) -> Option<Control> {
        let TileTree::Split { children, .. } =
            tree_at(self.runner.state().workspace.tiled(), path)?
        else {
            return None;
        };
        let ids = |t: &TileTree| t.tiles().into_iter().map(|t| t.id).collect();
        Some(Control::Divider {
            path: path.clone(),
            index,
            left: ids(&children.get(index)?.tree),
            right: ids(&children.get(index + 1)?.tree),
        })
    }

    pub(super) fn enhance(&self) {
        let controls: Vec<_> = nodes(&self.dom.borrow(), self.root())
            .into_iter()
            .map(|n| (n, self.control_at(n)))
            .collect();
        let mut dom = self.dom.borrow_mut();
        for (node, control) in controls {
            if matches!(control, Some(Control::Body { .. })) {
                continue;
            }
            if let Some(Control::Divider { index, .. }) = control.as_ref() {
                if let Some((_, children)) = self.divider_tree(control.as_ref().unwrap()) {
                    let pair = children[*index].fraction + children[*index + 1].fraction;
                    for (name, value) in [
                        ("tabindex", "0".into()),
                        ("aria-valuemin", "5".into()),
                        ("aria-valuemax", "95".into()),
                        (
                            "aria-valuenow",
                            (100.0 * children[*index].fraction / pair).to_string(),
                        ),
                        (
                            "aria-label",
                            format!("Resize panes at divider {}", index + 1),
                        ),
                    ] {
                        if attr(&dom, node, name).as_deref() != Some(&value) {
                            dom.set_attribute(node, qual(name), &value);
                        }
                    }
                }
            }
            if let Some(id) = attr(&dom, node, cambium::FRISKET_TILE_ATTR)
                .filter(|_| cambium::slot_kind(&*dom, node).is_some())
            {
                let label = id
                    .parse::<u64>()
                    .ok()
                    .and_then(|id| self.runner.state().workspace.find(TileId(id)))
                    .map(|tile| format!("{} pane content", tile.title))
                    .unwrap_or_else(|| "Pane content".into());
                if attr(&dom, node, "role").as_deref() != Some("group") {
                    dom.set_attribute(node, qual("role"), "group");
                }
                if attr(&dom, node, "aria-label").as_deref() != Some(label.as_str()) {
                    dom.set_attribute(node, qual("aria-label"), &label);
                }
            }
        }
    }
}
